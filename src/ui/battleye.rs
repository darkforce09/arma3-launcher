//! The BattlEye page (`Controls/BattlEyeStatusPage.xaml`): the anti-cheat toggle, help links and
//! the log of BattlEye messages.

use eframe::egui::{self, Align2, Id, Rect, Sense, UiBuilder, pos2, vec2};

use super::gfx::{FULL_UV, Gfx, snap};
use super::widgets::{
    self, checkbox16, flat_button, flat_button_width, section_header, tab_control,
};
use super::{icons, theme};
use crate::battleye::Log;
use crate::config::LauncherConfig;
use crate::params::Settings;

pub fn show(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    page: Rect,
    settings: &mut Settings,
    config: &LauncherConfig,
    log: &Log,
) {
    let (_, content) = tab_control(ui, "battleye-tabs", page, &["BattlEye status"], 0);

    // Padding 20,14,7,14; secondary text, then the checkbox and the links.
    let top = Rect::from_min_size(
        content.min,
        vec2(
            content.width(),
            14.0 + 2.0 * 20.0 + 8.0 + 24.0 + 8.0 + 20.0 + 14.0,
        ),
    );
    ui.painter().rect_filled(top, 0.0, theme::PANEL);
    let font = theme::body(theme::BODY_SIZE);
    let mut y = top.top() + 14.0;
    for line in [
        "BattlEye is an anti-cheat software preventing the use of cheats when playing the multiplayer game.",
        "Enabling the BattlEye might be required by some multiplayer servers.",
    ] {
        ui.painter().text(
            pos2(top.left() + 20.0, y),
            Align2::LEFT_TOP,
            line,
            font.clone(),
            theme::argb(0xFFCCCCCC),
        );
        y += 20.0;
    }
    y += 8.0;
    let enabled = settings.flag("UseBattleEye");
    let check = checkbox16(
        ui,
        gfx,
        Id::unique("battleye-enable"),
        pos2(top.left() + 20.0 + 10.0, y + 10.0),
        enabled,
    );
    let label = ui.painter().layout_no_wrap(
        "Enable BattlEye anti-cheat software".into(),
        font.clone(),
        theme::WHITE,
    );
    let label_rect = Rect::from_min_size(
        pos2(top.left() + 20.0 + 25.0, y + 10.0 - label.size().y / 2.0),
        label.size(),
    );
    ui.painter().galley(label_rect.min, label, theme::WHITE);
    let label_response = ui.interact(
        label_rect,
        Id::unique("battleye-enable-label"),
        Sense::click(),
    );
    if check.clicked() || label_response.clicked() {
        settings.set_flag("UseBattleEye", !enabled);
    }
    y += 24.0 + 8.0;
    let mut x = top.left() + 20.0;
    for (text, key) in [
        ("Read BattlEye FAQ", "BattlEyeFaqUrl"),
        ("Contact BattlEye Support", "BattlEyeSupportUrl"),
    ] {
        if hyperlink(
            ui,
            Id::unique(("battleye-link", key)),
            pos2(x, y),
            text,
            &mut x,
        ) {
            config.open_url(key);
        }
        x += 20.0;
    }

    let header = Rect::from_min_size(
        pos2(content.left(), top.bottom() + 1.0),
        vec2(content.width(), 50.0),
    );
    section_header(
        ui,
        Id::unique("battleye-log"),
        header,
        "BattlEye Log Messages",
        None,
        None,
    );
    let list = Rect::from_min_max(pos2(content.left(), header.bottom() + 1.0), content.max);
    ui.painter().rect_filled(list, 0.0, theme::PANEL);
    if log.messages.is_empty() {
        let center = list.center();
        ui.painter().text(
            center - vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            "There is nothing to report at this time.",
            font.clone(),
            theme::EMPTY_HINT,
        );
        ui.painter().text(
            center + vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            "BattlEye messages will be displayed here",
            font,
            theme::EMPTY_HINT,
        );
        return;
    }
    // One #111111 row per message (min 40, 1 px apart): margin 45, icon 20, time 80
    // right-aligned, the wrapped message, and a copy button that shows on hover.
    let mut list_ui = ui.new_child(UiBuilder::new().max_rect(list));
    list_ui.set_clip_rect(list.intersect(ui.clip_rect()));
    egui::ScrollArea::vertical()
        .id_salt("battleye-messages")
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(&mut list_ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let width = list.width();
            let text_left = 45.0 + 20.0 + 5.0 + 80.0 + 5.0;
            let text_width = (width - text_left - 5.0 - 44.0).max(50.0);
            for (i, message) in log.messages.iter().enumerate() {
                let galley = ui.painter().layout(
                    message.text.clone(),
                    font.clone(),
                    theme::WHITE,
                    text_width,
                );
                let height = (galley.size().y + 8.0).max(40.0);
                let (row, response) =
                    ui.allocate_exact_size(vec2(width, height + 1.0), Sense::hover());
                let row = Rect::from_min_max(pos2(row.left(), row.top() + 1.0), row.max);
                ui.painter().rect_filled(row, 0.0, theme::argb(0xFF111111));
                let y = row.center().y;
                if let Some(texture) = gfx.bitmap(ui.ctx(), message.severity.icon()) {
                    let icon = Rect::from_min_size(
                        pos2(row.left() + 45.0 + 4.0, y - 8.0),
                        vec2(16.0, 16.0),
                    );
                    ui.painter()
                        .image(texture.id(), snap(ui, icon), FULL_UV, egui::Color32::WHITE);
                }
                ui.painter().text(
                    pos2(row.left() + 45.0 + 20.0 + 5.0 + 80.0, y),
                    Align2::RIGHT_CENTER,
                    &message.time,
                    font.clone(),
                    theme::EMPTY_HINT,
                );
                ui.painter().galley(
                    pos2(row.left() + text_left, y - galley.size().y / 2.0),
                    galley,
                    theme::WHITE,
                );
                if response.hovered() {
                    let copy = Rect::from_min_size(
                        pos2(row.right() - 44.0, row.top()),
                        vec2(44.0, row.height()),
                    );
                    let r = widgets::overlay_button(ui, Id::unique(("battleye-copy", i)), copy);
                    gfx.paint_icon(ui, icons::COPY_ICON, copy, vec2(14.0, 16.0), theme::WHITE);
                    if r.clicked() {
                        ui.ctx().copy_text(message.text.clone());
                    }
                }
            }
        });
}

