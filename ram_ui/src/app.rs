//! Top-level application state and the `eframe::App` implementation that ties
//! the sidebar, main panel, settings, toast system, and backend bridge together.

use eframe::egui;
use ram_core::models::{AccountStore, AppConfig};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::bridge::{BackendBridge, BackendCommand, BackendEvent};
use crate::components::{group_panel, main_panel, settings, sidebar};
use crate::toast::{Toast, Toasts};

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Accounts,
    Settings,
}

// ---------------------------------------------------------------------------
// Add-account dialog state
// ---------------------------------------------------------------------------

#[derive(Default)]
struct AddAccountDialog {
    open: bool,
    cookie_input: String,
    /// Staging field for password — only committed on submit.
    password_input: String,
    /// True while we're waiting for the backend to validate.
    loading: bool,
    /// Error message from the last failed attempt.
    last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    config: AppConfig,
    config_path: PathBuf,
    store: AccountStore,
    master_password: String,

    bridge: BackendBridge,
    toasts: Toasts,

    // UI state
    active_tab: Tab,
    selected_ids: HashSet<u64>,
    sidebar_state: sidebar::SidebarState,
    main_panel_state: main_panel::MainPanelState,
    group_panel_state: group_panel::GroupPanelState,
    settings_state: settings::SettingsState,
    add_dialog: AddAccountDialog,

    /// Downloaded avatar image bytes, keyed by user ID.
    avatar_bytes: HashMap<u64, Vec<u8>>,

    /// User IDs currently visible in the sidebar (after search filtering).
    visible_user_ids: Vec<u64>,

    /// Cached flag from sysinfo (refreshed lazily).
    roblox_running: bool,
    /// Frame counter to throttle background refreshes.
    frame_count: u64,

    /// Password prompt shown on first launch when store file exists.
    needs_unlock: bool,
    unlock_password_input: String,

    /// When set, shows a confirmation dialog before removing the account.
    confirm_remove: Option<u64>,

    /// Available update info: (version, release_url).
    update_available: Option<(String, String)>,
    /// Show the "What's New" changelog window.
    show_changelog: bool,
}

impl AppState {
    pub fn new(mut config: AppConfig, config_path: PathBuf) -> Self {
        let bridge = BackendBridge::spawn();
        let needs_unlock = config.accounts_path.is_file();

        // If multi-instance was previously enabled, run the same validation as
        // the UI toggle: kill tray processes, wait, then only acquire the mutex
        // if no Roblox instances remain.
        if config.multi_instance_enabled {
            ram_core::process::kill_tray_roblox();
            std::thread::sleep(std::time::Duration::from_millis(500));
            if ram_core::process::is_roblox_running() {
                tracing::warn!(
                    "Roblox is running at startup — cannot acquire singleton mutex. \
                     Disabling multi-instance until manually re-enabled."
                );
                config.multi_instance_enabled = false;
            } else if let Err(e) = ram_core::process::enable_multi_instance() {
                tracing::warn!("Failed to acquire singleton mutex at startup: {e}");
                config.multi_instance_enabled = false;
            }
        }

        let mut state = Self {
            config,
            config_path,
            store: AccountStore::default(),
            master_password: String::new(),
            bridge,
            toasts: Toasts::default(),
            active_tab: Tab::Accounts,
            selected_ids: HashSet::new(),
            sidebar_state: sidebar::SidebarState::default(),
            main_panel_state: main_panel::MainPanelState::default(),
            group_panel_state: group_panel::GroupPanelState::default(),
            settings_state: settings::SettingsState::default(),
            add_dialog: AddAccountDialog::default(),
            avatar_bytes: HashMap::new(),
            visible_user_ids: Vec::new(),
            roblox_running: false,
            frame_count: 0,
            needs_unlock,
            unlock_password_input: String::new(),
            confirm_remove: None,
            update_available: None,
            show_changelog: false,
        };

        // Check for updates on startup
        state.bridge.send(BackendCommand::CheckForUpdates {
            current_version: env!("CARGO_PKG_VERSION").to_string(),
        });

        // Detect first launch after update
        let current = env!("CARGO_PKG_VERSION");
        let is_new_version = state.config.last_seen_version.as_deref() != Some(current);
        if is_new_version && state.config.last_seen_version.is_some() {
            // Upgraded from a previous version — show changelog
            state.show_changelog = true;
        }
        // Always update the stored version
        state.config.last_seen_version = Some(current.to_string());
        let _ = state.config.save(&state.config_path);

        state
    }

