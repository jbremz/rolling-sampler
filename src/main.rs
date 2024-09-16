use chrono::Utc;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{BufferSize, Device, SampleFormat, StreamConfig};
use dirs::home_dir;
use eframe::{run_native, App, CreationContext};
use egui::{CentralPanel, RichText, Vec2b};
use egui_plot::{CoordinatesFormatter, Corner, Line, Plot, PlotPoints, PlotUi};
use hound::{SampleFormat as HoundSampleFormat, WavSpec, WavWriter};
use rfd::FileDialog;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::collections::VecDeque;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

struct Recorder {
    is_grabbing: Arc<AtomicBool>,
    sample_buffer: Arc<Mutex<CircularBuffer>>,
    input_stream: Option<cpal::Stream>,
    config: StreamConfig,
    buffer_size: Arc<Mutex<usize>>,
    save_path: Option<String>,
    input_devices: Vec<Device>,          // Store available input devices
    current_input_device_index: usize,   // Store the index of the selected device
    is_monitoring: Arc<AtomicBool>,      // New flag to track if monitoring is active
    output_stream: Option<cpal::Stream>, // Optional output stream for monitoring
    output_devices: Vec<Device>,         // Output devices (new field for audio output)
    current_output_device_index: usize,  // Store the index of the selected output device
    monitoring_buffers: Arc<Mutex<Vec<VecDeque<f32>>>>, // One VecDeque per channel
    resampler: Option<SincFixedIn<f32>>,
    resample_buffers: Vec<Vec<f32>>, // One buffer per channel for resampled data
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
        let input_devices: Vec<Device> = host.input_devices().unwrap().collect(); // Fetch available devices

        let current_input_device_index = 0; // Set default to the first device
        let input_device = input_devices[current_input_device_index].clone();
        let config = input_device
            .default_input_config()
            .expect("Failed to get default input config");
        let config: StreamConfig = config.into();
        let initial_buffer_size =
            initial_buffer_size * config.sample_rate.0 as usize * config.channels as usize;

        // Get available output devices for live monitoring
        let output_devices: Vec<Device> = host.output_devices().unwrap().collect();
        let current_output_device_index = 0; // Set default to the first device
        let initial_monitoring_capacity = config.sample_rate.0 as usize * 4; // Example: 4 second buffer

        let num_channels = config.channels as usize;
        let monitoring_buffers = Arc::new(Mutex::new(vec![
            VecDeque::with_capacity(
                initial_monitoring_capacity
            );
            num_channels
        ]));
        let resample_buffers = vec![Vec::new(); num_channels];

        // Resolve the Desktop path and convert it to a String
        let save_path: Option<String> = home_dir().and_then(|mut path| {
            path.push("Desktop");
            path.to_str().map(|s| s.to_owned())
        });

        let mut recorder = Recorder {
            is_grabbing: Arc::new(AtomicBool::new(false)),
            sample_buffer: Arc::new(Mutex::new(CircularBuffer::new(initial_buffer_size))),
            input_stream: None,
            config,
            buffer_size: Arc::new(Mutex::new(initial_buffer_size)),
            save_path,
            input_devices,
            current_input_device_index,
            is_monitoring: Arc::new(AtomicBool::new(false)),
            output_stream: None,
            current_output_device_index, // Initially, no output device selected
            output_devices,              // Initialize with available output devices
            monitoring_buffers,
            resampler: None,
            resample_buffers,
        };

