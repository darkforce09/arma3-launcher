//! Context and dropdown menus (`dropdownbuttonstyles.xaml`) and the modal message box
//! (`ModernMessageBox`).

use eframe::egui::{self, Align2, Color32, Id, Rect, Sense, pos2, vec2};

use super::theme;
use super::widgets::flat_button;

/// ContextMenu base: black with a 1 px `#3F3F3F` border.
pub fn frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(theme::BLACK)
        .stroke(egui::Stroke::new(1.0, theme::argb(0xFF3F3F3F)))
        .shadow(egui::epaint::Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(140),
        })
}

/// `DefaultMenuItemStyle`: MinHeight 28, padding 14,10, `#EEEEEE`; highlight `#55FFFFFF`.
pub fn item(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    let color = if enabled {
        theme::argb(0xFFEEEEEE)
    } else {
        theme::argb(0xFF666666)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), theme::body(theme::BODY_SIZE), color);
    let width = ui.available_width().max(galley.size().x + 28.0);
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, (galley.size().y + 20.0).max(28.0)),
        Sense::click(),
    );
    let hovered = enabled && response.hovered();
    if hovered {
        ui.painter().rect_filled(rect, 0.0, theme::argb(0x55FFFFFF));
    }
    let text_color = if hovered { theme::WHITE } else { color };
    ui.painter().galley(
        pos2(rect.left() + 14.0, rect.center().y - galley.size().y / 2.0),
        galley,
        text_color,
    );
    let clicked = enabled && response.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// A 1 px `#333333` separator.
pub fn separator(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect.shrink2(vec2(1.0, 0.0)), 0.0, theme::argb(0xFF333333));
}

/// A modal message: caption, text and buttons; the first button is the default (accent).
/// What a confirmed message box does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    UnsubscribeAll,
    /// Go on with the PLAY the BattlEye EULA held back.
    ResumePlay,
    /// Start the last game command again without BattlEye (`BEError`).
    PlayWithoutBattlEye,
}

pub struct MessageBox {
    pub caption: String,
    pub body: String,
    pub buttons: Vec<String>,
    /// Run when the first button is clicked.
    pub confirm: Option<Confirm>,
}

impl MessageBox {
    pub fn info(caption: &str, body: impl Into<String>) -> Self {
        Self {
            caption: caption.into(),
            body: body.into(),
            buttons: vec!["OK".into()],
            confirm: None,
        }
    }

    /// Draws the box over a dimmed window; returns the index of the clicked button.
    pub fn show(&self, ui: &mut egui::Ui, win: Rect) -> Option<usize> {
        // Overlay that swallows clicks below.
        ui.interact(
            win,
            Id::unique("message-box-overlay"),
            Sense::click_and_drag(),
        );
        ui.painter().rect_filled(win, 0.0, theme::STRONG_PANEL);

        let width = 460.0f32.clamp(300.0, 600.0);
        let body_font = theme::body(theme::BODY_SIZE);
        let body = ui
            .painter()
            .layout(self.body.clone(), body_font, theme::WHITE, width - 20.0);
        let height = 36.0 + 10.0 + body.size().y + 10.0 + 1.0 + 46.0;
        let rect = Rect::from_center_size(win.center(), vec2(width, height));
        ui.painter().rect_filled(rect, 0.0, theme::argb(0xFF111111));
        ui.painter().rect_stroke(
            rect,
            0.0,
            (1.0, theme::argb(0xFF222222)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            pos2(rect.left() + 10.0, rect.top() + 18.0),
            Align2::LEFT_CENTER,
            &self.caption,
            theme::body(theme::HEADER_SIZE),
            theme::WHITE,
        );
        ui.painter().galley(
            pos2(rect.left() + 10.0, rect.top() + 46.0),
            body,
            theme::WHITE,
        );

        let footer = Rect::from_min_max(
            pos2(rect.left() + 1.0, rect.bottom() - 46.0),
            rect.max - vec2(1.0, 1.0),
        );
        ui.painter().rect_filled(footer, 0.0, theme::BLACK);
        ui.painter().rect_filled(
            Rect::from_min_size(footer.min, vec2(footer.width(), 1.0)),
            0.0,
            theme::argb(0xFF222222),
        );
        let mut right = footer.right() - 5.0;
        let mut clicked = None;
        for (i, label) in self.buttons.iter().enumerate().rev() {
            let w = super::widgets::flat_button_width(ui, label).max(100.0);
            let b = Rect::from_min_max(
                pos2(right - 5.0 - w, footer.top() + 8.0),
                pos2(right - 5.0, footer.bottom() - 8.0),
            );
            right -= w + 10.0;
            if i == 0 {
                ui.painter().rect_filled(b, 0.0, theme::ACCENT);
            }
            let response = if i == 0 {
                let r =
                    super::widgets::overlay_button(ui, Id::unique(("message-box-button", i)), b);
                let g = ui.painter().layout_no_wrap(
                    label.clone(),
                    theme::body(theme::BODY_SIZE),
                    theme::WHITE,
                );
                ui.painter()
                    .galley(b.center() - g.size() / 2.0, g, theme::WHITE);
                r
            } else {
                flat_button(ui, Id::unique(("message-box-button", i)), b, label)
            };
            if response.clicked() {
                clicked = Some(i);
            }
        }
        clicked
    }
}
