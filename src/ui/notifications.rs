//! The notification bar along the bottom of the page (`NotificationsViewModel` in the main
//! window template): "Mod … was installed. Load mod" after a workshop mod's first install, with
//! previous / next / close / close all.

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Id, Rect, Sense, pos2, vec2};

use super::gfx::{FULL_UV, Gfx, snap};
use super::{icons, theme};
use crate::library::Library;

/// The bar's height; the page gives it up while a notification is shown.
pub const HEIGHT: f32 = 46.0;

#[derive(Default)]
pub struct Notifications {
    /// Identifiers of installed mods, oldest first.
    mods: Vec<String>,
    current: usize,
}

impl Notifications {
    pub fn is_empty(&self) -> bool {
        self.mods.is_empty()
    }

    pub fn push(&mut self, id: String) {
        if !self.mods.contains(&id) {
            self.mods.push(id);
        }
    }

    fn remove_current(&mut self) {
        if self.current < self.mods.len() {
            self.mods.remove(self.current);
        }
        self.current = self.current.min(self.mods.len().saturating_sub(1));
    }

    /// Draws the bar at `rect`; returns the mod to load when Load mod is clicked.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        rect: Rect,
        library: &Library,
    ) -> Option<String> {
        // Mods unsubscribed since are dropped.
        self.mods.retain(|id| library.find(id).is_some());
        let id = self.mods.get(self.current).cloned()?;
        ui.interact(rect, Id::unique("notification-bar"), Sense::click());
        ui.painter().rect_filled(rect, 0.0, theme::SECONDARY_GREEN);
        let black = Color32::BLACK;
        let font = theme::body(theme::BODY_SIZE);
        let y = rect.center().y;

        // Right: "n / m", then previous, next, close and close all (24 × 24, margin 5).
        let mut right = rect.right() - 10.0;
        let count = self.mods.len();
        let buttons: [(&'static str, Option<bool>, bool); 4] = [
            (icons::CLOSE_ALL, None, true),
            (icons::CANCEL_ICON, None, true),
            (icons::ARMA_ARROW, Some(false), self.current + 1 < count),
            (icons::ARMA_ARROW, Some(true), self.current > 0),
        ];
        let mut clicked = None;
        for (i, (icon, flipped, enabled)) in buttons.into_iter().enumerate() {
            // Previous and next only show with more than one notification.
            if flipped.is_some() && count < 2 {
                continue;
            }
            let b = Rect::from_min_size(pos2(right - 5.0 - 24.0, y - 12.0), vec2(24.0, 24.0));
            right -= 34.0;
            let response = ui.interact(b, Id::unique(("notification-button", i)), Sense::click());
            if enabled && response.hovered() {
                ui.painter().rect_filled(b, 0.0, theme::argb(0x33000000));
                ui.painter()
                    .rect_stroke(b, 0.0, (1.0, black), egui::StrokeKind::Inside);
            }
            let tint = if enabled {
                black
            } else {
                black.gamma_multiply(0.2)
            };
            let size = if flipped.is_some() {
                vec2(12.0, 7.0)
            } else {
                vec2(14.0, 14.0)
            };
            if let Some(texture) = gfx.icon(ui.ctx(), icon, size) {
                let uv = if flipped == Some(true) {
                    Rect::from_min_max(pos2(0.0, 1.0), pos2(1.0, 0.0))
                } else {
                    FULL_UV
                };
                let r = snap(ui, Rect::from_center_size(b.center(), size));
                ui.painter().image(texture.id(), r, uv, tint);
            }
            if enabled && response.clicked() {
                clicked = Some(i);
            }
        }
        if count > 1 {
            ui.painter().text(
                pos2(right - 5.0, y),
                Align2::RIGHT_CENTER,
                format!("{} / {count}", self.current + 1),
                font.clone(),
                black,
            );
        }

        // Left (padding 20): "Mod <name> was installed. Load mod".
        let name = library
            .find(&id)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let bold = FontId::new(theme::BODY_SIZE, FontFamily::Name("roboto-bold".into()));
        let mut job = egui::text::LayoutJob::default();
        job.append("Mod ", 0.0, egui::TextFormat::simple(font.clone(), black));
        job.append(&name, 0.0, egui::TextFormat::simple(bold, black));
        job.append(
            " was installed. ",
            0.0,
            egui::TextFormat::simple(font.clone(), black),
        );
        let galley = ui.painter().layout_job(job);
        let text_pos = pos2(rect.left() + 20.0, y - galley.size().y / 2.0);
        let link_left = text_pos.x + galley.size().x;
        ui.painter().galley(text_pos, galley, black);
        let link = ui
            .painter()
            .layout_no_wrap("Load mod".into(), font.clone(), black);
        let link_rect = Rect::from_min_size(pos2(link_left, y - link.size().y / 2.0), link.size());
        let response = ui.interact(link_rect, Id::unique("notification-load"), Sense::click());
        // `Hyperlink` in the template: blue and underlined, navy without the underline on hover.
        let color = if response.hovered() {
            theme::argb(0xFF000080)
        } else {
            theme::argb(0xFF0000FF)
        };
        ui.painter()
            .text(link_rect.min, Align2::LEFT_TOP, "Load mod", font, color);
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        } else {
            ui.painter()
                .hline(link_rect.x_range(), link_rect.bottom() - 1.0, (1.0, color));
        }

        let load = response.clicked();
        if load {
            self.remove_current();
        }
        match clicked {
            Some(0) => {
                self.mods.clear();
                self.current = 0;
            }
            Some(1) => self.remove_current(),
            Some(2) => self.current += 1,
            Some(3) => self.current -= 1,
            _ => {}
        }
        load.then_some(id)
    }
}
