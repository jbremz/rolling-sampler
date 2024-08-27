use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use eframe::{run_native, App, NativeOptions, CreationContext};
use egui::CentralPanel;
use std::error::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavWriter, WavSpec, SampleFormat as HoundSampleFormat};

struct Recorder {
    is_grabbing: Arc<AtomicBool>,
    sample_buffer: Arc<Mutex<CircularBuffer>>,
    stream: Option<cpal::Stream>,
    config: StreamConfig,
    buffer_size: Arc<Mutex<usize>>,
}

struct CircularBuffer {
    buffer: Vec<f32>,
    max_size: usize,
    write_pos: usize,
    is_static: bool,
    current_size: usize,
}


impl Recorder {
    fn new(initial_buffer_size: usize) -> Self {
        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device available");
        let config = input_device.default_input_config().expect("Failed to get default input config");
        let config: StreamConfig = config.into();

        let mut recorder = Recorder {
            is_grabbing: Arc::new(AtomicBool::new(false)),
            sample_buffer: Arc::new(Mutex::new(CircularBuffer::new(initial_buffer_size))),
            stream: None,
            config,
            buffer_size: Arc::new(Mutex::new(initial_buffer_size)), // Initialize buffer size
        };

        // Start recording as soon as the Recorder is created
        recorder.start_recording();

        println!("{}", initial_buffer_size);

        recorder
    }

    fn start_recording(&mut self) {
        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device available");
        let sample_format = input_device.default_input_config().unwrap().sample_format();

        let sample_buffer = Arc::clone(&self.sample_buffer);

        let stream = match sample_format {
            SampleFormat::F32 => {
                input_device.build_input_stream(
                    &self.config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let mut buffer = sample_buffer.lock().unwrap();
                        buffer.add_samples(data);
                    },
                    err_fn,
                    None,
                )
            }
            _ => panic!("Unsupported sample format"),
        }
        .unwrap();

        self.stream = Some(stream);
        self.is_grabbing.store(false, Ordering::SeqCst);
    }

    fn grab_recording(&mut self) {
        self.is_grabbing.store(false, Ordering::SeqCst);
        if let Some(stream) = self.stream.take() {
            drop(stream);  // This drops the stream and stops recording
        }

        let buffer = self.sample_buffer.lock().unwrap();

        // Determine the number of samples in the buffer
        let num_samples = buffer.current_size;
        let num_channels = self.config.channels as usize;

        println!("Recorded shape: ({}, {})", num_samples / num_channels, num_channels);

        // Prepare WAV writer specifications based on the buffer content
        let spec = WavSpec {
            channels: self.config.channels,
            sample_rate: self.config.sample_rate.0,
            bits_per_sample: 32, // Assuming f32 for this example
            sample_format: HoundSampleFormat::Float,
        };

        let mut writer = WavWriter::create("output.wav", spec).expect("Failed to create WAV writer");

        // Save all data in the buffer
        if buffer.is_static {
            for &sample in buffer.buffer.iter() {
                writer.write_sample(sample).unwrap();
            }
        } else {
            // Save data from the circular buffer, taking care of wrapping
            let start_pos = if buffer.current_size < buffer.max_size {
                0
            } else {
                buffer.write_pos
            };
            for i in 0..buffer.current_size {
                let pos = (start_pos + i) % buffer.max_size;
                writer.write_sample(buffer.buffer[pos]).unwrap();
            }
        }

        writer.finalize().expect("Failed to finalize WAV writer");

        println!("Recording saved!");

        // Clear and reset the buffer for the next session
        drop(buffer);
        self.sample_buffer.lock().unwrap().clear();
    }

    fn update_buffer_size(&mut self, new_size: usize) {
        let mut buffer_size = self.buffer_size.lock().unwrap();
        *buffer_size = new_size;

        // Update the circular buffer to the new size
        let mut buffer = self.sample_buffer.lock().unwrap();
        *buffer = CircularBuffer::new(new_size);
    }
}

impl CircularBuffer {
    fn new(max_size: usize) -> Self {
        CircularBuffer {
            buffer: Vec::with_capacity(max_size),
            max_size,
            write_pos: 0,
            is_static: false,
            current_size: 0,
        }
    }

    fn add_samples(&mut self, samples: &[f32]) {
        if self.is_static {
            // In static mode, just append to the end
            self.buffer.extend_from_slice(samples);
            self.current_size = self.buffer.len();
        } else {
            // In circular mode, overwrite old data if necessary
            for &sample in samples {
                if self.current_size < self.max_size {
                    self.buffer.push(sample);
                    self.current_size += 1;
                } else {
                    self.buffer[self.write_pos] = sample;
                }
                self.write_pos = (self.write_pos + 1) % self.max_size;
            }
        }
    }

    fn set_static(&mut self) {
        self.is_static = true;
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.write_pos = 0;
        self.is_static = false;
        self.current_size = 0;
    }
}


impl App for Recorder {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0); // Add some space at the top
                ui.heading("Recorder");
                ui.add_space(20.0); // Add some space between the heading and buttons

                ui.horizontal_centered(|ui| {
                    ui.vertical_centered(|ui| {
                        // Slider to control buffer size
                        let mut buffer_size = *self.buffer_size.lock().unwrap();
                        let mut buffer_size_seconds = buffer_size as f32 / self.config.sample_rate.0 as f32;
                        ui.add(egui::Slider::new(&mut buffer_size_seconds, 1.0..=60.0)
                            .text("Buffer Size (s)"));
                        let new_buffer_size = (buffer_size_seconds as f32 * self.config.sample_rate.0 as f32) as usize;
                        if new_buffer_size != buffer_size {
                            buffer_size = new_buffer_size;
                            self.update_buffer_size(buffer_size);
                        }

                        ui.add_space(20.0); // Add some space between the slider and the button

                        // Start/Stop Recording button
                        let record_button_text = if self.is_grabbing.load(Ordering::SeqCst) {
                            "Stop Grab"
                        } else {
                            "Start Grab"
                        };
                        
                        if ui.add_sized([100.0, 40.0], egui::Button::new(record_button_text)).clicked() {
                            if self.is_grabbing.load(Ordering::SeqCst) {
                                println!("Stop button clicked");
                                self.grab_recording();

                                // Start recording again
                                self.start_recording();
                            } else {
                                println!("Start grab button clicked");
                                let mut buffer = self.sample_buffer.lock().unwrap();
                                buffer.set_static(); // Transition the buffer to static mode
                                self.is_grabbing.store(true, Ordering::SeqCst);
                            }
                        }
                    });
                });
            });
        });
    }
}


fn err_fn(err: cpal::StreamError) {
    eprintln!("An error occurred on the input stream: {}", err);
}

fn main() -> Result<(), Box<dyn Error>> {
    let app_name = "Recorder";
    let native_options = NativeOptions::default();
    let app_creator = move |_cc: &CreationContext| -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>> {
        Ok(Box::new(Recorder::new(5*44100))) // Initialize with a buffer size of 44100 samples
    };
    run_native(app_name, native_options, Box::new(app_creator))?;
    Ok(())
}