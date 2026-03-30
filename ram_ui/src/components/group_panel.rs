//! Group control panel — shown when multiple accounts are selected.
//! Provides bulk launch, bulk remove, and selection summary.

use eframe::egui;
use ram_core::models::Account;

/// Actions the group panel can request.
pub enum GroupPanelAction {
    /// Launch all selected accounts into the given place/server.
    BulkLaunch {
        place_id: u64,
        job_id: Option<String>,
    },
    /// Deselect all.
    ClearSelection,
    /// Kill all Roblox instances.
    KillAll,
}

/// Persistent input state for the group panel.
pub struct GroupPanelState {
    pub place_id_input: String,
    pub job_id_input: String,
}

impl Default for GroupPanelState {
    fn default() -> Self {
        Self {
            place_id_input: String::new(),
            job_id_input: String::new(),
        }
    }
}

/// Draw the group control panel for multiple selected accounts.
pub fn show(
    ui: &mut egui::Ui,
    selected_accounts: &[&Account],
    state: &mut GroupPanelState,
    roblox_running: bool,
) -> Option<GroupPanelAction> {
    let mut action: Option<GroupPanelAction> = None;
    let count = selected_accounts.len();

    egui::ScrollArea::vertical().show(ui, |ui| {
    ui.vertical(|ui| {
        // Header
        ui.horizontal(|ui| {
            ui.heading(format!("{count} Accounts Selected"));
            if ui.small_button("Clear selection").clicked() {
                action = Some(GroupPanelAction::ClearSelection);
            }
        });
        ui.separator();
        ui.add_space(4.0);

        // Selected account list (compact)
        egui::ScrollArea::vertical()
            .max_height(150.0)
            .show(ui, |ui| {
                for account in selected_accounts {
                    ui.horizontal(|ui| {
                        let dot = presence_color(account.last_presence.user_presence_type);
                        let (dot_rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 14.0), egui::Sense::hover());
                        ui.painter().circle_filled(
                            dot_rect.center(),
                            4.0,
                            dot,
                        );
                        ui.label(account.label());
                    });
                }
            });

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        // Bulk launch controls
        ui.heading("Bulk Launch");
        ui.add_space(4.0);
        ui.label("All selected accounts will join the same server sequentially.");
        ui.add_space(4.0);

        egui::Grid::new("bulk_launch_grid")
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

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let place_valid = state.place_id_input.parse::<u64>().is_ok();
            let btn = ui.add_enabled(
                place_valid,
                egui::Button::new(format!("\u{1f680}  Launch {count} Accounts")),
            );
            if btn.clicked() {
                if let Ok(place_id) = state.place_id_input.parse::<u64>() {
                    let job_id = if state.job_id_input.trim().is_empty() {
                        None
                    } else {
                        Some(state.job_id_input.trim().to_string())
                    };
                    action = Some(GroupPanelAction::BulkLaunch { place_id, job_id });
                }
            }

            if roblox_running {
                if ui.button("\u{2620}  Kill All Instances").clicked() {
                    action = Some(GroupPanelAction::KillAll);
                }
            }
        });
    });
    }); // ScrollArea

    action
}

fn presence_color(presence_type: u8) -> egui::Color32 {
    match presence_type {
        1 => egui::Color32::from_rgb(60, 180, 75),
        2 => egui::Color32::from_rgb(30, 144, 255),
        3 => egui::Color32::from_rgb(255, 165, 0),
        _ => egui::Color32::from_rgb(130, 130, 130),
    }
}
