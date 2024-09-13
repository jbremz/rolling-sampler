use std::sync::mpsc::{Receiver, Sender};

use chrono::Utc;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{DefaultStreamConfigError, Device, DevicesError, SampleFormat, StreamConfig};
use dirs::home_dir;
use eframe::App;
use egui::{CentralPanel, RichText, Vec2b};
use egui_plot::{CoordinatesFormatter, Corner, Line, Plot, PlotPoints, PlotUi};
use hound::{SampleFormat as HoundSampleFormat, WavSpec, WavWriter};
use rfd::FileDialog;

use crate::circular_buffer::CircularBuffer;

pub struct Recorder {
    is_grabbing: bool,
    sample_buffer: CircularBuffer,
    stream: Option<cpal::Stream>,
    config: StreamConfig,
    buffer_size: usize,
    save_path: Option<String>,
    devices: Vec<Device>,        // Store available input devices
    current_device_index: usize, // Store the index of the selected device
    audio_receiver: Receiver<Vec<f32>>,
    audio_transmitter: Sender<Vec<f32>>,
}
#[derive(Debug)]
pub enum RecorderError {
    DevicesError(DevicesError),
    ConfigError(cpal::DefaultStreamConfigError),
    NoInputDevices,
    NoHomeDirectory,
}

impl From<DevicesError> for RecorderError {
    fn from(value: DevicesError) -> Self {
        Self::DevicesError(value)
    }
}

impl From<DefaultStreamConfigError> for RecorderError {
    fn from(value: DefaultStreamConfigError) -> Self {
        Self::ConfigError(value)
    }
}

impl Recorder {
    pub fn new(initial_buffer_size_s: usize) -> Result<Self, RecorderError> {
        let devices: Vec<Device> = cpal::default_host().input_devices()?.collect(); // Fetch available devices

        let input_device = devices.first().ok_or(RecorderError::NoInputDevices)?;

        let config: StreamConfig = input_device.default_input_config()?.into();

        let initial_buffer_size_samples = initial_buffer_size_s * config.sample_rate.0 as usize;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut recorder = Recorder {
            is_grabbing: false,
            sample_buffer: CircularBuffer::new(initial_buffer_size_samples),
            stream: None,
            config,
            buffer_size: initial_buffer_size_samples,
            save_path: get_save_path(),
            devices,
            current_device_index: 0,
            audio_receiver: rx,
            audio_transmitter: tx,
        };

        recorder.start_recording();
        Ok(recorder)
    }

    fn start_recording(&mut self) {
        // Get the currently selected device
        let input_device = self.devices[self.current_device_index].clone();

        // Fetch the latest configuration
        let config =
            input_device.default_input_config().expect("Failed to get default input config");
        let config: StreamConfig = config.into();
        self.config = config;
        let sample_format = input_device.default_input_config().unwrap().sample_format();
        let audio_transmitter = self.audio_transmitter.clone();
        self.reset_buffer(); // Reset the buffer before starting a new recording

        let stream = match sample_format {
            SampleFormat::F32 => input_device.build_input_stream(
                &self.config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let _ = audio_transmitter.send(Vec::from(data));
                },
                err_fn,
                None,
            ),
            _ => panic!("Unsupported sample format"),
        }
        .unwrap();

        // Stop the previous stream if it exists
        if let Some(old_stream) = self.stream.take() {
            drop(old_stream);
        }

        self.stream = Some(stream);
        self.is_grabbing = false;
    }

    fn grab_recording(&mut self) {
        self.is_grabbing = false;
        if let Some(stream) = self.stream.take() {
            drop(stream); // This drops the stream and stops recording
        }

        // Determine the number of samples in the buffer
        let num_samples = self.sample_buffer.current_size;
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

        let filepath =
            format!("{}/{}.wav", self.save_path.as_ref().unwrap(), get_file_safe_timestamp());
        let mut writer = WavWriter::create(filepath, spec).expect("Failed to create WAV writer");

        // Save buffer
        for sample in self.sample_buffer.static_buffer.drain(..) {
            writer.write_sample(sample).unwrap();
        }

        writer.finalize().expect("Failed to finalize WAV writer");

        println!("Recording saved!");

        // Restart recording with a fresh stream
        self.start_recording();
    }

    fn update_buffer_size(&mut self, new_size: usize) {
        self.buffer_size = new_size;

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
        // Replace the old buffer with a new one
        self.sample_buffer = CircularBuffer::new(self.buffer_size);
    }
}

