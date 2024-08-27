use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use eframe::{run_native, App, NativeOptions, CreationContext};
use egui::CentralPanel;
use std::error::Error;

struct Recorder;

impl App for Recorder {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        CentralPanel::default().show(ctx, |ui| {
            ui.heading("Recorder");
            ui.horizontal(|ui| {
                if ui.button("Record").clicked() {
                    println!("Record button clicked");
                }
                if ui.button("Stop").clicked() {
                    println!("Stop button clicked");
                }
            });
        });
    } 
}

pub fn main() {
    let app_name = "Recorder";
    let native_options = NativeOptions::default();
    let app_creator = move |cc: &CreationContext| -> Result<Box<dyn App>, Box<dyn Error + Send + Sync>> {
        Ok(Box::new(Recorder))
    };
    run_native(app_name, native_options, Box::new(app_creator)).unwrap();
}
