#![windows_subsystem = "windows"]

mod app;
mod bridge;
mod components;
mod toast;

use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

/// Canonical data directory: `%APPDATA%\RM`.
fn data_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("RM")
}

/// Check for legacy data files next to the exe and offer to migrate them.
fn maybe_migrate_legacy_data(data_dir: &std::path::Path) {
    let legacy_config = PathBuf::from("config.json");
    let legacy_accounts = PathBuf::from("accounts.dat");

    let has_legacy = legacy_config.is_file() || legacy_accounts.is_file();
    let has_new = data_dir.join("config.json").is_file();

    if !has_legacy || has_new {
        return;
    }

    // Show a native dialog before the egui window opens
    let result = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Info)
        .set_title("RM — Migrate Data")
        .set_description(
            "RM now stores data in a standard location so it works \
             no matter where the exe is placed.\n\n\
             Found existing data next to the exe. Move it to the new location?\n\n\
             • Yes — move files (recommended)\n\
             • No — keep using files next to the exe",
        )
        .set_buttons(rfd::MessageButtons::YesNo)
        .show();

    if result == rfd::MessageDialogResult::Yes {
        if let Err(e) = std::fs::create_dir_all(data_dir) {
            tracing::error!("Failed to create data dir: {e}");
            return;
        }
        for name in &["config.json", "accounts.dat"] {
            let src = PathBuf::from(name);
            if src.is_file() {
                let dst = data_dir.join(name);
                if let Err(e) = std::fs::rename(&src, &dst) {
                    // rename can fail across volumes; fall back to copy+delete
                    if let Err(e2) = std::fs::copy(&src, &dst) {
                        tracing::error!("Failed to migrate {name}: rename={e}, copy={e2}");
                    } else {
                        let _ = std::fs::remove_file(&src);
                    }
                }
            }
        }
    }
}

fn main() {
    // Initialise tracing (logs to stderr, controllable via RUST_LOG).
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let data_dir = data_dir();

    // Offer to migrate legacy data from the exe directory
    maybe_migrate_legacy_data(&data_dir);

    // Ensure the data directory exists
    let _ = std::fs::create_dir_all(&data_dir);

    // Resolve config and account paths.
    // If a legacy config.json still exists next to the exe (user declined migration),
    // keep using local paths for backwards compatibility.
    let (config_path, config) = if PathBuf::from("config.json").is_file()
        && !data_dir.join("config.json").is_file()
    {
        // User declined migration — use local files
        let p = PathBuf::from("config.json");
        let c = ram_core::AppConfig::load(&p);
        (p, c)
    } else {
        let p = data_dir.join("config.json");
        let mut c = ram_core::AppConfig::load(&p);
        // Ensure accounts_path is absolute under the data dir
        if c.accounts_path == std::path::Path::new("accounts.dat") {
            c.accounts_path = data_dir.join("accounts.dat");
        }
        (p, c)
    };

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
            .with_title(format!("RM | Roblox Manager v{}", env!("CARGO_PKG_VERSION")))
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
