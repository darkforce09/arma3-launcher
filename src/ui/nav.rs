//! The left navigation (`MainNavButtonStyle` and the hover/active sliders in `MainWindow`).

use eframe::egui::{self, Id, Rect, Sense, epaint::Mesh, pos2, vec2};

use super::anim::{NAV_SPLINE, Tween};
use super::{Page, theme};

const BUTTON_WIDTH: f32 = 200.0;
const SLIDE_SECONDS: f64 = 0.2;

pub struct Nav {
    active: Tween,
    hover: Tween,
    hover_visible: bool,
}

impl Nav {
    pub fn new(page: Page) -> Self {
        let y = slot_y(page);
        Self {
            active: Tween::new(y, SLIDE_SECONDS, NAV_SPLINE),
            hover: Tween::new(y, SLIDE_SECONDS, NAV_SPLINE),
            hover_visible: false,
        }
    }

    /// Draws the navigation and returns the page the user picked.
    /// `news_unread` shows as a badge on News.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        win: Rect,
        page: Page,
        news_unread: usize,
    ) -> Option<Page> {
        let ctx = ui.ctx().clone();
        let mut picked = None;
        let mut hovered_slot = None;
        let mut pressed_slot = None;
        for (i, &item) in Page::ALL.iter().enumerate() {
            let rect = Rect::from_min_size(
                win.min + vec2(0.0, slot_y(item)),
                vec2(BUTTON_WIDTH, theme::NAV_ITEM_HEIGHT),
            );
            let response = ui.interact(rect, Id::unique(("nav", i)), Sense::click());
            if response.hovered() {
                hovered_slot = Some(slot_y(item));
            }
            if response.is_pointer_button_down_on() {
                pressed_slot = Some(item);
            }
            if response.clicked() {
                picked = Some(item);
            }
        }

        // Hover slider: 236 × 43 gradient; jumps when it was hidden, slides otherwise.
        match hovered_slot {
            Some(y) if !self.hover_visible => {
                self.hover.jump(y);
                self.hover_visible = true;
            }
            Some(y) => self.hover.set(&ctx, y),
            None => self.hover_visible = false,
        }
        if self.hover_visible {
            let rect = Rect::from_min_size(
                win.min + vec2(0.0, self.hover.value(&ctx)),
                vec2(theme::NAV_WIDTH, theme::NAV_ITEM_HEIGHT),
            );
            paint_horizontal_gradient(ui, rect, theme::NAV_HOVER_FROM, theme::NAV_HOVER_TO);
        }

        // Permanent slider: 10 × 43 accent bar on the active page (none for Options).
        let shown = picked.unwrap_or(page);
        if Page::ALL.contains(&shown) {
            self.active.set(&ctx, slot_y(shown));
            let bar = Rect::from_min_size(
                win.min + vec2(0.0, self.active.value(&ctx)),
                vec2(theme::NAV_ACCENT_WIDTH, theme::NAV_ITEM_HEIGHT),
            );
            ui.painter().rect_filled(bar, 0.0, theme::accent());
        }

        let font = theme::heading(21.0);
        for &item in &Page::ALL {
            let color = if pressed_slot == Some(item) {
                theme::accent_pressed()
            } else if item == page {
                theme::accent()
            } else {
                theme::WHITE
            };
            // 10 px left border + 15 px padding; text vertically centred.
            let galley =
                ui.painter()
                    .layout_no_wrap(item.title().to_uppercase(), font.clone(), color);
            let top = win.top() + slot_y(item) + (theme::NAV_ITEM_HEIGHT - galley.size().y) / 2.0;
            ui.painter()
                .galley(pos2(win.left() + 25.0, top), galley, color);
            // Badge: 24 × 24, corner radius 4, accent with black text, docked right in the button.
            if item == Page::News && news_unread > 0 {
                let badge = Rect::from_center_size(
                    pos2(
                        win.left() + BUTTON_WIDTH - 12.0 - 4.0,
                        win.top() + slot_y(item) + theme::NAV_ITEM_HEIGHT / 2.0,
                    ),
                    vec2(24.0, 24.0),
                );
                ui.painter().rect_filled(badge, 4.0, theme::accent());
                let count = ui.painter().layout_no_wrap(
                    news_unread.to_string(),
                    theme::body(theme::BODY_SIZE),
                    theme::BLACK,
                );
                ui.painter()
                    .galley(badge.center() - count.size() / 2.0, count, theme::BLACK);
            }
        }
        picked
    }
}

fn slot_y(page: Page) -> f32 {
    let index = Page::ALL.iter().position(|&p| p == page).unwrap_or(0);
    theme::NAV_TOP + index as f32 * theme::NAV_ITEM_HEIGHT
}

fn paint_horizontal_gradient(ui: &egui::Ui, rect: Rect, left: egui::Color32, right: egui::Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
}
