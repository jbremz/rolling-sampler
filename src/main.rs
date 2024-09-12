use std::error::Error;

use buffer_sample::recorder::Recorder;
use eframe::{run_native, App, CreationContext};

fn main() -> Result<(), Box<dyn Error>> {
    let app_name = "Rolling Sampler";
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 430.0]), /* Set your desired width and height */
        ..Default::default()
    };
    let app_creator =
        move |_cc: &CreationContext| -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>> {
            Ok(Box::new(Recorder::new(5).unwrap())) // Initialize with a buffer size of 44100
                                                    // samples
        };
    run_native(app_name, native_options, Box::new(app_creator))?;
    Ok(())
}
