#![windows_subsystem = "windows"]

mod app;
mod bridge;
mod components;
mod toast;

use tracing_subscriber::EnvFilter;

fn main() {
    // Initialise tracing (logs to stderr, controllable via RUST_LOG).
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config_path = std::path::PathBuf::from("config.json");
    let config = ram_core::AppConfig::load(&config_path);

    // Decode the embedded logo for the window icon.
    let icon = {
        let png = include_bytes!("../../assets/Logo.png");
        let img = image::load_from_memory(png).expect("failed to decode Logo.png");
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        eframe::egui::IconData {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([config.window_width, config.window_height])
            .with_min_inner_size([640.0, 400.0])
            .with_title("RM | Roblox Manager")
            .with_icon(icon),
        ..Default::default()
    };

    let _ = eframe::run_native(
        "RM",
        native_options,
        Box::new(move |cc| {
            // Enable image loading for egui_extras (avatars, etc.)
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(app::AppState::new(config, config_path)))
        }),
    );
}
