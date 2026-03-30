//! Toast notification system for non-blocking user feedback.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created: Instant,
    pub duration: Duration,
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created: Instant::now(),
            duration: Duration::from_secs(4),
        }
    }

    pub fn info(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Info)
    }

    pub fn success(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Success)
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Error)
    }

    #[allow(dead_code)]
    pub fn warning(msg: impl Into<String>) -> Self {
        Self::new(msg, ToastLevel::Warning)
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= self.duration
    }
}

/// Manages the queue of active toasts.
#[derive(Default)]
pub struct Toasts {
    queue: VecDeque<Toast>,
}

impl Toasts {
    pub fn push(&mut self, toast: Toast) {
        self.queue.push_back(toast);
    }

    /// Remove expired toasts and return the active ones.
    #[allow(dead_code)]
    pub fn active(&mut self) -> impl Iterator<Item = &Toast> {
        self.queue.retain(|t| !t.is_expired());
        self.queue.iter()
    }

    /// Render toast overlays in the bottom-right of the screen.
    pub fn show(&mut self, ctx: &eframe::egui::Context) {
        use eframe::egui;

        self.queue.retain(|t| !t.is_expired());

        if self.queue.is_empty() {
            return;
        }

        // Request continuous repaint while toasts are visible
        ctx.request_repaint();

        let screen = ctx.screen_rect();
        let mut y = screen.max.y - 10.0;

        for toast in self.queue.iter().rev() {
            let color = match toast.level {
                ToastLevel::Info => egui::Color32::from_rgb(60, 120, 200),
                ToastLevel::Success => egui::Color32::from_rgb(50, 170, 80),
                ToastLevel::Warning => egui::Color32::from_rgb(220, 160, 40),
                ToastLevel::Error => egui::Color32::from_rgb(200, 60, 60),
            };

            let id = egui::Id::new(toast.created);
            let width = 300.0;
            let height = 36.0;
            y -= height + 4.0;

            let rect = egui::Rect::from_min_size(
                egui::pos2(screen.max.x - width - 10.0, y),
                egui::vec2(width, height),
            );

            egui::Area::new(id)
                .fixed_pos(rect.min)
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    let frame = egui::Frame::default()
                        .fill(color)
                        .rounding(4.0)
                        .inner_margin(8.0);
                    frame.show(ui, |ui| {
                        ui.set_min_width(width - 16.0);
                        ui.colored_label(egui::Color32::WHITE, &toast.message);
                    });
                });
        }
    }
}
