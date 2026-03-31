//! Interactive first-launch tutorial.
//!
//! Each step highlights a specific widget (pulsing border drawn on the
//! foreground layer) and shows a small callout panel positioned next to it.
//! Steps advance automatically when the user performs the expected action
//! (adds an account, selects one, etc.) or manually via Back / Next buttons.
//!
//! # Rect reporting
//! Components write their key widget rects into [`TutorialState`] each frame
//! so the overlay always reflects the current layout:
//!   - `sidebar::show` → `state.add_btn_rect`
//!   - `main_panel::show` → `state.launch_btn_rect`
//!   - `app::show_add_dialog` → `state.cookie_field_rect`

use eframe::egui;

// ---------------------------------------------------------------------------
// Step enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TutorialStep {
    /// Intro — no widget highlighted, centered callout.
    Welcome,
    /// Highlight the "Add Account" sidebar button. Advances when the dialog opens.
    AddAccount,
    /// Highlight the cookie input inside the add-account dialog. Advances when
    /// an account is successfully added.
    EnterCookie,
    /// Highlight the sidebar account list. Advances when an account is selected.
    SelectAccount,
    /// Highlight the Launch button. "Next" button advances manually.
    LaunchGame,
    /// Final screen — no highlight, centered callout with Finish button.
    Done,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Tutorial state stored on `AppState`. Components write their key widget
/// rects into this struct each frame so the overlay is always accurate.
pub struct TutorialState {
    pub active: bool,
    pub step: TutorialStep,

    // Widget rects reported each frame by components.
    pub add_btn_rect: egui::Rect,
    pub launch_btn_rect: egui::Rect,
    pub cookie_field_rect: egui::Rect,
    pub sidebar_accounts_rect: egui::Rect,
}

impl Default for TutorialState {
    fn default() -> Self {
        Self {
            active: false,
            step: TutorialStep::Welcome,
            add_btn_rect: egui::Rect::NOTHING,
            launch_btn_rect: egui::Rect::NOTHING,
            cookie_field_rect: egui::Rect::NOTHING,
            sidebar_accounts_rect: egui::Rect::NOTHING,
        }
    }
}

impl TutorialState {
    pub fn start() -> Self {
        Self {
            active: true,
            step: TutorialStep::Welcome,
            ..Self::default()
        }
    }

    /// Advance to the next step if we are currently on `from`.
    pub fn advance_from(&mut self, from: TutorialStep) {
        if self.active && self.step == from {
            self.step = match from {
                TutorialStep::Welcome => TutorialStep::AddAccount,
                TutorialStep::AddAccount => TutorialStep::EnterCookie,
                TutorialStep::EnterCookie => TutorialStep::SelectAccount,
                TutorialStep::SelectAccount => TutorialStep::LaunchGame,
                TutorialStep::LaunchGame => TutorialStep::Done,
                TutorialStep::Done => {
                    self.active = false;
                    TutorialStep::Done
                }
            };
        }
    }

    fn target_rect(&self) -> egui::Rect {
        match self.step {
            TutorialStep::AddAccount => self.add_btn_rect,
            TutorialStep::EnterCookie => self.cookie_field_rect,
            TutorialStep::SelectAccount => self.sidebar_accounts_rect,
            TutorialStep::LaunchGame => self.launch_btn_rect,
            TutorialStep::Welcome | TutorialStep::Done => egui::Rect::NOTHING,
        }
    }