    // ---- Event processing ----

    fn process_events(&mut self) {
        for event in self.bridge.poll() {
            match event {
                BackendEvent::AccountValidated {
                    account,
                    encrypted_cookie: _,
                } => {
                    let name = if self.config.anonymize_names {
                        "Account".to_string()
                    } else {
                        account.username.clone()
                    };
                    // Avoid duplicates
                    self.store.remove_by_id(account.user_id);
                    self.store.accounts.push(*account);
                    self.toasts.push(Toast::success(format!("Added {name}")));
                    self.add_dialog.loading = false;
                    self.add_dialog.last_error = None;
                    self.auto_save();
                }
                BackendEvent::AccountRemoved { user_id } => {
                    self.store.remove_by_id(user_id);
                    self.selected_ids.remove(&user_id);
                    self.toasts.push(Toast::info("Account removed"));
                    self.auto_save();
                }
                BackendEvent::AvatarsUpdated(avatars) => {
                    for (id, url) in avatars {
                        if let Some(acc) = self.store.find_by_id_mut(id) {
                            acc.avatar_url = url;
                        }
                    }
                }
                BackendEvent::AvatarImagesReady(images) => {
                    for (id, bytes) in images {
                        self.avatar_bytes.insert(id, bytes);
                    }
                }
                BackendEvent::PresencesUpdated(presences) => {
                    for (id, p) in presences {
                        if let Some(acc) = self.store.find_by_id_mut(id) {
                            acc.last_presence = p;
                        }
                    }
                }
                BackendEvent::GameLaunched => {
                    self.toasts.push(Toast::success("Game launched"));
                    if self.config.auto_arrange_windows {
                        self.bridge.send(BackendCommand::ArrangeWindows);
                    }
                }
                BackendEvent::BulkLaunchProgress { launched, total } => {
                    self.toasts
                        .push(Toast::info(format!("Launching {launched}/{total}...")));
                }
                BackendEvent::BulkLaunchComplete { launched, failed } => {
                    if failed == 0 {
                        self.toasts.push(Toast::success(format!(
                            "Bulk launch complete — {launched} launched"
                        )));
                    } else {
                        self.toasts.push(Toast::error(format!(
                            "Bulk launch done — {launched} launched, {failed} failed"
                        )));
                    }
                    if self.config.auto_arrange_windows {
                        self.bridge.send(BackendCommand::ArrangeWindows);
                    }
                }
                BackendEvent::StoreSaved => {
                    // silent
                }
                BackendEvent::StoreLoaded(store) => {
                    self.store = store;
                    self.needs_unlock = false;
                    self.toasts
                        .push(Toast::success("Account store unlocked"));
                    self.trigger_refresh();
                    self.trigger_revalidation();
                }
                BackendEvent::Killed(count) => {
                    self.toasts
                        .push(Toast::info(format!("Killed {count} instance(s)")));
                }
                BackendEvent::WindowsArranged => {
                    // silent — arrangement complete
                }
                BackendEvent::AccountRevalidated {
                    user_id,
                    valid,
                    username,
                    display_name,
                } => {
                    if let Some(acc) = self.store.find_by_id_mut(user_id) {
                        if valid {
                            acc.last_validated = Some(chrono::Utc::now());
                            acc.username = username;
                            acc.display_name = display_name;
                            acc.cookie_expired = false;
                        } else {
                            acc.cookie_expired = true;
                        }
                    }
                    self.auto_save();
                    if !valid {
                        if let Some(acc) = self.store.find_by_id(user_id) {
                            let label = if self.config.anonymize_names {
                                "An account".to_string()
                            } else {
                                acc.label().to_string()
                            };
                            self.toasts.push(Toast::error(format!(
                                "Cookie expired for {label} — re-add with a fresh cookie"
                            )));
                        }
                    }
                }
                BackendEvent::Error(msg) => {
                    // If the add dialog is loading, show error there for retry
                    if self.add_dialog.loading {
                        self.add_dialog.loading = false;
                        self.add_dialog.last_error = Some(msg.clone());
                    }
                    self.toasts.push(Toast::error(msg));
                }
                BackendEvent::UpdateAvailable { version, url } => {
                    self.update_available = Some((version, url));
                }
            }
        }
    }

