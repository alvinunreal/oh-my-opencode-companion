#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod gifs;
mod screen;
mod singleton;
mod state;

use singleton::acquire;

fn main() -> eframe::Result {
    // Exit immediately if another instance is already running
    if !acquire() {
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_transparent(true)
            .with_inner_size([1.0, 1.0])
            // Offscreen so the "coordinator" window is invisible
            .with_position([-500.0, -500.0])
            .with_active(false),
        // Run as a macOS accessory app: no Dock icon, never steals focus
        // from the terminal when the windows appear.
        event_loop_builder: Some(Box::new(|builder| {
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
                builder.with_activation_policy(ActivationPolicy::Accessory);
                builder.with_activate_ignoring_other_apps(false);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = builder;
        })),
        ..Default::default()
    };

    eframe::run_native(
        "oh-my-opencode-slim-companion",
        options,
        Box::new(|cc| Ok(Box::new(app::CompanionApp::new(cc)))),
    )
}