/// The BattlEye EULA window (`BattlEyeEULAWindow`), shown before the first launch with
/// BattlEye. The licence is BattlEye's `EULA.txt` from the game folder. Returns `Some(true)` on
/// Accept, `Some(false)` on Decline.
pub fn eula(ui: &mut egui::Ui, win: Rect, licence: &str) -> Option<bool> {
    ui.interact(win, Id::unique("eula-overlay"), Sense::click_and_drag());
    ui.painter().rect_filled(win, 0.0, theme::STRONG_PANEL);
    let height = (win.height() - 40.0).clamp(300.0, 600.0);
    let rect = Rect::from_center_size(win.center(), vec2(600.0f32.min(win.width() - 40.0), height));
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
        "BattlEye EULA",
        theme::body(theme::HEADER_SIZE),
        theme::WHITE,
    );
    let font = theme::body(theme::BODY_SIZE);
    let secondary = theme::argb(0xFFCCCCCC);
    let italic = |text: &str, width: f32| {
        let mut job = egui::text::LayoutJob::default();
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color: secondary,
                italics: true,
                ..Default::default()
            },
        );
        job.wrap.max_width = width;
        job
    };
    let inner = rect.shrink(10.0);
    let mut y = rect.top() + 36.0 + 10.0;
    let intro = ui.painter().layout_job(italic(
        "BattlEye is anti-cheat software used by Arma 3. It pro-actively protects the game against cheating and hacking and ensures a fair experience for all users.",
        inner.width(),
    ));
    let intro_height = intro.size().y;
    ui.painter().galley(pos2(inner.left(), y), intro, secondary);
    y += intro_height;
    let more = ui.painter().layout_job(italic(
        "More information about the BattlEye anti-cheat software can be found at ",
        inner.width(),
    ));
    let more_size = more.size();
    ui.painter().galley(pos2(inner.left(), y), more, secondary);
    // The link wraps to the next line when it doesn't fit, followed by a full stop.
    const LINK: &str = "http://www.battleye.com";
    let link_width = ui
        .painter()
        .layout_no_wrap(LINK.into(), font.clone(), theme::accent())
        .size()
        .x;
    let mut link_pos = pos2(inner.left() + more_size.x, y);
    if link_pos.x + link_width + 5.0 > inner.right() {
        link_pos = pos2(inner.left(), y + more_size.y);
        y += more_size.y;
    }
    let mut right = 0.0;
    if hyperlink(
        ui,
        Id::unique("eula-battleye-link"),
        link_pos,
        LINK,
        &mut right,
    ) {
        let _ = open::that_detached("http://www.battleye.com/");
    }
    ui.painter().text(
        pos2(right, link_pos.y),
        Align2::LEFT_TOP,
        ".",
        font.clone(),
        secondary,
    );
    y += more_size.y + 10.0;

    // Footer: the consequence of declining, then Decline and Accept at the right.
    let buttons_top = rect.bottom() - 15.0 - 30.0;
    let note_y = buttons_top - 10.0 - 5.0 - 20.0;
    ui.painter().text(
        pos2(rect.right() - 15.0, note_y + 10.0),
        Align2::RIGHT_CENTER,
        "Declining the licence will run Arma 3 without the BattlEye anti-cheat software.",
        font.clone(),
        secondary,
    );
    let text_rect = Rect::from_min_max(
        pos2(rect.left() + 5.0, y),
        pos2(rect.right() - 5.0, note_y - 5.0),
    );
    ui.painter().rect_filled(text_rect, 0.0, theme::BLACK);
    let mut text_ui = ui.new_child(UiBuilder::new().max_rect(text_rect.shrink(10.0)));
    text_ui.set_clip_rect(text_rect.shrink(1.0));
    egui::ScrollArea::vertical()
        .id_salt("eula-text")
        .auto_shrink([false, false])
        .show(&mut text_ui, |ui| {
            let galley = ui.painter().layout(
                licence.to_owned(),
                font.clone(),
                theme::WHITE,
                text_rect.width() - 20.0,
            );
            let (r, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
            ui.painter().galley(r.min, galley, theme::WHITE);
        });
    let mut x = rect.right() - 10.0;
    let mut answer = None;
    for (label, value) in [("Accept", true), ("Decline", false)] {
        let w = flat_button_width(ui, label).max(90.0);
        let b = Rect::from_min_max(pos2(x - w, buttons_top), pos2(x, buttons_top + 30.0));
        x -= w + 5.0;
        if flat_button(ui, Id::unique(("eula", label)), b, label).clicked() {
            answer = Some(value);
        }
    }
    answer
}

/// An implicit-style hyperlink: accent and underlined; red without underline on hover.
pub fn hyperlink(ui: &mut egui::Ui, id: Id, pos: egui::Pos2, text: &str, right: &mut f32) -> bool {
    let galley =
        ui.painter()
            .layout_no_wrap(text.into(), theme::body(theme::BODY_SIZE), theme::accent());
    let rect = Rect::from_min_size(pos, galley.size());
    let response = ui.interact(rect, id, Sense::click());
    let color = if response.hovered() {
        egui::Color32::RED
    } else {
        theme::accent()
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.into(), theme::body(theme::BODY_SIZE), color);
    ui.painter().galley(pos, galley, color);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else {
        ui.painter()
            .hline(rect.x_range(), rect.bottom() - 1.0, (1.0, color));
    }
    *right = rect.right();
    response.clicked()
}
