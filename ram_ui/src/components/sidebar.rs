//! Left sidebar — account list, search/filter, status indicators.
//! Supports multi-select via Ctrl+click (toggle) and Shift+click (range).

use eframe::egui;
use ram_core::models::Account;
use std::collections::HashSet;

/// Sort order for the account list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Name,
    Status,
}

impl std::fmt::Display for SortOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SortOrder::Name => write!(f, "Name"),
            SortOrder::Status => write!(f, "Status"),
        }
    }
}

/// Persistent state for the sidebar widget.
pub struct SidebarState {
    pub search_query: String,
    /// Index of the last clicked account in the filtered list (for Shift-range).
    last_clicked_index: Option<usize>,
    pub sort_order: SortOrder,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            last_clicked_index: None,
            sort_order: SortOrder::Name,
        }
    }
}

/// Actions the sidebar can request from the parent.
pub enum SidebarAction {
    /// Replace the entire selection with this single account.
    Select(u64),
    /// Toggle this account in/out of the selection (Ctrl+click).
    ToggleSelect(u64),
    /// Range-select from the last click to this account (Shift+click).
    RangeSelect(Vec<u64>),
    /// Copy the Job ID of the account's current game session.
    CopyJobId(String),
    AddAccountDialog,
    /// Double-click: quick-launch this account.
    QuickLaunch(u64),
}

/// Sidebar result: an optional action and the list of currently visible user IDs.
pub struct SidebarResult {
    pub action: Option<SidebarAction>,
    pub visible_user_ids: Vec<u64>,
}