    fn auto_save(&self) {
        if !self.master_password.is_empty() {
            self.bridge.send(BackendCommand::SaveStore {
                store: self.store.clone(),
                path: self.config.accounts_path.clone(),
                password: self.master_password.clone(),
            });
        }
    }

    /// Get the first available cookie for API calls (decrypted from credential
    /// manager or in-memory encrypted cookie).
    fn first_account_with_cookie(&self) -> Option<&ram_core::models::Account> {
        self.store.accounts.iter().find(|a| {
            self.config.use_credential_manager || a.encrypted_cookie.is_some()
        })
    }

    fn trigger_refresh(&self) {
        let user_ids: Vec<u64> = self.store.accounts.iter().map(|a| a.user_id).collect();
        if user_ids.is_empty() {
            return;
        }
        if let Some(first) = self.first_account_with_cookie() {
            self.bridge.send(BackendCommand::RefreshAll {
                user_ids,
                first_user_id: first.user_id,
                encrypted_cookie: first.encrypted_cookie.clone(),
                password: self.master_password.clone(),
                use_credential_manager: self.config.use_credential_manager,
            });
        }
    }

    /// Lightweight presence-only refresh for the currently visible accounts.
    fn trigger_presence_refresh(&self) {
        if self.visible_user_ids.is_empty() {
            return;
        }
        if let Some(first) = self.first_account_with_cookie() {
            self.bridge.send(BackendCommand::RefreshPresenceOnly {
                user_ids: self.visible_user_ids.clone(),
                first_user_id: first.user_id,
                encrypted_cookie: first.encrypted_cookie.clone(),
                password: self.master_password.clone(),
                use_credential_manager: self.config.use_credential_manager,
            });
        }
    }

    /// Revalidate all account cookies in the background.
    fn trigger_revalidation(&self) {
        if self.store.accounts.is_empty() {
            return;
        }
        let accounts: Vec<(u64, Option<String>)> = self
            .store
            .accounts
            .iter()
            .map(|a| (a.user_id, a.encrypted_cookie.clone()))
            .collect();
        self.bridge.send(BackendCommand::RevalidateAll {
            accounts,
            password: self.master_password.clone(),
            use_credential_manager: self.config.use_credential_manager,
        });
    }
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for AppState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame_count += 1;
        self.process_events();

        // Periodically refresh roblox_running flag (every ~120 frames ≈ 2s)
        if self.frame_count.is_multiple_of(120) {
            self.roblox_running = ram_core::process::is_roblox_running();
        }

        // Periodically kill background tray Roblox processes when enabled
        // (every ~600 frames ≈ 10s)
        if (self.config.kill_background_roblox || self.config.multi_instance_enabled)
            && self.frame_count.is_multiple_of(600)
        {
            ram_core::process::kill_tray_roblox();
        }

        // Periodically refresh presence for visible accounts (every ~600 frames ≈ 10s)
        if self.frame_count.is_multiple_of(600) && !self.visible_user_ids.is_empty() {
            self.trigger_presence_refresh();
        }

        // Periodically refresh avatars for all accounts (every ~3600 frames ≈ 60s)
        if self.frame_count % 3600 == 300 && !self.store.accounts.is_empty() {
            self.trigger_refresh();
        }