    fn step_label(&self) -> &'static str {
        match self.step {
            TutorialStep::Welcome => "1 / 6",
            TutorialStep::AddAccount => "2 / 6",
            TutorialStep::EnterCookie => "3 / 6",
            TutorialStep::SelectAccount => "4 / 6",
            TutorialStep::LaunchGame => "5 / 6",
            TutorialStep::Done => "6 / 6",
        }
    }

    fn callout_content(&self) -> (&'static str, &'static str, bool, bool) {
        // (title, body, show_next_btn, show_back_btn)
        match self.step {
            TutorialStep::Welcome => (
                "Welcome to RM",
                "RM lets you manage multiple Roblox accounts, launch games, and save \
                 private servers, all from one window.\n\
                 Click Next to start the tour.",
                true,
                false,
            ),
            TutorialStep::AddAccount => (
                "Add your first account",
                "Click the highlighted button to open the Add Account dialog.",
                false,
                true,
            ),
            TutorialStep::EnterCookie => (
                "Enter your cookie",
                "Paste your .ROBLOSECURITY cookie into the highlighted field, then \
                 click Add. In Chrome: press F12, open Application > Cookies, \
                 find .ROBLOSECURITY, and copy its value.",
                false,
                false,
            ),
            TutorialStep::SelectAccount => (
                "Select an account",
                "Your account now appears in the sidebar. Click it to select it.",
                false,
                false,
            ),
            TutorialStep::LaunchGame => (
                "Launch Roblox",
                "Enter a Place ID in the box and click Launch to start Roblox with \
                 this account. You can also save Favourite Places for quicker access.",
                true,
                true,
            ),
            TutorialStep::Done => (
                "You're all set!",
                "Use the Private Servers tab to save VIP server links, and the \
                 Settings tab to configure multi-instance mode and other options.\n\
                 Enjoy using RM!",
                false,
                true,
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Overlay rendering
// ---------------------------------------------------------------------------

/// Call this every frame after all other widgets are rendered.
pub fn show_overlay(ctx: &egui::Context, state: &mut TutorialState) {
    if !state.active {
        return;
    }

    let target = state.target_rect();
    let has_target = target != egui::Rect::NOTHING && target.is_finite();

    // ---- Pulsing highlight border ----
    if has_target {
        let t = (ctx.input(|i| i.time) * std::f64::consts::PI * 1.6).sin() as f32 * 0.5 + 0.5;
        let r = (220.0 + 35.0 * t) as u8;
        let g = (180.0 - 60.0 * t) as u8;
        let b = 50_u8;
        let color = egui::Color32::from_rgb(r, g, b);

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("tutorial_highlight"),
        ));
        let padded = target.expand(4.0);
        painter.rect_stroke(
            padded,
            egui::Rounding::same(6.0),
            egui::Stroke::new(2.5, color),
        );
        // Request repaint so the animation keeps running
        ctx.request_repaint();
    }

    // ---- Callout panel ----
    let (title, body, show_next, show_back) = state.callout_content();
    let step_label = state.step_label();
    let is_done = state.step == TutorialStep::Done;

    // Position: below the target rect if available, otherwise screen center
    let screen = ctx.screen_rect();
    let anchor_pos = if has_target {
        let below = egui::pos2(target.center().x, target.max.y + 12.0);
        // Clamp horizontally so the callout doesn't go off screen
        egui::pos2(below.x.clamp(screen.min.x + 10.0, screen.max.x - 310.0), below.y)
    } else {
        egui::pos2(screen.center().x - 150.0, screen.center().y - 80.0)
    };

    let mut close = false;
    let mut next = false;
    let mut back = false;

    egui::Area::new(egui::Id::new("tutorial_callout"))
        .order(egui::Order::Tooltip)
        .fixed_pos(anchor_pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    ui.set_max_width(300.0);

                    // Step indicator
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(step_label)
                                .small()
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Label::new(
                                        egui::RichText::new("x")
                                            .small()
                                            .color(ui.visuals().weak_text_color()),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text("Skip tutorial")
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(title).strong());
                    ui.add_space(4.0);
                    ui.label(body);
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if show_back && ui.small_button("< Back").clicked() {
                            back = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if is_done {
                                if ui.button("Finish").clicked() {
                                    close = true;
                                }
                            } else if show_next && ui.button("Next >").clicked() {
                                next = true;
                            }
                        });
                    });
                });
        });

    if close {
        state.active = false;
        state.step = TutorialStep::Welcome;
    } else if next {
        state.advance_from(state.step);
    } else if back {
        state.step = match state.step {
            TutorialStep::AddAccount => TutorialStep::Welcome,
            TutorialStep::LaunchGame => TutorialStep::SelectAccount,
            TutorialStep::Done => TutorialStep::LaunchGame,
            other => other,
        };
    }
}


struct Step {
    icon: &'static str,
    title: &'static str,
    body: &'static str,
}

