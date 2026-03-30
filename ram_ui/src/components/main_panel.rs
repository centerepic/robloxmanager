//! Main content panel — selected account details, avatar, launch controls.

use eframe::egui;
use ram_core::models::{Account, FavoritePlace};

/// Actions the main panel can request.
pub enum MainPanelAction {
    LaunchGame { place_id: u64, job_id: Option<String> },
    RemoveAccount(u64),
    UpdateAlias { user_id: u64, alias: String },
    SaveFavorite { name: String, place_id: u64 },
    RemoveFavorite(usize),
    KillAll,
}

/// Persistent input state for the main panel.
#[derive(Default)]
pub struct MainPanelState {
    pub place_id_input: String,
    pub job_id_input: String,
    pub alias_input: String,
    /// Track which account the alias input belongs to.
    alias_for_user: Option<u64>,
    pub favorite_name_input: String,
}

/// Draw the main panel for a selected account. Returns an optional action.
pub fn show(
    ui: &mut egui::Ui,
    account: &Account,
    state: &mut MainPanelState,
    roblox_running: bool,
    avatar_bytes: Option<&Vec<u8>>,
    favorite_places: &[FavoritePlace],
) -> Option<MainPanelAction> {
    let mut action: Option<MainPanelAction> = None;

    egui::ScrollArea::vertical().show(ui, |ui| {
    ui.vertical(|ui| {
        let section_frame = egui::Frame::default()
            .inner_margin(egui::Margin::same(10.0))
            .rounding(egui::Rounding::same(6.0))
            .fill(ui.visuals().extreme_bg_color);

        // ---- Header row: avatar + name ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            // Avatar image (loaded from backend-downloaded bytes)
            if let Some(bytes) = avatar_bytes {
                let uri = format!("bytes://avatar/{}.png", account.user_id);
                ui.add(
                    egui::Image::from_bytes(uri, bytes.clone())
                        .fit_to_exact_size(egui::vec2(64.0, 64.0))
                        .rounding(egui::Rounding::same(8.0)),
                );
            } else {
                // Placeholder
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(64.0, 64.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(
                    rect,
                    8.0,
                    egui::Color32::from_rgb(60, 60, 70),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "?",
                    egui::FontId::proportional(28.0),
                    egui::Color32::WHITE,
                );
            }

            ui.vertical(|ui| {
                ui.heading(&account.display_name);
                ui.label(format!("@{}", account.username));
                ui.label(format!("ID: {}", account.user_id));

                // Presence badge
                let status = account.last_presence.status_text();
                let color = match account.last_presence.user_presence_type {
                    1 => egui::Color32::from_rgb(60, 180, 75),
                    2 => egui::Color32::from_rgb(30, 144, 255),
                    3 => egui::Color32::from_rgb(255, 165, 0),
                    _ => egui::Color32::GRAY,
                };
                ui.colored_label(color, status);
            });
        });
        }); // header frame
        ui.add_space(6.0);

        // ---- Launch controls ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
        ui.heading("Launch Game");
        ui.add_space(4.0);

        // Favorite places quick-select
        if !favorite_places.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Favorites:");
                for (i, fav) in favorite_places.iter().enumerate() {
                    if ui.small_button(&fav.name).clicked() {
                        state.place_id_input = fav.place_id.to_string();
                    }
                    // Right-click to remove
                    if ui.interact(
                        ui.min_rect(),
                        egui::Id::new(("fav_ctx", i)),
                        egui::Sense::click(),
                    ).secondary_clicked() {
                        action = Some(MainPanelAction::RemoveFavorite(i));
                    }
                }
            });
            ui.add_space(4.0);
        }

        egui::Grid::new("launch_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Place ID:");
                ui.text_edit_singleline(&mut state.place_id_input);
                ui.end_row();

                ui.label("Job ID (optional):");
                ui.text_edit_singleline(&mut state.job_id_input);
                ui.end_row();
            });

        ui.add_space(4.0);

        // Save current Place ID as a favorite
        let place_valid = state.place_id_input.parse::<u64>().is_ok();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(place_valid, |ui| {
                ui.text_edit_singleline(&mut state.favorite_name_input)
                    .on_hover_text("Name for this favorite");
                let can_save = !state.favorite_name_input.trim().is_empty();
                if ui.add_enabled(can_save, egui::Button::new("⭐ Save Favorite")).clicked() {
                    if let Ok(pid) = state.place_id_input.parse::<u64>() {
                        action = Some(MainPanelAction::SaveFavorite {
                            name: state.favorite_name_input.trim().to_string(),
                            place_id: pid,
                        });
                        state.favorite_name_input.clear();
                    }
                }
            });
        });

        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let launch_btn = ui.add_enabled(place_valid, egui::Button::new("🚀  Launch"));
            if launch_btn.clicked() {
                if let Ok(place_id) = state.place_id_input.parse::<u64>() {
                    let job_id = if state.job_id_input.trim().is_empty() {
                        None
                    } else {
                        Some(state.job_id_input.trim().to_string())
                    };
                    action = Some(MainPanelAction::LaunchGame { place_id, job_id });
                }
            }

            if roblox_running
                && ui.button("☠  Kill All Instances").clicked()
            {
                action = Some(MainPanelAction::KillAll);
            }
        });
        }); // launch frame
        ui.add_space(6.0);

        // ---- Account metadata ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());

        // Sync alias input when switching accounts
        if state.alias_for_user != Some(account.user_id) {
            state.alias_input = account.alias.clone();
            state.alias_for_user = Some(account.user_id);
        }

        egui::Grid::new("meta_grid")
            .num_columns(2)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                ui.label("Alias:");
                let alias_response = ui.text_edit_singleline(&mut state.alias_input);
                if alias_response.lost_focus() && state.alias_input != account.alias {
                    action = Some(MainPanelAction::UpdateAlias {
                        user_id: account.user_id,
                        alias: state.alias_input.clone(),
                    });
                }
                ui.end_row();

                if !account.group.is_empty() {
                    ui.label("Group:");
                    ui.label(&account.group);
                    ui.end_row();
                }

                if let Some(ts) = &account.last_validated {
                    ui.label("Validated:");
                    let age = chrono::Utc::now() - *ts;
                    let color = if age.num_hours() > 24 {
                        egui::Color32::from_rgb(200, 160, 60)
                    } else {
                        ui.visuals().text_color()
                    };
                    ui.colored_label(color, ts.format("%Y-%m-%d %H:%M UTC").to_string());
                    ui.end_row();
                }

                if !account.last_presence.last_location.is_empty() {
                    ui.label("Location:");
                    ui.label(&account.last_presence.last_location);
                    ui.end_row();
                }
            });

        // Expired cookie warning
        if account.cookie_expired {
            ui.add_space(4.0);
            egui::Frame::default()
                .fill(egui::Color32::from_rgb(80, 30, 30))
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 100, 100),
                        "\u{26a0} Cookie expired — remove and re-add this account with a fresh cookie",
                    );
                });
        }
        }); // metadata frame
        ui.add_space(6.0);

        // ---- Danger zone ----
        section_frame.show(ui, |ui: &mut egui::Ui| {
            ui.set_min_width(ui.available_width());
            ui.strong("Danger Zone");
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::from_rgb(200, 60, 60), "These actions cannot be undone.");
            if ui.button("🗑  Remove Account").clicked() {
                action = Some(MainPanelAction::RemoveAccount(account.user_id));
            }
        });
    });
    }); // ScrollArea

    action
}

/// Show a placeholder when no account is selected.
pub fn show_empty(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.label("Select an account from the sidebar to get started.");
    });
}
