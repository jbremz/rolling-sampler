use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use eframe::{run_native, App, NativeOptions, CreationContext};
use egui::{CentralPanel, Vec2b};
use std::error::Error;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, SampleFormat, StreamConfig};
use hound::{WavWriter, WavSpec, SampleFormat as HoundSampleFormat};
use rfd::FileDialog;
use chrono::Utc;
use egui_plot::{Line, Plot, PlotUi, PlotPoints};
use dirs::home_dir;


struct Recorder {
    is_grabbing: Arc<AtomicBool>,
    sample_buffer: Arc<Mutex<CircularBuffer>>,
    stream: Option<cpal::Stream>,
    config: StreamConfig,
    buffer_size: Arc<Mutex<usize>>,
    save_path: Option<String>,
    devices: Vec<Device>,                // Store available input devices
    current_device_index: usize,          // Store the index of the selected device
}
struct CircularBuffer {
    circular_buffer: Vec<f32>,
    static_buffer: Vec<f32>,
    max_size: usize,
    write_pos: usize,
    current_size: usize,
    is_static_mode: bool,
}

fn get_file_safe_timestamp() -> String {
    // Get the current time in UTC
    let now = Utc::now();

    // Format the time as "2024-09-10_15-06-32" (safe for file paths)
    now.format("%Y-%m-%d_%H-%M-%S").to_string()
}


impl Recorder {
    fn new(initial_buffer_size: usize) -> Self {
        let host = cpal::default_host();
        let devices: Vec<Device> = host.input_devices().unwrap().collect();  // Fetch available devices

        let current_device_index = 0; // Set default to the first device
        let input_device = devices[current_device_index].clone();
        let config = input_device.default_input_config().expect("Failed to get default input config");
        let config: StreamConfig = config.into();
        let initial_buffer_size = initial_buffer_size * config.sample_rate.0 as usize;

        // Resolve the Desktop path and convert it to a String
        let save_path: Option<String> = home_dir().map(|mut path| {
            path.push("Desktop");
            path.to_str().map(|s| s.to_owned()) // Convert to String
        }).flatten(); // Flatten the Option<Option<String>> to Option<String>

        let mut recorder = Recorder {
            is_grabbing: Arc::new(AtomicBool::new(false)),
            sample_buffer: Arc::new(Mutex::new(CircularBuffer::new(initial_buffer_size))),
            stream: None,
            config,
            buffer_size: Arc::new(Mutex::new(initial_buffer_size)),
            save_path,
            devices,
            current_device_index,
        };

        recorder.start_recording();
        recorder
    }

    fn start_recording(&mut self) {

        // Get the currently selected device
        let input_device = self.devices[self.current_device_index].clone();

        // Fetch the latest configuration
        let config = input_device.default_input_config().expect("Failed to get default input config");
        let config: StreamConfig = config.into();
        self.config = config;
        let sample_format = input_device.default_input_config().unwrap().sample_format();

        self.reset_buffer(); // Reset the buffer before starting a new recording
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

        // Stop the previous stream if it exists
        if let Some(old_stream) = self.stream.take() {
            drop(old_stream);
        }

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
        println!("Sample rate: {}", self.config.sample_rate.0);
    
        // Prepare WAV writer specifications based on the buffer content
        let spec = WavSpec {
            channels: self.config.channels,
            sample_rate: self.config.sample_rate.0,
            bits_per_sample: 32, // Assuming f32 for this example
            sample_format: HoundSampleFormat::Float,
        };
    
        let filepath = format!("{}/{}.wav", self.save_path.as_ref().unwrap(), get_file_safe_timestamp());
        let mut writer = WavWriter::create(filepath, spec).expect("Failed to create WAV writer");
    
        // Save buffer
        for &sample in buffer.static_buffer.iter().take(buffer.current_size) {
            writer.write_sample(sample).unwrap();
        }
        
        writer.finalize().expect("Failed to finalize WAV writer");
    
        println!("Recording saved!");
    
        // Replace the buffer with a new one rather than clearing the old one
        drop(buffer);  // Unlock the mutex before replacing the buffer

        // Restart recording with a fresh stream
        self.start_recording();
    }
    