/// Draw the sidebar. Returns action + visible account IDs.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut SidebarState,
    accounts: &[Account],
    selected_ids: &HashSet<u64>,
) -> SidebarResult {
    let mut action: Option<SidebarAction> = None;

    ui.vertical(|ui| {
        ui.heading("Accounts");
        ui.separator();

        // Search bar
        ui.horizontal(|ui| {
            ui.label("\u{1f50d}");
            ui.text_edit_singleline(&mut state.search_query);
        });

        // Sort selector
        ui.horizontal(|ui| {
            ui.label("Sort:");
            egui::ComboBox::from_id_salt("sort_order")
                .selected_text(state.sort_order.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.sort_order, SortOrder::Name, "Name");
                    ui.selectable_value(&mut state.sort_order, SortOrder::Status, "Status");
                });
        });
        ui.add_space(4.0);

        // Add account button
        if ui.button("\u{2795}  Add Account").clicked() {
            action = Some(SidebarAction::AddAccountDialog);
        }

        // Selection count badge
        if selected_ids.len() > 1 {
            ui.colored_label(
                egui::Color32::from_rgb(130, 180, 255),
                format!("{} selected", selected_ids.len()),
            );
        }

        ui.separator();

        // Build filtered list
        let query = state.search_query.to_lowercase();
        let mut filtered: Vec<(usize, &Account)> = accounts
            .iter()
            .enumerate()
            .filter(|(_, account)| {
                if query.is_empty() {
                    return true;
                }
                account.username.to_lowercase().contains(&query)
                    || account.display_name.to_lowercase().contains(&query)
                    || account.alias.to_lowercase().contains(&query)
            })
            .collect();

        // Sort the filtered list
        match state.sort_order {
            SortOrder::Name => {
                filtered.sort_by(|(_, a), (_, b)| {
                    a.label().to_lowercase().cmp(&b.label().to_lowercase())
                });
            }
            SortOrder::Status => {
                // Higher presence type = more active; sort active first
                filtered.sort_by(|(_, a), (_, b)| {
                    b.last_presence
                        .user_presence_type
                        .cmp(&a.last_presence.user_presence_type)
                        .then_with(|| a.label().to_lowercase().cmp(&b.label().to_lowercase()))
                });
            }
        }

        // Account list
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (filter_idx, (_, account)) in filtered.iter().enumerate() {
                    let is_selected = selected_ids.contains(&account.user_id);
                    let has_subtitle = account.alias.is_empty()
                        && account.display_name != account.username;
                    let row_height = if has_subtitle { 36.0 } else { 24.0 };

                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), row_height),
                        egui::Sense::click(),
                    );

                    // Background highlight for selected / hovered
                    if is_selected {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            ui.style().visuals.selection.bg_fill,
                        );
                    } else if response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            2.0,
                            ui.style().visuals.widgets.hovered.bg_fill,
                        );
                    }

                    let painter = ui.painter_at(rect);

                    // Presence indicator dot
                    let dot_color = if account.cookie_expired {
                        egui::Color32::from_rgb(200, 60, 60) // Red for expired
                    } else {
                        presence_color(account.last_presence.user_presence_type)
                    };
                    painter.circle_filled(
                        egui::pos2(rect.min.x + 8.0, rect.min.y + 12.0),
                        4.0,
                        dot_color,
                    );

                    // Expired cookie warning icon
                    if account.cookie_expired {
                        painter.text(
                            egui::pos2(rect.min.x + 8.0, rect.min.y + 12.0),
                            egui::Align2::CENTER_CENTER,
                            "!",
                            egui::FontId::proportional(7.0),
                            egui::Color32::WHITE,
                        );
                    }

                    // Account label
                    let label_pos = egui::pos2(rect.min.x + 20.0, rect.min.y + 3.0);
                    painter.text(
                        label_pos,
                        egui::Align2::LEFT_TOP,
                        account.label(),
                        egui::FontId::proportional(14.0),
                        ui.style().visuals.text_color(),
                    );

                    // Display name subtitle (if different from label)
                    if has_subtitle {
                        let sub_pos = egui::pos2(rect.min.x + 20.0, rect.min.y + 19.0);
                        painter.text(
                            sub_pos,
                            egui::Align2::LEFT_TOP,
                            &account.display_name,
                            egui::FontId::proportional(11.0),
                            egui::Color32::GRAY,
                        );
                    }

                    if response.clicked() {
                        let modifiers = ui.input(|i| i.modifiers);
                        if modifiers.ctrl || modifiers.mac_cmd {
                            // Ctrl+click: toggle this account
                            action = Some(SidebarAction::ToggleSelect(account.user_id));
                            state.last_clicked_index = Some(filter_idx);
                        } else if modifiers.shift {
                            // Shift+click: range select
                            let anchor = state.last_clicked_index.unwrap_or(0);
                            let lo = anchor.min(filter_idx);
                            let hi = anchor.max(filter_idx);
                            let range_ids: Vec<u64> = filtered[lo..=hi]
                                .iter()
                                .map(|(_, a)| a.user_id)
                                .collect();
                            action = Some(SidebarAction::RangeSelect(range_ids));
                        } else {
                            // Plain click: single select
                            action = Some(SidebarAction::Select(account.user_id));
                            state.last_clicked_index = Some(filter_idx);
                        }
                    }

                    // Double-click: quick launch
                    if response.double_clicked() {
                        action = Some(SidebarAction::QuickLaunch(account.user_id));
                    }

                    // Right-click context menu
                    response.context_menu(|ui| {
                        if let Some(ref gid) = account.last_presence.game_id {
                            if account.last_presence.user_presence_type == 2 {
                                if ui.button("\u{1f4cb}  Copy Job ID").clicked() {
                                    action = Some(SidebarAction::CopyJobId(gid.clone()));
                                    ui.close_menu();
                                }
                                if let Some(pid) = account.last_presence.place_id {
                                    if ui.button("\u{1f4cb}  Copy Place ID").clicked() {
                                        action = Some(SidebarAction::CopyJobId(pid.to_string()));
                                        ui.close_menu();
                                    }
                                }
                                ui.separator();
                            }
                        }
                        ui.label(format!("@{}", account.username));
                        ui.label(format!("ID: {}", account.user_id));
                    });

                    // Tooltip with presence
                    response.on_hover_text(format!(
                        "{} \u{2014} {}",
                        account.username,
                        account.last_presence.status_text()
                    ));
                }
            });
    });

    let visible_user_ids = {
        let query = state.search_query.to_lowercase();
        accounts
            .iter()
            .filter(|a| {
                if query.is_empty() {
                    return true;
                }
                a.username.to_lowercase().contains(&query)
                    || a.display_name.to_lowercase().contains(&query)
                    || a.alias.to_lowercase().contains(&query)
            })
            .map(|a| a.user_id)
            .collect()
    };

    SidebarResult {
        action,
        visible_user_ids,
    }
}

fn presence_color(presence_type: u8) -> egui::Color32 {
    match presence_type {
        1 => egui::Color32::from_rgb(60, 180, 75),   // Online — green
        2 => egui::Color32::from_rgb(30, 144, 255),   // In Game — blue
        3 => egui::Color32::from_rgb(255, 165, 0),    // In Studio — orange
        _ => egui::Color32::from_rgb(130, 130, 130),  // Offline — gray
    }
}
