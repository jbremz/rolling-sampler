use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig, StreamError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;
use hound::{WavWriter, WavSpec, SampleFormat as HoundSampleFormat};

enum SampleData {
    F32(Vec<f32>),
    I16(Vec<i16>),
    U16(Vec<u16>),
}

fn main() {
    let host = cpal::default_host();

    let input_device = host.default_input_device().expect("No input device available");
    println!("Default input device: {:?}", input_device.name().unwrap());

    // Get the default input stream configuration
    let config = input_device.default_input_config()
    .expect("Failed to get default input config");
    println!("Default input stream configuration: {:?}", config);

    let sample_format = config.sample_format();
    let config: StreamConfig = config.into();

    // Create a single buffer to hold the sample data
    let sample_buffer = Arc::new(Mutex::new(SampleData::F32(Vec::new())));

    let err_fn = |err: StreamError| eprintln!("An error occurred on the input stream: {}", err);

    let stream = match sample_format {
        SampleFormat::F32 => {
            let sample_buffer_clone = Arc::clone(&sample_buffer);
            input_device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buffer = sample_buffer_clone.lock().unwrap();
                    if let SampleData::F32(ref mut vec) = *buffer {
                        vec.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let sample_buffer_clone = Arc::clone(&sample_buffer);
            *sample_buffer.lock().unwrap() = SampleData::I16(Vec::new());
            input_device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mut buffer = sample_buffer_clone.lock().unwrap();
                    if let SampleData::I16(ref mut vec) = *buffer {
                        vec.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )
        }
        SampleFormat::U16 => {
            let sample_buffer_clone = Arc::clone(&sample_buffer);
            *sample_buffer.lock().unwrap() = SampleData::U16(Vec::new());
            input_device.build_input_stream(
                &config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let mut buffer = sample_buffer_clone.lock().unwrap();
                    if let SampleData::U16(ref mut vec) = *buffer {
                        vec.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )
        }
        _ => panic!("Unsupported sample format"),
    }
    .unwrap();

    // Start the stream
    stream.play().unwrap();

    // Keep the main thread alive for a while to capture some samples
    thread::sleep(Duration::from_secs(5));

    // Stop the stream
    drop(stream);

    // Print out the captured data for inspection (you can remove this part)
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
}