    fn update_buffer_size(&mut self, new_size: usize) {
        {
            // Update the buffer size in the Arc<Mutex<usize>>
            let mut buffer_size = self.buffer_size.lock().unwrap();
            *buffer_size = new_size;
            // The lock (buffer_size) will be dropped at the end of this scope
        }
    
        // Reset the buffer after the lock on buffer_size has been released
        self.reset_buffer();
    }
    

    fn open_file_dialog(&mut self) {
        if let Some(path) = FileDialog::new().pick_folder() {
            // Store the selected directory path
            self.save_path = Some(path.display().to_string());
            println!("Save directory selected: {}", self.save_path.as_ref().unwrap());
        }
    }

    fn reset_buffer(&mut self) {
        // Lock the current buffer size to reuse it
        let new_buffer_size = *self.buffer_size.lock().unwrap();

        // Replace the old buffer with a new one
        self.sample_buffer = Arc::new(Mutex::new(CircularBuffer::new(new_buffer_size)));
    }
}

impl CircularBuffer {
    fn new(max_size: usize) -> Self {
        CircularBuffer {
            circular_buffer: Vec::with_capacity(max_size),
            static_buffer: Vec::new(), // Start with an empty static buffer
            max_size,
            write_pos: 0,
            current_size: 0,
            is_static_mode: false,
        }
    }

    fn add_samples(&mut self, samples: &[f32]) {
        if self.is_static_mode {
            // In static mode, add samples to the static buffer
            self.static_buffer.extend_from_slice(samples);
            self.current_size += samples.len();
        } else {
            // In circular mode, overwrite old data if necessary
            for &sample in samples {
                if self.current_size < self.max_size {
                    self.circular_buffer.push(sample);
                    self.current_size += 1;
                } else {
                    self.circular_buffer[self.write_pos] = sample;
                }
                self.write_pos = (self.write_pos + 1) % self.max_size;
            }
        }
    }

    fn start_static_mode(&mut self) {
        self.is_static_mode = true;

        println!("Transitioning to static mode");

        // Copy the contents of the circular buffer to the static buffer
        if self.current_size < self.max_size {
            self.static_buffer.extend_from_slice(&self.circular_buffer[..self.current_size]);
        } else {
            self.static_buffer.extend_from_slice(&self.circular_buffer[self.write_pos..]);
            self.static_buffer.extend_from_slice(&self.circular_buffer[..self.write_pos]);
        }
    }

    // Method to get the current samples for plotting (whether static or circular)
    fn get_samples_for_plot(&self) -> Vec<f32> {
        if self.is_static_mode {
            self.static_buffer.clone()
        } else {
            if self.current_size < self.max_size {
                self.circular_buffer.clone()
            } else {
                let mut plot_samples = Vec::with_capacity(self.max_size);
                plot_samples.extend_from_slice(&self.circular_buffer[self.write_pos..]);
                plot_samples.extend_from_slice(&self.circular_buffer[..self.write_pos]);
                plot_samples
            }
        }
    }
}


impl App for Recorder {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {

        // Repaint the UI to update the plot
        ctx.request_repaint();

        CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0); // Add some space at the top
                let panel_width = ui.available_width();