        // Periodically revalidate all account cookies (every ~18000 frames ≈ 5 min)
        if self.frame_count % 18000 == 900 && !self.store.accounts.is_empty() {
            self.trigger_revalidation();
        }

        // ---- Unlock screen ----
        if self.needs_unlock {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("🔒 RM | Unlock Account Store");
                    ui.add_space(16.0);
                    ui.label("Enter your master password to decrypt accounts:");
                    ui.add_space(8.0);

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.unlock_password_input)
                            .password(true)
                            .hint_text("Master password"),
                    );

                    ui.add_space(8.0);
                    let enter_pressed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    if ui.button("Unlock").clicked() || enter_pressed {
                        let pw = self.unlock_password_input.clone();
                        self.master_password = pw.clone();
                        self.bridge.send(BackendCommand::LoadStore {
                            path: self.config.accounts_path.clone(),
                            password: pw,
                        });
                    }
                });
            });
            self.toasts.show(ctx);
            return;
        }

        // ---- Top bar ----
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, Tab::Accounts, "📋 Accounts");
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "⚙ Settings");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((ref version, ref url)) = self.update_available {
                        let text = format!("⬆ Update v{version} available");
                        if ui.link(text).on_hover_text("Click to open the download page").clicked() {
                            ui.output_mut(|o| o.open_url = Some(egui::output::OpenUrl::new_tab(url)));
                        }
                        ui.separator();
                    }
                    if self.roblox_running {
                        ui.colored_label(
                            egui::Color32::from_rgb(30, 144, 255),
                            "● Roblox Running",
                        );
                    }
                    ui.label(format!("{} account(s)", self.store.accounts.len()));
                });
            });
        });

        // ---- Status bar ----
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(format!("{} account(s)", self.store.accounts.len()));
                    ui.separator();
                    if self.roblox_running {
                        let count = ram_core::process::roblox_instance_count();
                        ui.colored_label(
                            egui::Color32::from_rgb(30, 144, 255),
                            format!("● {count} Roblox instance(s)"),
                        );
                    } else {
                        ui.colored_label(egui::Color32::GRAY, "○ Roblox not running");
                    }
                    ui.separator();
                    ui.label(format!("{} selected", self.selected_ids.len()));
                });
            });

        match self.active_tab {
            Tab::Accounts => self.show_accounts_tab(ctx),
            Tab::Settings => self.show_settings_tab(ctx),
        }

        // ---- Floating add-account dialog ----
        self.show_add_dialog(ctx);

        // ---- Confirmation dialog for account removal ----
        self.show_confirm_remove_dialog(ctx);

        // ---- Changelog window ----
        self.show_changelog_window(ctx);

        // ---- Toasts ----
        self.toasts.show(ctx);
    }
}

// ---------------------------------------------------------------------------
// Tab rendering
// ---------------------------------------------------------------------------