const STEPS: &[Step] = &[
    Step {
        icon: "👋",
        title: "Welcome to Roblox Account Manager",
        body: "RM lets you manage multiple Roblox accounts, launch games, and save \
               private servers, all from one window.\n\n\
               Use the buttons below to step through the tour, or close this window to skip.",
    },
    Step {
        icon: "👤",
        title: "Adding accounts",
        body: "Click '+ Add Account' at the top of the sidebar to add a Roblox account.\n\n\
               You will need the account's .ROBLOSECURITY cookie. In Chrome, press F12, \
               open Application > Cookies, find .ROBLOSECURITY, and copy its value.",
    },
    Step {
        icon: "🚀",
        title: "Launching Roblox",
        body: "Select an account from the sidebar and click Launch in the main panel. \
               You can save Favourite Places for quick access to games you play often.\n\n\
               To launch multiple accounts at once, enable Multi-Instance in Settings, \
               select several accounts, and use the group launch controls that appear.",
    },
    Step {
        icon: "🔒",
        title: "Private servers",
        body: "The Private Servers tab lets you save VIP and private server links. Paste a \
               share link (rbxShareLink://...) or a direct VIP server URL and RM will \
               resolve the access code for you.\n\n\
               Servers are grouped by game so they are easy to find.",
    },
    Step {
        icon: "⚙",
        title: "Settings & you're done!",
        body: "The Settings tab has options for multi-instance mode, privacy mode, \
               auto window arrangement, and credential storage.\n\n\
               That's it. Enjoy using RM!",
    },
];

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Show the first-launch tutorial wizard. Call every frame while `open` is `true`.
pub fn show(ctx: &egui::Context, step: &mut usize, open: &mut bool) {
    if !*open {
        return;
    }

    let total = STEPS.len();
    let idx = (*step).min(total.saturating_sub(1));
    let data = &STEPS[idx];

    // Track whether the window should remain open after this frame.
    // We drive close state ourselves via buttons; the egui .open() X button
    // uses a separate bool to avoid a double-mutable-borrow.
    let mut close = false;

    let mut egui_open = true; // only false if user clicks the X title-bar button
    egui::Window::new("👋  Welcome to RM")
        .open(&mut egui_open)
        .resizable(false)
        .collapsible(false)
        .min_width(430.0)
        .max_width(480.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_min_width(430.0);

            // ---- Step indicator / progress dots ----
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("Step {} of {}", idx + 1, total))
                        .color(ui.visuals().weak_text_color())
                        .small(),
                );

                // Progress dots
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for i in (0..total).rev() {
                        let active = i == idx;
                        let color = if active {
                            ui.visuals().selection.bg_fill
                        } else {
                            ui.visuals().weak_text_color()
                        };
                        let (rect, _) = ui.allocate_exact_size(
                            egui::Vec2::splat(if active { 8.0 } else { 6.0 }),
                            egui::Sense::hover(),
                        );
                        ui.painter().circle_filled(rect.center(), rect.width() / 2.0, color);
                    }
                    ui.add_space(4.0);
                });
            });

            ui.add_space(8.0);

            // ---- Icon + title ----
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(data.icon).size(28.0));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(data.title).size(16.0).strong());
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(10.0);

            // ---- Body (scrollable so window height stays stable) ----
            egui::ScrollArea::vertical()
                .id_salt("tutorial_body_scroll")
                .max_height(110.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.label(data.body);
                });

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // ---- Navigation row ----
            ui.horizontal(|ui| {
                // Skip link (left side)
                if ui
                    .add(
                        egui::Label::new(
                            egui::RichText::new("Skip tutorial")
                                .color(ui.visuals().weak_text_color()),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    close = true;
                }

                // Back / Next buttons (right side)
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_last = idx + 1 == total;
                    if ui
                        .button(if is_last { "  Finish  " } else { "  Next >  " })
                        .clicked()
                    {
                        if is_last {
                            close = true;
                        } else {
                            *step += 1;
                        }
                    }
                    if idx > 0 && ui.button("< Back").clicked() {
                        *step -= 1;
                    }
                });
            });
        });

    // Sync close state — catches both the X button and our own buttons.
    if !egui_open || close {
        *open = false;
        *step = 0;
    }
}