                // Center the contents inside the horizontal layout
                ui.vertical_centered(|ui| {

                    // Fetch the audio buffer samples for plotting
                    if let Ok(buffer) = self.sample_buffer.lock() {
                        let plot_data = buffer.get_samples_for_plot();

                        // Set your desired downsampling factor (e.g., take every 10th sample)
                        let downsample_factor = 10;

                        // Create plot points as Vec<[f64; 2]> with downsampling
                        let points: Vec<[f64; 2]> = plot_data
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| i % downsample_factor == 0) // Pick every Nth sample
                            .map(|(i, &sample)| [i as f64, sample as f64])  // Create [x, y] pairs
                            .collect();

                        let plot_points = PlotPoints::new(points);

                        // Create a line from the points
                        let line = Line::new(plot_points);

                        // Display the plot
                        Plot::new("Rolling Waveform Plot")
                            .view_aspect(4.0)  // Adjust aspect ratio if necessary
                            .auto_bounds(Vec2b::new(true, false))  // Disable auto bounds for y-axis, keep x-axis auto-bounds
                            .show_axes(false)
                            .show_grid(false)
                            .show_background(false)
                            .allow_zoom(false)
                            .allow_drag(false)
                            .allow_scroll(false)
                            .show(ui, |plot_ui: &mut PlotUi| {
                                plot_ui.line(line);
                            });
                    }
                    
                    // Slider to control buffer size
                    let mut buffer_size = *self.buffer_size.lock().unwrap();

                    // because slider contains subwidgets, this alignment doesn't work
                    ui.horizontal_centered(|ui| {
                        let desired_width = panel_width * 0.8;
                        ui.style_mut().spacing.slider_width = desired_width;

                        // Convert buffer size from samples to seconds for the slider display
                        let buffer_size_seconds = buffer_size as f32 / self.config.sample_rate.0 as f32;
                        let max_buffer_seconds = 60.0; // Maximum 60 seconds for the slider
                        let mut new_buffer_size_seconds = buffer_size_seconds;

                        let response = ui.add(egui::Slider::new(&mut new_buffer_size_seconds, 1.0..=max_buffer_seconds)
                        .text("Buffer Size (s)"));

                        let new_buffer_size = (new_buffer_size_seconds * self.config.sample_rate.0 as f32) as usize;

                        if response.drag_stopped() && new_buffer_size != buffer_size {
                            buffer_size = new_buffer_size;
                            self.update_buffer_size(buffer_size);
                            self.start_recording();
                        }

                    });

                    ui.add_space(20.0); // Add some space between the slider and the button

                    // File path selection button
                    if ui.button("Select Save Folder").clicked() {
                        self.open_file_dialog(); // Open the native file dialog
                    }

                    if let Some(path) = &self.save_path {
                        ui.label(format!("Selected Folder: {}", path));
                    }

                    // ui.add_space(20.0); // Add some space between the path selector and the button

                    // Device selection dropdown
                    ui.label("Input Device:");
                    let current_device_index = self.current_device_index; // Store the current device index for later comparison
                    egui::ComboBox::from_id_source("Device")  // Using an ID instead of a label
                    .selected_text(self.devices[self.current_device_index].name().unwrap_or_default().clone())
                    .show_ui(ui, |ui| {
                        for (idx, device) in self.devices.iter().enumerate() {
                            ui.selectable_value(&mut self.current_device_index, idx, device.name().unwrap_or_default());
                        }
                    });

                    // Check if the selected device has changed
                    if current_device_index != self.current_device_index {
                        // Stop current recording
                        if let Some(stream) = self.stream.take() {
                            drop(stream);
                        }
                        // Update config for new device
                        let new_device = &self.devices[self.current_device_index];
                        self.config = new_device.default_input_config().expect("Failed to get default input config").into();
                        // Start recording with new device
                        self.start_recording();
                    }

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
                        } else {
                            println!("Start grab button clicked");
                            let mut buffer = self.sample_buffer.lock().unwrap();
                            buffer.start_static_mode(); // Transition the buffer to static mode
                            self.is_grabbing.store(true, Ordering::SeqCst);
                        }
                    }
                });
            });
        });
    }
}



fn err_fn(err: cpal::StreamError) {
    eprintln!("An error occurred on the input stream: {}", err);
}

fn main() -> Result<(), Box<dyn Error>> {
    let app_name = "Rolling Buffer Recorder";
    let native_options = NativeOptions::default();
    let app_creator = move |_cc: &CreationContext| -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>> {
        Ok(Box::new(Recorder::new(5))) // Initialize with a buffer size of 44100 samples
    };
    run_native(app_name, native_options, Box::new(app_creator))?;
    Ok(())
}