        recorder.start_recording();
        recorder
    }

    fn start_recording(&mut self) {
        // Get the currently selected device
        let input_device = self.input_devices[self.current_input_device_index].clone();

        // Fetch the latest configuration
        let config = input_device
            .default_input_config()
            .expect("Failed to get default input config");
        let config: StreamConfig = config.into();
        self.config = config;
        let sample_format = input_device.default_input_config().unwrap().sample_format();

        println!(
            "Input Stream Config - Sample Rate: {}, Channels: {}",
            self.config.sample_rate.0, self.config.channels
        );

        self.reset_buffer(); // Reset the buffer before starting a new recording
        let sample_buffer = Arc::clone(&self.sample_buffer);

        // Reinitialize monitoring buffers
        self.reset_monitoring_buffers();

        let is_monitoring = Arc::clone(&self.is_monitoring);
        let num_channels = self.config.channels as usize;
        let monitoring_buffers = Arc::clone(&self.monitoring_buffers);

        let stream = match sample_format {
            SampleFormat::F32 => {
                input_device.build_input_stream(
                    &self.config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        // Write to sample_buffer
                        {
                            let mut buffer = sample_buffer.lock().unwrap();
                            buffer.add_samples(data);
                        }

                        // If monitoring is enabled, distribute samples to per-channel buffers
                        if is_monitoring.load(Ordering::SeqCst) {
                            let mut m_buffers = monitoring_buffers.lock().unwrap();
                            for (i, &sample) in data.iter().enumerate() {
                                let channel = i % num_channels;
                                let m_buffer = &mut m_buffers[channel];
                                if m_buffer.len() == m_buffer.capacity() {
                                    m_buffer.pop_front();
                                }
                                m_buffer.push_back(sample);
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

        // Stop the previous stream if it exists
        if let Some(old_stream) = self.input_stream.take() {
            drop(old_stream);
        }

        self.input_stream = Some(stream);
        self.is_grabbing.store(false, Ordering::SeqCst);
    }

    fn grab_recording(&mut self) {
        self.is_grabbing.store(false, Ordering::SeqCst);
        if let Some(stream) = self.input_stream.take() {
            drop(stream); // This drops the stream and stops recording
        }

        let buffer = self.sample_buffer.lock().unwrap();

        // Determine the number of samples in the buffer
        let num_samples = buffer.current_size;
        let num_channels = self.config.channels as usize;

        println!(
            "Recorded shape: ({}, {})",
            num_samples / num_channels,
            num_channels
        );
        println!("Sample rate: {}", self.config.sample_rate.0);

        // Prepare WAV writer specifications based on the buffer content
        let spec = WavSpec {
            channels: self.config.channels,
            sample_rate: self.config.sample_rate.0,
            bits_per_sample: 32, // Assuming f32 for this example
            sample_format: HoundSampleFormat::Float,
        };

        let filepath = format!(
            "{}/{}.wav",
            self.save_path.as_ref().unwrap(),
            get_file_safe_timestamp()
        );
        let mut writer = WavWriter::create(filepath, spec).expect("Failed to create WAV writer");

        // Save buffer
        for &sample in buffer.static_buffer.iter().take(buffer.current_size) {
            writer.write_sample(sample).unwrap();
        }

        writer.finalize().expect("Failed to finalize WAV writer");

        println!("Recording saved!");

        // Replace the buffer with a new one rather than clearing the old one
        drop(buffer); // Unlock the mutex before replacing the buffer

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
            println!(
                "Save directory selected: {}",
                self.save_path.as_ref().unwrap()
            );
        }
    }

    fn reset_buffer(&mut self) {
        // Lock the current buffer size to reuse it
        let new_buffer_size = *self.buffer_size.lock().unwrap();

        // Replace the old buffer with a new one
        self.sample_buffer = Arc::new(Mutex::new(CircularBuffer::new(new_buffer_size)));
    }

    fn start_monitoring(&mut self) {
        let output_device = self.output_devices[self.current_output_device_index].clone();
        let config = output_device.default_output_config().unwrap();
        let sample_format = config.sample_format();
        let config: StreamConfig = config.into();
        let output_config = StreamConfig {
            channels: config.channels,
            sample_rate: config.sample_rate,
            buffer_size: BufferSize::Fixed(2048), // Adjust this value as needed
        };

        println!(
            "Output Stream Config - Sample Rate: {}, Channels: {}",
            config.sample_rate.0, config.channels
        );

        let num_output_channels = config.channels as usize;
        let num_input_channels = self.config.channels as usize;

        // Initialize resampler only if sample rates differ
        let input_sample_rate = self.config.sample_rate.0 as f64;
        let output_sample_rate = config.sample_rate.0 as f64;

        if (input_sample_rate - output_sample_rate).abs() > f64::EPSILON {
            let resampler = SincFixedIn::<f32>::new(
                output_sample_rate / input_sample_rate, // Resampling ratio
                2.0,                                    // Oversampling factor
                SincInterpolationParameters {
                    sinc_len: 256, // Increased from default
                    f_cutoff: 0.95,
                    interpolation: SincInterpolationType::Linear,
                    oversampling_factor: 256, // Increased from default
                    window: WindowFunction::BlackmanHarris2,
                },
                4096,               // Chunk size
                num_input_channels, // Number of channels
            )
            .expect("Failed to create resampler");
            self.resampler = Some(resampler);
        } else {
            self.resampler = None;
        }

        self.resample_buffers = vec![Vec::new(); num_input_channels]; // Reset resample buffers

        let monitoring_buffers = Arc::clone(&self.monitoring_buffers);
        let resampler = Arc::new(Mutex::new(self.resampler.take()));

        let resampler_clone = Arc::clone(&resampler);
        let output_stream = match sample_format {
            SampleFormat::F32 => {
                output_device.build_output_stream(
                    &output_config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut m_buffers = monitoring_buffers.lock().unwrap();
                        let mut samples_per_channel: Vec<Vec<f32>> =
                            vec![Vec::new(); num_input_channels];

                        // Collect samples per channel
                        for (channel, m_buffer) in m_buffers.iter_mut().enumerate() {
                            while let Some(sample) = m_buffer.pop_front() {
                                samples_per_channel[channel].push(sample);
                            }
                        }

                        // Determine the minimum number of samples across all channels
                        let min_samples = samples_per_channel
                            .iter()
                            .map(|v| v.len())
                            .min()
                            .unwrap_or(0);

                        // Resample if necessary and if we have enough samples
                        let resampled_samples_per_channel: Vec<Vec<f32>> =
                            if let Some(resampler) = resampler_clone.lock().unwrap().as_mut() {
                                let chunk_size = resampler.input_frames_next();
                                if min_samples >= chunk_size {
                                    // We have enough samples to resample
                                    let input: Vec<&[f32]> = samples_per_channel
                                        .iter()
                                        .map(|v| &v[..chunk_size])
                                        .collect();
                                    match resampler.process(&input, None) {
                                        Ok(output) => output,
                                        Err(e) => {
                                            eprintln!("Resampling failed: {}", e);
                                            vec![vec![0.0; chunk_size]; num_input_channels]
                                            // Return silence on error
                                        }
                                    }
                                } else {
                                    // Not enough samples, return the original samples
                                    samples_per_channel.clone()
                                }
                            } else {
                                samples_per_channel.clone()
                            };

                        // Trim the original samples_per_channel to remove processed samples
                        if min_samples > 0 {
                            for channel_samples in samples_per_channel.iter_mut() {
                                channel_samples.drain(..min_samples);
                            }
                        }

                        // Determine the number of frames to write
                        let num_frames = data.len() / num_output_channels;

                        for frame_idx in 0..num_frames {
                            for channel in 0..num_output_channels {
                                if channel < resampled_samples_per_channel.len() {
                                    let channel_samples = &resampled_samples_per_channel[channel];
                                    if frame_idx < channel_samples.len() {
                                        data[frame_idx * num_output_channels + channel] =
                                            channel_samples[frame_idx];
                                    } else {
                                        data[frame_idx * num_output_channels + channel] = 0.0;
                                        // Silence
                                    }
                                } else {
                                    data[frame_idx * num_output_channels + channel] = 0.0;
                                    // Silence
                                }
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            _ => panic!("Unsupported sample format for monitoring"),
        }
        .unwrap();

        // Stop the previous output stream if it exists
        if let Some(old_stream) = self.output_stream.take() {
            drop(old_stream);
        }
        self.output_stream = Some(output_stream);
        self.is_monitoring.store(true, Ordering::SeqCst);
        println!("Monitoring started");
    }

    fn stop_monitoring(&mut self) {
        if let Some(output_stream) = self.output_stream.take() {
            drop(output_stream); // Stop the output stream
        }
        self.is_monitoring.store(false, Ordering::SeqCst);

        // Clear the monitoring buffers
        let mut m_buffers = self.monitoring_buffers.lock().unwrap();
        for buffer in m_buffers.iter_mut() {
            buffer.clear();
        }

        println!("Monitoring stopped");
    }
    fn reset_monitoring_buffers(&mut self) {
        let num_channels = self.config.channels as usize;
        let initial_monitoring_capacity = self.config.sample_rate.0 as usize * 4; // Example: 4 second buffer
        self.monitoring_buffers = Arc::new(Mutex::new(vec![
            VecDeque::with_capacity(
                initial_monitoring_capacity
            );
            num_channels
        ]));
        self.resample_buffers = vec![Vec::new(); num_channels];
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
            self.static_buffer
                .extend_from_slice(&self.circular_buffer[..self.current_size]);
        } else {
            self.static_buffer
                .extend_from_slice(&self.circular_buffer[self.write_pos..]);
            self.static_buffer
                .extend_from_slice(&self.circular_buffer[..self.write_pos]);
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
            ui.add_space(10.0); // Add some space at the top

            ui.vertical_centered(|ui| {
                // ui.add_space(10.0); // Add some space at the top
                let panel_width = ui.available_width();

                // Center the contents inside the horizontal layout
                ui.vertical_centered(|ui| {
                    // Device selection dropdown - can't centre this because it isn't an atomic widget 🤷
                    ui.horizontal(|ui| {
                        ui.label("Input Device:");
                        let current_input_device_index = self.current_input_device_index; // Store the current device index for later comparison
                        egui::ComboBox::from_id_source("Device") // Using an ID instead of a label
                            .selected_text(
                                self.input_devices[self.current_input_device_index]
                                    .name()
                                    .unwrap_or_default()
                                    .clone(),
                            )
                            .show_ui(ui, |ui| {
                                for (idx, device) in self.input_devices.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.current_input_device_index,
                                        idx,
                                        device.name().unwrap_or_default(),
                                    );
                                }
                            });
                        // Check if the selected device has changed
                        if current_input_device_index != self.current_input_device_index {
                            // Stop current recording
                            if let Some(stream) = self.input_stream.take() {
                                drop(stream);
                            }
                            // Update config for new device
                            let new_device = &self.input_devices[self.current_input_device_index];
                            self.config = new_device
                                .default_input_config()
                                .expect("Failed to get default input config")
                                .into();
                            // Start recording with new device
                            self.start_recording();
                        }
                    });

                    // Output Device Selection
                    ui.horizontal(|ui| {
                        ui.label("Output Device:");
                        let output_device =
                            self.output_devices[self.current_output_device_index].clone();
                        egui::ComboBox::from_id_source("OutputDevice")
                            .selected_text(output_device.name().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for device in &self.output_devices {
                                    // Get the name of the current device
                                    if let Ok(device_name) = device.name() {
                                        // Check if the device's name matches the currently selected one
                                        let is_selected = self.output_devices
                                            [self.current_output_device_index]
                                            .name()
                                            .unwrap_or_default()
                                            == device_name;

                                        if ui
                                            .selectable_label(is_selected, device_name.clone())
                                            .clicked()
                                        {
                                            self.current_output_device_index = self
                                                .output_devices
                                                .iter()
                                                .position(|d| {
                                                    d.name().unwrap_or_default() == device_name
                                                })
                                                .unwrap_or(0); // Update the selected device
                                        }
                                    }
                                }
                            });
                    });

                    // Add a checkbox to enable/disable monitoring
                    ui.horizontal(|ui| {
                        let mut monitoring = self.is_monitoring.load(Ordering::SeqCst);
                        if ui.checkbox(&mut monitoring, "Enable Monitoring").changed() {
                            if monitoring {
                                self.start_monitoring();
                            } else {
                                self.stop_monitoring();
                            }
                        }
                    });

                    // Fetch the audio buffer samples for plotting
                    if let Ok(buffer) = self.sample_buffer.lock() {
                        let plot_data = buffer.get_samples_for_plot();

                        // Set desired downsampling factor (e.g., take every 10th sample)
                        let downsample_factor = 10;

                        // Create plot points as Vec<[f64; 2]> with downsampling
                        let points: Vec<[f64; 2]> = plot_data
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| i % downsample_factor == 0) // Pick every Nth sample
                            .map(|(i, &sample)| [i as f64, sample as f64]) // Create [x, y] pairs
                            .collect();

                        let plot_points = PlotPoints::new(points);

                        // Create a line from the points
                        let line = Line::new(plot_points);

                        // this didn't do what I wanted it to do but I think it's something along these lines
                        let no_coordinates_formatter =
                            CoordinatesFormatter::new(|_, _| String::new());

                        // Display the plot
                        Plot::new("Rolling Waveform Plot")
                            .view_aspect(4.0) // Adjust aspect ratio if necessary
                            .auto_bounds(Vec2b::new(true, false)) // Disable auto bounds for y-axis, keep x-axis auto-bounds
                            .show_axes(false)
                            .show_grid(false)
                            .show_background(false)
                            .allow_zoom(false)
                            .allow_drag(false)
                            .allow_scroll(false)
                            .sharp_grid_lines(true)
                            .coordinates_formatter(Corner::LeftBottom, no_coordinates_formatter) // Disable coordinates display
                            .show(ui, |plot_ui: &mut PlotUi| {
                                plot_ui.line(line);
                            });
                    }

                    ui.label(
                        RichText::new("Choose how much past audio to include in the recording:")
                            .italics(),
                    );

                    // Slider to control buffer size
                    let mut buffer_size = *self.buffer_size.lock().unwrap();

                    let desired_width = panel_width * 0.8;
                    ui.style_mut().spacing.slider_width = desired_width;

                    // Convert buffer size from samples to seconds for the slider display
                    let buffer_size_seconds = buffer_size as f32 / self.config.sample_rate.0 as f32;
                    let max_buffer_seconds = 60.0; // Maximum 60 seconds for the slider
                    let mut new_buffer_size_seconds = buffer_size_seconds;

                    ui.horizontal(|ui| {
                        ui.label("Buffer Size (s):"); // Text label before the slider
                        let response = ui.add(egui::Slider::new(
                            &mut new_buffer_size_seconds,
                            1.0..=max_buffer_seconds,
                        ));

                        let new_buffer_size =
                            (new_buffer_size_seconds * self.config.sample_rate.0 as f32) as usize;

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

                    ui.add_space(20.0); // Add some space between the path selector and the button
                                        // Start/Stop Recording button
                    let record_button_text = if self.is_grabbing.load(Ordering::SeqCst) {
                        "Stop Grab"
                    } else {
                        "Start Grab"
                    };

                    if ui
                        .add_sized([100.0, 40.0], egui::Button::new(record_button_text))
                        .clicked()
                    {
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
    let app_name = "Rolling Sampler";
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 445.0]), // Set your desired width and height
        ..Default::default()
    };
    let app_creator =
        move |_cc: &CreationContext| -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>> {
            Ok(Box::new(Recorder::new(5))) // Initialize with a buffer size of 44100 samples
        };
    run_native(app_name, native_options, Box::new(app_creator))?;
    Ok(())
}
