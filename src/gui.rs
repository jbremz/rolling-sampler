use std::sync::{Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use egui::{CentralPanel, ViewportId};
use egui_winit::winit::event_loop::{ControlFlow, EventLoop};
use egui_winit::winit::window::WindowBuilder;
use egui_winit::winit::event::{Event, WindowEvent};

pub fn run_gui(is_recording: Arc<AtomicBool>, stop_flag: Arc<AtomicBool>) {
    // Correctly create the event loop, unwrapping the Result
    let event_loop = EventLoop::new().expect("Failed to create event loop");

    // Create the window using the event loop
    let window = WindowBuilder::new()
        .with_title("Audio Recorder")
        .build(&event_loop)  // Pass the event loop directly here
        .unwrap();           // Handle potential errors

    let mut egui_ctx = egui::Context::default();
    
    // Create a new ViewportId from a hashable source
    let viewport_id = ViewportId::from_hash_of(&"unique_viewport_id");

    // Initialize egui_winit_state with the required parameters.
    let mut egui_winit_state = egui_winit::State::new(
        egui_ctx,  // Pass by value
        viewport_id,
        &event_loop,
        None, // Option<f32>: Pass None if you don't need to specify a dpi scale override
        None  // Option<usize>: Pass None if you don't need to limit the maximum frame rate
    );

    event_loop.run(move |event, control_flow| {
        match &event {
            Event::WindowEvent { event, .. } => {
                let redraw = egui_winit_state.on_window_event(&window, event);
                if redraw {
                    window.request_redraw();
                }
    
                if let WindowEvent::CloseRequested = event {
                    *control_flow = ControlFlow::ExitWithCode(0);
                }
            }
            Event::RedrawRequested(_) => {
                let raw_input = egui_winit_state.take_egui_input(&window);
    
                egui_ctx.begin_frame(raw_input);
    
                CentralPanel::default().show(&egui_ctx, |ui| {
                    if ui.button("Start Recording").clicked() {
                        is_recording.store(true, Ordering::SeqCst);
                        stop_flag.store(false, Ordering::SeqCst);
                    }
                    if ui.button("Stop Recording").clicked() {
                        is_recording.store(false, Ordering::SeqCst);
                        stop_flag.store(true, Ordering::SeqCst);
                    }
                });
    
                let output = egui_ctx.end_frame();
                let paint_jobs = egui_ctx.tessellate(output.shapes, output.pixels_per_point);

                // Here, you should handle rendering with your chosen backend (like wgpu).
                // The paint method is not directly available in `State`, so you'd need to
                // manage drawing with your graphics API.
                //
                // Example pseudocode for rendering:
                // render(paint_jobs, output.textures_delta);
            }
            _ => (),
        }
    });
}
