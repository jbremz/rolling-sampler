use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use eframe::{run_native, App, NativeOptions, CreationContext};
use egui::CentralPanel;
use std::error::Error;
use std::thread;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavWriter, WavSpec, SampleFormat as HoundSampleFormat};

enum SampleData {
    F32(Vec<f32>),
    I16(Vec<i16>),
    U16(Vec<u16>),
}

struct Recorder {
    is_recording: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    sample_buffer: Arc<Mutex<SampleData>>,
    stream: Option<cpal::Stream>,
    config: StreamConfig,
}

impl Recorder {
    fn new() -> Self {
        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device available");
        let config = input_device.default_input_config().expect("Failed to get default input config");
        let config: StreamConfig = config.into();

        Recorder {
            is_recording: Arc::new(AtomicBool::new(false)),
            stop_flag: Arc::new(AtomicBool::new(false)),
            sample_buffer: Arc::new(Mutex::new(SampleData::F32(Vec::new()))),
            stream: None,
            config,
        }
    }

    fn start_recording(&mut self) {
        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device available");
        let sample_format = input_device.default_input_config().unwrap().sample_format();

        let sample_buffer = Arc::clone(&self.sample_buffer);
        let is_recording = Arc::clone(&self.is_recording);

        let stream = match sample_format {
            SampleFormat::F32 => {
                input_device.build_input_stream(
                    &self.config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if is_recording.load(Ordering::SeqCst) {
                            let mut buffer = sample_buffer.lock().unwrap();
                            if let SampleData::F32(ref mut vec) = *buffer {
                                vec.extend_from_slice(data);
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::I16 => {
                *sample_buffer.lock().unwrap() = SampleData::I16(Vec::new());
                input_device.build_input_stream(
                    &self.config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if is_recording.load(Ordering::SeqCst) {
                            let mut buffer = sample_buffer.lock().unwrap();
                            if let SampleData::I16(ref mut vec) = *buffer {
                                vec.extend_from_slice(data);
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::U16 => {
                *sample_buffer.lock().unwrap() = SampleData::U16(Vec::new());
                input_device.build_input_stream(
                    &self.config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if is_recording.load(Ordering::SeqCst) {
                            let mut buffer = sample_buffer.lock().unwrap();
                            if let SampleData::U16(ref mut vec) = *buffer {
                                vec.extend_from_slice(data);
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            _ => panic!("Unsupported sample format"),
        }
        .unwrap();

        self.stream = Some(stream);
        self.is_recording.store(true, Ordering::SeqCst);
    }

    fn stop_recording(&mut self) {
        self.is_recording.store(false, Ordering::SeqCst);
        self.stream.take();  // This drops the stream and stops recording

        let buffer = self.sample_buffer.lock().unwrap();
        let num_samples = match &*buffer {
            SampleData::F32(data) => data.len(),
            SampleData::I16(data) => data.len(),
            SampleData::U16(data) => data.len(),
        };

        let num_channels = self.config.channels as usize;
        println!("Recorded shape: ({}, {})", num_samples / num_channels, num_channels);

        match &*buffer {
            SampleData::F32(data) => {
                let spec = WavSpec {
                    channels: self.config.channels,
                    sample_rate: self.config.sample_rate.0,
                    bits_per_sample: 32,
                    sample_format: HoundSampleFormat::Float,
                };
                let mut writer = WavWriter::create("output.wav", spec).unwrap();
                for &sample in data.iter() {
                    writer.write_sample(sample).unwrap();
                }
            }
            SampleData::I16(data) => {
                let spec = WavSpec {
                    channels: self.config.channels,
                    sample_rate: self.config.sample_rate.0,
                    bits_per_sample: 16,
                    sample_format: HoundSampleFormat::Int,
                };
                let mut writer = WavWriter::create("output.wav", spec).unwrap();
                for &sample in data.iter() {
                    writer.write_sample(sample).unwrap();
                }
            }
            SampleData::U16(data) => {
                let spec = WavSpec {
                    channels: self.config.channels,
                    sample_rate: self.config.sample_rate.0,
                    bits_per_sample: 16,
                    sample_format: HoundSampleFormat::Int,
                };
                let mut writer = WavWriter::create("output.wav", spec).unwrap();
                for &sample in data.iter() {
                    writer.write_sample(sample as i16).unwrap();
                }
            }
        }

        println!("Recording saved!");
    }
}

impl App for Recorder {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.heading("Recorder");
            ui.horizontal(|ui| {
                if ui.button("Record").clicked() {
                    println!("Record button clicked");
                    self.start_recording();
                }
                if ui.button("Stop").clicked() {
                    println!("Stop button clicked");
                    self.stop_recording();
                }
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
    let app_creator = move |cc: &CreationContext| -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>> {
        Ok(Box::new(Recorder::new()))
    };
    run_native(app_name, native_options, Box::new(app_creator))?;
    Ok(())
}