impl App for Recorder {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // Repaint the UI to update the plot
        ctx.request_repaint();

        while let Ok(thing) = self.audio_receiver.try_recv() {
            self.sample_buffer.add_samples(&thing);
        }

        CentralPanel::default().show(ctx, |ui| {
            ui.add_space(10.0); // Add some space at the top

            ui.vertical_centered(|ui| {
                // ui.add_space(10.0); // Add some space at the top
                let panel_width = ui.available_width();

                // Center the contents inside the horizontal layout
                ui.vertical_centered(|ui| {
                    // Device selection dropdown - can't centre this because it isn't an atomic
                    // widget 🤷
                    ui.horizontal(|ui| {
                        ui.label("Input Device:");
                        let current_device_index = self.current_device_index; // Store the current device index for later comparison
                        egui::ComboBox::from_id_source("Device") // Using an ID instead of a label
                            .selected_text(
                                self.devices[self.current_device_index]
                                    .name()
                                    .unwrap_or_default()
                                    .clone(),
                            )
                            .show_ui(ui, |ui| {
                                for (idx, device) in self.devices.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.current_device_index,
                                        idx,
                                        device.name().unwrap_or_default(),
                                    );
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
                            self.config = new_device
                                .default_input_config()
                                .expect("Failed to get default input config")
                                .into();
                            // Start recording with new device
                            self.start_recording();
                        }
                    });

                    // Fetch the audio buffer samples for plotting

                    let plot_data = self.sample_buffer.get_samples_for_plot();

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

                    // this didn't do what I wanted it to do but I think it's something along
                    // these lines
                    let no_coordinates_formatter = CoordinatesFormatter::new(|_, _| String::new());

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

                    ui.label(
                        RichText::new("Choose how much past audio to include in the recording:")
                            .italics(),
                    );

                    // Slider to control buffer size
                    // let mut buffer_size = *self.buffer_size.lock().unwrap();

                    let desired_width = panel_width * 0.8;
                    ui.style_mut().spacing.slider_width = desired_width;

                    // Convert buffer size from samples to seconds for the slider display
                    let buffer_size_seconds =
                        self.buffer_size as f32 / self.config.sample_rate.0 as f32;
                    let max_buffer_seconds = 60.0; // Maximum 60 seconds for the slider
                    let mut new_buffer_size_seconds = buffer_size_seconds;

                    ui.horizontal(|ui| {
                        ui.label("Buffer Size (s):"); // Text label before the slider
                        let response = ui.add(egui::Slider::new(
                            &mut new_buffer_size_seconds,
                            1.0..=max_buffer_seconds,
                        ));
                        // .text("Buffer Size (s)"));

                        let new_buffer_size =
                            (new_buffer_size_seconds * self.config.sample_rate.0 as f32) as usize;

                        if response.drag_stopped() && new_buffer_size != self.buffer_size {
                            self.update_buffer_size(new_buffer_size);
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
                    let record_button_text =
                        if self.is_grabbing { "Stop Grab" } else { "Start Grab" };

                    if ui.add_sized([100.0, 40.0], egui::Button::new(record_button_text)).clicked()
                    {
                        if self.is_grabbing {
                            println!("Stop button clicked");
                            self.grab_recording();
                        } else {
                            println!("Start grab button clicked");

                            self.sample_buffer.start_static_mode(); // Transition the buffer to static mode
                            self.is_grabbing = true;
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

fn get_file_safe_timestamp() -> String {
    // Get the current time in UTC
    let now = Utc::now();

    // Format the time as "2024-09-10_15-06-32" (safe for file paths)
    now.format("%Y-%m-%d_%H-%M-%S").to_string()
}

fn get_save_path() -> Option<String> {
    let mut home_dir = home_dir()?;
    home_dir.push("Desktop");
    let path_str = home_dir.to_str()?;

    Some(path_str.to_owned())
}
