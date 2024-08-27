mod gui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, StreamConfig, StreamError};
use hound::{WavWriter, WavSpec, SampleFormat as HoundSampleFormat};

enum SampleData {
    F32(Vec<f32>),
    I16(Vec<i16>),
    U16(Vec<u16>),
}

fn main() {
    let host = cpal::default_host();
    let input_device = host.default_input_device().expect("No input device available");
    let config = input_device.default_input_config().expect("Failed to get default input config");
    let sample_format = config.sample_format();
    let config: StreamConfig = config.into();

    let sample_buffer = Arc::new(Mutex::new(SampleData::F32(Vec::new())));
    let is_recording = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let sample_buffer_clone = Arc::clone(&sample_buffer);
    let is_recording_clone = Arc::clone(&is_recording);
    let stop_flag_clone = Arc::clone(&stop_flag);

    let stream = match sample_format {
        SampleFormat::F32 => {
            input_device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if is_recording_clone.load(Ordering::SeqCst) {
                        let mut buffer = sample_buffer_clone.lock().unwrap();
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
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if is_recording_clone.load(Ordering::SeqCst) {
                        let mut buffer = sample_buffer_clone.lock().unwrap();
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
                &config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    if is_recording_clone.load(Ordering::SeqCst) {
                        let mut buffer = sample_buffer_clone.lock().unwrap();
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

    let gui_thread = thread::spawn(move || {
        gui::run_gui(is_recording, stop_flag);
    });

    loop {
        thread::sleep(Duration::from_secs(1));
        if stop_flag_clone.load(Ordering::SeqCst) {
            break;
        }
    }

    drop(stream);

    let buffer = sample_buffer.lock().unwrap();
    let num_samples = match &*buffer {
        SampleData::F32(data) => data.len(),
        SampleData::I16(data) => data.len(),
        SampleData::U16(data) => data.len(),
    };

    let num_channels = config.channels as usize;
    println!("Recorded shape: ({}, {})", num_samples / num_channels, num_channels);

    match &*buffer {
        SampleData::F32(data) => {
            let spec = WavSpec {
                channels: config.channels,
                sample_rate: config.sample_rate.0,
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
                channels: config.channels,
                sample_rate: config.sample_rate.0,
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
                channels: config.channels,
                sample_rate: config.sample_rate.0,
                bits_per_sample: 16,
                sample_format: HoundSampleFormat::Int,
            };
            let mut writer = WavWriter::create("output.wav", spec).unwrap();
            for &sample in data.iter() {
                writer.write_sample(sample as i16).unwrap();  // U16 values are written as I16
            }
        }
    }

    println!("Recording saved!");

    gui_thread.join().unwrap();
}

fn err_fn(err: StreamError) {
    eprintln!("An error occurred on the input stream: {}", err);
}