impl AppState {
    fn show_accounts_tab(&mut self, ctx: &egui::Context) {
        // Sidebar
        egui::SidePanel::left("sidebar")
            .default_width(220.0)
            .width_range(140.0..=400.0)
            .resizable(true)
            .show(ctx, |ui| {
                let result = sidebar::show(
                    ui,
                    &mut self.sidebar_state,
                    &self.store.accounts,
                    &self.selected_ids,
                    self.config.anonymize_names,
                );
                self.visible_user_ids = result.visible_user_ids;
                if let Some(a) = result.action {
                    match a {
                        sidebar::SidebarAction::Select(id) => {
                            self.selected_ids.clear();
                            self.selected_ids.insert(id);
                        }
                        sidebar::SidebarAction::ToggleSelect(id) => {
                            if self.selected_ids.contains(&id) {
                                self.selected_ids.remove(&id);
                            } else {
                                self.selected_ids.insert(id);
                            }
                        }
                        sidebar::SidebarAction::RangeSelect(ids) => {
                            for id in ids {
                                self.selected_ids.insert(id);
                            }
                        }
                        sidebar::SidebarAction::AddAccountDialog => {
                            self.add_dialog.open = true;
                            self.add_dialog.cookie_input.clear();
                            self.add_dialog.last_error = None;
                            self.add_dialog.loading = false;
                            self.add_dialog.password_input = self.master_password.clone();
                        }
                        sidebar::SidebarAction::CopyJobId(job_id) => {
                            ui.output_mut(|o| o.copied_text = job_id.clone());
                            self.toasts.push(Toast::info("Copied to clipboard"));
                        }
                        sidebar::SidebarAction::QuickLaunch(user_id) => {
                            // Use the first favorite place, or fall back to the main panel place_id_input
                            let place_id = self
                                .config
                                .favorite_places
                                .first()
                                .map(|f| f.place_id)
                                .or_else(|| self.main_panel_state.place_id_input.parse::<u64>().ok());
                            if let Some(place_id) = place_id {
                                if let Some(acc) = self.store.find_by_id(user_id) {
                                    self.bridge.send(BackendCommand::LaunchGameEncrypted {
                                        user_id: acc.user_id,
                                        encrypted_cookie: acc.encrypted_cookie.clone(),
                                        password: self.master_password.clone(),
                                        use_credential_manager: self.config.use_credential_manager,
                                        place_id,
                                        job_id: None,
                                        multi_instance: self.config.multi_instance_enabled,
                                        kill_background: self.config.kill_background_roblox,
                                        privacy_mode: self.config.privacy_mode,
                                    });
                                }
                            } else {
                                self.toasts.push(Toast::error(
                                    "No favorite place or Place ID set — enter one first",
                                ));
                            }
                        }
                    }
                }
            });

        // Main panel — single selection shows detail, multi shows group panel
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.selected_ids.len() > 1 {
                // Group control panel
                let selected_accounts: Vec<&ram_core::models::Account> = self
                    .store
                    .accounts
                    .iter()
                    .filter(|a| self.selected_ids.contains(&a.user_id))
                    .collect();
                let action = group_panel::show(
                    ui,
                    &selected_accounts,
                    &mut self.group_panel_state,
                    self.roblox_running,
                    self.config.anonymize_names,
                );
                if let Some(a) = action {
                    match a {
                        group_panel::GroupPanelAction::BulkLaunch { place_id, job_id } => {
                            let accounts: Vec<(u64, Option<String>)> = self
                                .store
                                .accounts
                                .iter()
                                .filter(|a| self.selected_ids.contains(&a.user_id))
                                .map(|a| (a.user_id, a.encrypted_cookie.clone()))
                                .collect();
                            self.bridge.send(BackendCommand::BulkLaunchEncrypted {
                                accounts,
                                password: self.master_password.clone(),
                                use_credential_manager: self.config.use_credential_manager,
                                place_id,
                                job_id,
                                multi_instance: self.config.multi_instance_enabled,
                                kill_background: self.config.kill_background_roblox,
                                privacy_mode: self.config.privacy_mode,
                            });
                        }
                        group_panel::GroupPanelAction::ClearSelection => {
                            self.selected_ids.clear();
                        }
                        group_panel::GroupPanelAction::KillAll => {
                            self.bridge.send(BackendCommand::KillAll);
                        }
                    }
                }
            } else if self.selected_ids.len() == 1 {
                let id = *self.selected_ids.iter().next().unwrap();
                let account = self.store.find_by_id(id).cloned();
                if let Some(account) = account {
                    let avatar_bytes = self.avatar_bytes.get(&account.user_id);
                    let action = main_panel::show(
                        ui,
                        &account,
                        &mut self.main_panel_state,
                        self.roblox_running,
                        avatar_bytes,
                        &self.config.favorite_places,
                        self.config.anonymize_names,
                    );
                    if let Some(a) = action {
                        match a {
                            main_panel::MainPanelAction::LaunchGame { place_id, job_id } => {
                                self.bridge.send(BackendCommand::LaunchGameEncrypted {
                                    user_id: account.user_id,
                                    encrypted_cookie: account.encrypted_cookie.clone(),
                                    password: self.master_password.clone(),
                                    use_credential_manager: self.config.use_credential_manager,
                                    place_id,
                                    job_id,
                                    multi_instance: self.config.multi_instance_enabled,
                                    kill_background: self.config.kill_background_roblox,
                                    privacy_mode: self.config.privacy_mode,
                                });
                            }
                            main_panel::MainPanelAction::RemoveAccount(uid) => {
                                self.confirm_remove = Some(uid);
                            }
                            main_panel::MainPanelAction::UpdateAlias { user_id, alias } => {
                                if let Some(acc) = self.store.find_by_id_mut(user_id) {
                                    acc.alias = alias;
                                }
                                self.auto_save();
                            }
                            main_panel::MainPanelAction::SaveFavorite { name, place_id } => {
                                self.config.favorite_places.push(
                                    ram_core::models::FavoritePlace { name, place_id },
                                );
                                let _ = self.config.save(&self.config_path);
                                self.toasts.push(Toast::success("Favorite saved"));
                            }
                            main_panel::MainPanelAction::RemoveFavorite(index) => {
                                if index < self.config.favorite_places.len() {
                                    self.config.favorite_places.remove(index);
                                    let _ = self.config.save(&self.config_path);
                                    self.toasts.push(Toast::info("Favorite removed"));
                                }
                            }
                            main_panel::MainPanelAction::KillAll => {
                                self.bridge.send(BackendCommand::KillAll);
                            }
                        }
                    }
                } else {
                    main_panel::show_empty(ui);
                }
            } else {
                main_panel::show_empty(ui);
            }
        });

        // ---- Keyboard shortcuts ----
        let any_text_focused = ctx.memory(|m| m.focused().is_some());
        ctx.input(|i| {
            // Ctrl+A: select all accounts
            if i.modifiers.ctrl && i.key_pressed(egui::Key::A) && !any_text_focused {
                for acc in &self.store.accounts {
                    self.selected_ids.insert(acc.user_id);
                }
            }
            // Escape: clear selection
            if i.key_pressed(egui::Key::Escape) {
                self.selected_ids.clear();
            }
            // Delete: prompt to remove selected account(s)
            if i.key_pressed(egui::Key::Delete) && !any_text_focused
                && self.selected_ids.len() == 1
            {
                let uid = *self.selected_ids.iter().next().unwrap();
                self.confirm_remove = Some(uid);
            }
        });
    }

    fn show_settings_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let has_password = !self.master_password.is_empty();
            let action = settings::show(
                ui,
                &mut self.config,
                has_password,
                &mut self.settings_state,
                self.roblox_running,
            );
            match action {
                Some(settings::SettingsAction::SaveConfig) => {
                    if let Err(e) = self.config.save(&self.config_path) {
                        self.toasts
                            .push(Toast::error(format!("Save failed: {e}")));
                    } else {
                        self.toasts.push(Toast::success("Settings saved"));
                    }
                }
                Some(settings::SettingsAction::EnableMultiInstance) => {
                    if self.roblox_running {
                        // Kill tray processes first, then check again
                        ram_core::process::kill_tray_roblox();
                        // Brief wait for the OS to reap terminated processes
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        // Re-check after killing tray processes
                        let still_running = ram_core::process::is_roblox_running();
                        if still_running {
                            self.toasts.push(Toast::error(
                                "Close all Roblox instances (including tray) before enabling multi-instance.",
                            ));
                            // Don't enable — the checkbox was toggled but we
                            // leave config unchanged, so next frame it resets.
                        } else {
                            // Tray killed, nothing else running — safe to acquire
                            match ram_core::process::enable_multi_instance() {
                                Ok(()) => {
                                    self.config.multi_instance_enabled = true;
                                    self.toasts.push(Toast::success("Multi-instance enabled"));
                                }
                                Err(e) => {
                                    self.toasts.push(Toast::error(format!("Failed: {e}")));
                                }
                            }
                        }
                    } else {
                        match ram_core::process::enable_multi_instance() {
                            Ok(()) => {
                                self.config.multi_instance_enabled = true;
                                self.toasts.push(Toast::success("Multi-instance enabled"));
                            }
                            Err(e) => {
                                self.toasts.push(Toast::error(format!("Failed: {e}")));
                            }
                        }
                    }
                }
                Some(settings::SettingsAction::DisableMultiInstance) => {
                    self.config.multi_instance_enabled = false;
                    self.toasts.push(Toast::info("Multi-instance disabled (takes effect after restart)"));
                }
                Some(settings::SettingsAction::ChangePassword { new_password }) => {
                    let old_password = self.master_password.clone();
                    // Re-encrypt every account's cookie with the new password
                    for account in &mut self.store.accounts {
                        if let Some(ref enc) = account.encrypted_cookie {
                            if let Ok(plain) = ram_core::crypto::decrypt_cookie(enc, &old_password) {
                                if let Ok(new_enc) = ram_core::crypto::encrypt_cookie(&plain, &new_password) {
                                    account.encrypted_cookie = Some(new_enc);
                                }
                            }
                        }
                    }
                    self.master_password = new_password;
                    self.auto_save();
                    self.toasts.push(Toast::success("Password changed - store re-encrypted"));
                }
                Some(settings::SettingsAction::ClearPassword) => {
                    self.master_password.clear();
                    self.toasts.push(Toast::info("Password cleared"));
                }
                None => {}
            }
        });
    }

    fn show_add_dialog(&mut self, ctx: &egui::Context) {
        if !self.add_dialog.open {
            return;
        }

        let mut open = self.add_dialog.open;
        egui::Window::new("Add Account")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Paste or type the .ROBLOSECURITY cookie:");
                ui.add_space(4.0);

                let cookie_edit = egui::TextEdit::multiline(&mut self.add_dialog.cookie_input)
                    .desired_rows(3)
                    .hint_text("_|WARNING:-DO-NOT-SHARE-THIS...");
                ui.add_enabled(!self.add_dialog.loading, cookie_edit);
                ui.add_space(8.0);

                // Always show password field — uses a staging buffer so
                // partial input is never committed.
                ui.label(if self.master_password.is_empty() {
                    "Set a master password for encryption:"
                } else {
                    "Master password:"
                });
                ui.add_enabled(
                    !self.add_dialog.loading,
                    egui::TextEdit::singleline(&mut self.add_dialog.password_input)
                        .password(true)
                        .hint_text("Master password"),
                );
                ui.add_space(4.0);

                // Show error from last attempt with retry option
                if let Some(err) = &self.add_dialog.last_error {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 60, 60),
                            format!("⚠ {err}"),
                        );
                    });
                    ui.add_space(4.0);
                }

                if self.add_dialog.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Validating cookie...");
                    });
                } else {
                    let valid = !self.add_dialog.cookie_input.trim().is_empty()
                        && !self.add_dialog.password_input.is_empty();

                    let button_label = if self.add_dialog.last_error.is_some() {
                        "Retry"
                    } else {
                        "Add"
                    };

                    if ui
                        .add_enabled(valid, egui::Button::new(button_label))
                        .clicked()
                    {
                        let cookie = self.add_dialog.cookie_input.trim().to_string();
                        // Commit the password only on explicit submit
                        self.master_password = self.add_dialog.password_input.clone();
                        self.add_dialog.loading = true;
                        self.add_dialog.last_error = None;
                        self.bridge.send(BackendCommand::AddAccount {
                            cookie,
                            password: self.master_password.clone(),
                            use_credential_manager: self.config.use_credential_manager,
                        });
                    }
                }
            });
        self.add_dialog.open = open;
    }

    fn show_confirm_remove_dialog(&mut self, ctx: &egui::Context) {
        let Some(uid) = self.confirm_remove else {
            return;
        };
        let label = if self.config.anonymize_names {
            "this account".to_string()
        } else {
            self.store
                .find_by_id(uid)
                .map(|a| a.label().to_string())
                .unwrap_or_else(|| uid.to_string())
        };

        let mut keep_open = true;
        egui::Window::new("Confirm Removal")
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!("Remove account \"{label}\"? This cannot be undone."));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("🗑  Remove")
                        .clicked()
                    {
                        self.bridge
                            .send(BackendCommand::RemoveAccount { user_id: uid });
                        keep_open = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep_open = false;
                    }
                });
            });
        if !keep_open {
            self.confirm_remove = None;
        }
    }

    fn show_changelog_window(&mut self, ctx: &egui::Context) {
        if !self.show_changelog {
            return;
        }
        let mut open = true;
        egui::Window::new(format!("What's New in v{}", env!("CARGO_PKG_VERSION")))
            .open(&mut open)
            .resizable(true)
            .default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        let changelog = include_str!("../../CHANGELOG.md");
                        // Show only the section for the current version
                        let current = format!("## v{}", env!("CARGO_PKG_VERSION"));
                        let section = if let Some(start) = changelog.find(&current) {
                            let rest = &changelog[start..];
                            let end = rest[current.len()..]
                                .find("\n## v")
                                .map(|i| i + current.len())
                                .unwrap_or(rest.len());
                            &rest[..end]
                        } else {
                            changelog
                        };
                        // Render markdown-lite
                        for line in section.lines() {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                ui.add_space(2.0);
                            } else if let Some(h) = trimmed.strip_prefix("### ") {
                                ui.add_space(4.0);
                                ui.strong(h);
                            } else if let Some(h) = trimmed.strip_prefix("## ") {
                                ui.heading(h);
                            } else if let Some(item) = trimmed.strip_prefix("- ") {
                                Self::render_md_line(ui, &format!("  • {item}"));
                            } else {
                                Self::render_md_line(ui, trimmed);
                            }
                        }
                    });
                ui.add_space(8.0);
                if ui.button("Close").clicked() {
                    self.show_changelog = false;
                }
            });
        if !open {
            self.show_changelog = false;
        }
    }

    /// Render a single line with **bold** spans converted to egui RichText.
    fn render_md_line(ui: &mut egui::Ui, line: &str) {
        let mut job = egui::text::LayoutJob::default();
        let style = ui.style();
        let normal_color = style.visuals.text_color();
        let normal_font = egui::FontId::proportional(14.0);
        let bold_font = egui::FontId {
            size: 14.0,
            family: egui::FontFamily::Proportional,
        };

        let mut remaining = line;
        while let Some(start) = remaining.find("**") {
            // Text before the bold marker
            let before = &remaining[..start];
            if !before.is_empty() {
                job.append(before, 0.0, egui::text::TextFormat {
                    font_id: normal_font.clone(),
                    color: normal_color,
                    ..Default::default()
                });
            }
            remaining = &remaining[start + 2..];
            // Find the closing **
            if let Some(end) = remaining.find("**") {
                let bold_text = &remaining[..end];
                job.append(bold_text, 0.0, egui::text::TextFormat {
                    font_id: bold_font.clone(),
                    color: normal_color,
                    italics: false,
                    ..Default::default()
                });
                remaining = &remaining[end + 2..];
            } else {
                // No closing ** — just emit the rest as normal
                job.append(&format!("**{remaining}"), 0.0, egui::text::TextFormat {
                    font_id: normal_font.clone(),
                    color: normal_color,
                    ..Default::default()
                });
                remaining = "";
            }
        }
        // Remaining plain text
        if !remaining.is_empty() {
            job.append(remaining, 0.0, egui::text::TextFormat {
                font_id: normal_font,
                color: normal_color,
                ..Default::default()
            });
        }
        ui.label(job);
    }
}
