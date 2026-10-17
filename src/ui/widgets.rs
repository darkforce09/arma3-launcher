//! Controls shared by several pages: the `LauncherTabControl` header strip, the status bar and
//! the PLAY split button.

use eframe::egui::{self, Color32, Id, Rect, Sense, pos2, vec2};

use super::gfx::Gfx;
use super::{icons, theme};

const TAB_MIN_WIDTH: f32 = 120.0;
const TAB_PADDING: f32 = 20.0;

/// A `LauncherTabControl` at `rect`: draws the header strip and the content background and
/// returns the selected tab (changed when a header is clicked) and the content rectangle.
pub fn tab_control(
    ui: &mut egui::Ui,
    id: &str,
    rect: Rect,
    headers: &[&str],
    selected: usize,
) -> (usize, Rect) {
    let font = theme::heading(theme::HEADER_SIZE);
    let mut selected_now = selected;
    let mut x = rect.left();
    for (i, header) in headers.iter().enumerate() {
        let text_width = ui
            .painter()
            .layout_no_wrap(header.to_uppercase(), font.clone(), Color32::WHITE)
            .size()
            .x;
        // Padding 20 on both sides plus the 1 px transparent right border.
        let width = (text_width + 2.0 * TAB_PADDING).max(TAB_MIN_WIDTH) + 1.0;
        let tab = Rect::from_min_size(pos2(x, rect.top()), vec2(width - 1.0, theme::TAB_HEIGHT));
        x += width;
        let response = ui.interact(tab, Id::unique((id, "tab", i)), Sense::click());
        if response.clicked() {
            selected_now = i;
        }
        let (background, foreground) = if i == selected_now {
            (theme::TAB_SELECTED, theme::BLACK)
        } else if response.hovered() {
            (theme::TAB_HOT, theme::BLACK)
        } else {
            (theme::TAB_STRIP, theme::TAB_FOREGROUND)
        };
        ui.painter().rect_filled(tab, 0.0, background);
        let galley = ui
            .painter()
            .layout_no_wrap(header.to_uppercase(), font.clone(), foreground);
        let text_top = tab.top() + (tab.height() - galley.size().y) / 2.0;
        ui.painter()
            .galley(pos2(tab.left() + TAB_PADDING, text_top), galley, foreground);
    }
    let content = Rect::from_min_max(pos2(rect.left(), rect.top() + theme::TAB_HEIGHT), rect.max);
    ui.painter().rect_filled(content, 0.0, theme::TAB_CONTENT);
    let line = Rect::from_min_size(content.min, vec2(content.width(), 1.0));
    ui.painter().rect_filled(line, 0.0, theme::TAB_SELECTED);
    (
        selected_now,
        Rect::from_min_max(content.min + vec2(0.0, 2.0), content.max),
    )
}

/// The height a tab control adds above its content.
pub const TAB_CHROME_HEIGHT: f32 = theme::TAB_HEIGHT + 2.0;

/// The status bar along the bottom right (`MainWindowUniversalProgressBarControl`).
pub fn status_bar(ui: &egui::Ui, win: Rect) {
    let rect = Rect::from_min_max(
        pos2(
            win.left() + theme::NAV_WIDTH,
            win.bottom() - theme::STATUS_HEIGHT,
        ),
        win.max,
    );
    // `#AA000000` over a black border: solid black.
    ui.painter().rect_filled(rect, 0.0, theme::BLACK);
}

pub struct PlayButtons {
    pub play: bool,
    /// The dropdown part, for its menu.
    pub menu: egui::Response,
}

/// What PLAY shows (`PlayButtonStyleType`), by what is loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayStyle {
    Play,
    Dlc,
    Mod,
    DlcMod,
}

impl PlayStyle {
    /// `UpdateButtonStyle`.
    pub fn new(dlc: bool, mods: bool) -> Self {
        match (dlc, mods) {
            (true, true) => PlayStyle::DlcMod,
            (true, false) => PlayStyle::Dlc,
            (false, true) => PlayStyle::Mod,
            (false, false) => PlayStyle::Play,
        }
    }

    fn subtitle(self) -> Option<&'static str> {
        match self {
            PlayStyle::Play => None,
            PlayStyle::Dlc => Some("OPTIONAL DLC ON"),
            PlayStyle::Mod => Some("WITH MODS"),
            PlayStyle::DlcMod => Some("MODS & DLCS ON"),
        }
    }

    fn background(self, response: &egui::Response) -> Color32 {
        match self {
            PlayStyle::Play | PlayStyle::Mod => accent_state(response),
            PlayStyle::Dlc | PlayStyle::DlcMod => blue_state(response),
        }
    }
}

/// The PLAY split button: 168 × 60 main part plus a 32 px dropdown, overhanging the status bar
/// by 23 px (`PlayButton`, `PlayButtonStyleDlc`, `PlaySplitButtonStyleDlc`). Steam blue with
/// optional DLC loaded, with a subtitle naming what is loaded, and an accent notched stripe on
/// the left with both DLC and mods.
pub fn play_button(ui: &mut egui::Ui, gfx: &mut Gfx, win: Rect, style: PlayStyle) -> PlayButtons {
    // Bottom margin −23 below the row above the 46 px status bar.
    let bottom = win.bottom() - theme::STATUS_HEIGHT + 23.0;
    let rect = Rect::from_min_max(
        pos2(win.left() + 20.0, bottom - 60.0),
        pos2(win.left() + 220.0, bottom),
    );
    let main = Rect::from_min_size(rect.min, vec2(168.0, 60.0));
    let split = Rect::from_min_max(pos2(main.right(), rect.top()), rect.max);

    let main_response = ui.interact(main, Id::unique("play"), Sense::click());
    ui.painter()
        .rect_filled(main, 0.0, style.background(&main_response));
    let title = ui
        .painter()
        .layout_no_wrap("PLAY".into(), theme::heading(28.0), theme::WHITE);
    let subtitle = style.subtitle().map(|s| {
        ui.painter()
            .layout_no_wrap(s.into(), theme::heading(14.0), theme::WHITE)
    });
    let height = title.size().y + subtitle.as_ref().map_or(0.0, |s| s.size().y);
    let mut y = main.center().y - height / 2.0;
    for galley in std::iter::once(title).chain(subtitle) {
        let size = galley.size();
        ui.painter().galley(
            pos2(main.center().x - size.x / 2.0, y),
            galley,
            theme::WHITE,
        );
        y += size.y;
    }
    if style == PlayStyle::DlcMod {
        // Path `M0,0 L10,0 L10,25 L15,30 L10,35 L10,60 L0,60`.
        let color = accent_state(&main_response);
        let o = main.min;
        ui.painter().rect_filled(
            Rect::from_min_size(o, vec2(10.0, main.height())),
            0.0,
            color,
        );
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                o + vec2(10.0, 25.0),
                o + vec2(15.0, 30.0),
                o + vec2(10.0, 35.0),
            ],
            color,
            egui::Stroke::NONE,
        ));
    }

    let split_response = ui.interact(split, Id::unique("play-menu"), Sense::click());
    ui.painter()
        .rect_filled(split, 0.0, style.background(&split_response));
    ui.painter().rect_filled(
        Rect::from_min_size(split.min, vec2(1.0, split.height())),
        0.0,
        theme::SPLIT_DIVIDER,
    );
    gfx.paint_icon(ui, icons::ARMA_ARROW, split, vec2(14.0, 8.0), theme::WHITE);
    PlayButtons {
        play: main_response.clicked(),
        menu: split_response,
    }
}

/// PLAY CONTACT (`PlayContactButtonStyleDlc`), shown to owners of the Contact DLC: 200 × 60, 60 px
/// above the status bar row.
pub fn play_contact_button(ui: &mut egui::Ui, win: Rect) -> bool {
    let bottom = win.bottom() - theme::STATUS_HEIGHT - 60.0;
    let rect = Rect::from_min_max(
        pos2(win.left() + 20.0, bottom - 60.0),
        pos2(win.left() + 220.0, bottom),
    );
    let response = ui.interact(rect, Id::unique("play-contact"), Sense::click());
    let background = if response.is_pointer_button_down_on() {
        theme::argb(0xFF3976A8)
    } else if response.hovered() {
        theme::argb(0xFF30638E)
    } else {
        theme::BLACK
    };
    ui.painter().rect_filled(rect, 0.0, background);
    let galley =
        ui.painter()
            .layout_no_wrap("PLAY CONTACT".into(), theme::heading(21.0), theme::WHITE);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, theme::WHITE);
    response.clicked()
}

fn accent_state(response: &egui::Response) -> Color32 {
    if response.is_pointer_button_down_on() {
        theme::ACCENT_PRESSED
    } else if response.hovered() {
        theme::ACCENT_HOVER
    } else {
        theme::ACCENT
    }
}

fn blue_state(response: &egui::Response) -> Color32 {
    if response.is_pointer_button_down_on() {
        theme::STEAM_BLUE_PRESSED
    } else if response.hovered() {
        theme::STEAM_BLUE_HOVER
    } else {
        theme::STEAM_BLUE
    }
}

/// A transparent button with the default hover/pressed overlays (`BorderlessFlatButtonStyle`).
pub fn overlay_button(ui: &mut egui::Ui, id: Id, rect: Rect) -> egui::Response {
    let response = ui.interact(rect, id, Sense::click());
    if response.is_pointer_button_down_on() {
        ui.painter()
            .rect_filled(rect, 0.0, theme::BUTTON_PRESSED_OVERLAY);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, theme::BUTTON_HOVER_OVERLAY);
    }
    response
}

/// `FlatButtonStyle`: black, 1 px `#33FFFFFF` border, hover/pressed overlays, centred label.
pub fn flat_button(ui: &mut egui::Ui, id: Id, rect: Rect, label: &str) -> egui::Response {
    ui.painter().rect_filled(rect, 0.0, theme::BLACK);
    ui.painter().rect_stroke(
        rect,
        0.0,
        (1.0, theme::FLAT_BORDER),
        egui::StrokeKind::Inside,
    );
    let response = overlay_button(ui, id, rect);
    if !label.is_empty() {
        let galley =
            ui.painter()
                .layout_no_wrap(label.into(), theme::body(theme::BODY_SIZE), theme::WHITE);
        ui.painter()
            .galley(rect.center() - galley.size() / 2.0, galley, theme::WHITE);
    }
    response
}

/// The width a flat button needs for `label` with the default 10 px padding.
pub fn flat_button_width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(label.into(), theme::body(theme::BODY_SIZE), theme::WHITE)
        .size()
        .x
        + 20.0
}

/// `CheckBox16Style`: a 16 × 16 box, white border, black when checked with a white tick.
pub fn checkbox16(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    id: Id,
    center: egui::Pos2,
    checked: bool,
) -> egui::Response {
    let rect = Rect::from_center_size(center, vec2(16.0, 16.0));
    let response = ui.interact(rect.expand(2.0), id, Sense::click());
    let (fill, border) = if checked {
        (theme::BLACK, theme::WHITE)
    } else if response.hovered() {
        (theme::BUTTON_HOVER_OVERLAY, theme::WHITE)
    } else {
        (theme::argb(0x66000000), theme::WHITE.gamma_multiply(0.7))
    };
    ui.painter().rect_filled(rect, 0.0, fill);
    ui.painter()
        .rect_stroke(rect, 0.0, (1.0, border), egui::StrokeKind::Inside);
    if checked {
        gfx.paint_icon(ui, icons::CHECK, rect, vec2(12.0, 12.0), theme::WHITE);
    }
    response
}

/// A section header (`SubHeaderBorderStyle` + `SubHeaderStyle`): 50 px, accent Purista 17 at
/// 20 px padding, an optional count badge and an optional flat button on the right.
pub fn section_header(
    ui: &mut egui::Ui,
    id: Id,
    rect: Rect,
    title: &str,
    count: Option<usize>,
    button: Option<&str>,
) -> bool {
    ui.painter().rect_filled(rect, 0.0, theme::PANEL);
    let galley = ui.painter().layout_no_wrap(
        title.into(),
        theme::heading(theme::HEADER_SIZE),
        theme::ACCENT,
    );
    let text_pos = pos2(rect.left() + 20.0, rect.center().y - galley.size().y / 2.0);
    let text_right = text_pos.x + galley.size().x;
    ui.painter().galley(text_pos, galley, theme::ACCENT);
    if let Some(count) = count {
        count_badge(ui, pos2(text_right + 10.0, rect.center().y), count);
    }
    let Some(label) = button else { return false };
    let width = flat_button_width(ui, label);
    let button_rect = Rect::from_min_max(
        pos2(rect.right() - 20.0 - width, rect.top() + 6.0),
        pos2(rect.right() - 20.0, rect.bottom() - 6.0),
    );
    flat_button(ui, id, button_rect, label).clicked()
}

/// The accent count badge: 20 × 20 (wider for long numbers), corner radius 3, black Purista 12.
pub fn count_badge(ui: &egui::Ui, left_center: egui::Pos2, count: usize) {
    let galley = ui
        .painter()
        .layout_no_wrap(count.to_string(), theme::heading(12.0), theme::BLACK);
    let size = vec2((galley.size().x + 6.0).max(20.0), 20.0);
    let rect = Rect::from_min_size(pos2(left_center.x, left_center.y - 10.0), size);
    ui.painter().rect_filled(rect, 3.0, theme::ACCENT);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, theme::BLACK);
}

/// `SimpleToolbar`: a 40 px strip.
pub fn toolbar(ui: &egui::Ui, rect: Rect) {
    ui.painter().rect_filled(rect, 0.0, theme::STRONG_PANEL);
}

pub enum ToolbarIcon<'a> {
    Image(&'a str),
    /// A 16 × 16 vector icon.
    Glyph(&'static str),
    /// The "More" button: three dots and a dropdown arrow.
    MoreDots,
}

/// A toolbar button (`SubscribeButtonStyle`): optional 28 × 28 image, label; hover border and
/// fill. Returns the response; `x` advances past the button.
pub fn toolbar_button(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    id: Id,
    x: &mut f32,
    bar: Rect,
    icon: ToolbarIcon,
    label: &str,
) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.into(), theme::body(theme::BODY_SIZE), theme::WHITE);
    let (icon_width, dropdown) = match icon {
        ToolbarIcon::Image(_) => (28.0 + 3.0, false),
        ToolbarIcon::Glyph(_) => (16.0 + 5.0, false),
        ToolbarIcon::MoreDots => (11.0 + 5.0, true),
    };
    let width = 5.0 + icon_width + galley.size().x + 5.0 + if dropdown { 16.0 } else { 0.0 };
    let rect = Rect::from_min_size(
        pos2(*x + 1.0, bar.top() + 1.0),
        vec2(width, bar.height() - 2.0),
    );
    *x += width + 2.0;
    let response = ui.interact(rect, id, Sense::click());
    if response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 0.0, theme::argb(0x11FFFFFF));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::argb(0x22FFFFFF));
        ui.painter().rect_stroke(
            rect,
            0.0,
            (1.0, theme::argb(0x66FFFFFF)),
            egui::StrokeKind::Inside,
        );
    }
    let left = rect.left() + 5.0;
    match icon {
        ToolbarIcon::Image(image) => {
            if let Some(texture) = gfx.bitmap(ui.ctx(), image) {
                let r = Rect::from_min_size(pos2(left, rect.center().y - 14.0), vec2(28.0, 28.0));
                ui.painter().image(
                    texture.id(),
                    super::gfx::snap(ui, r),
                    super::gfx::FULL_UV,
                    Color32::WHITE,
                );
            }
        }
        ToolbarIcon::Glyph(glyph) => {
            let r = Rect::from_min_size(pos2(left, rect.center().y - 8.0), vec2(16.0, 16.0));
            gfx.paint_icon(ui, glyph, r, vec2(16.0, 16.0), theme::WHITE);
        }
        // Three 3 × 3 white squares, 4 px apart.
        ToolbarIcon::MoreDots => {
            for i in 0..3 {
                let dot = Rect::from_min_size(
                    pos2(left + i as f32 * 4.0, rect.center().y - 1.5),
                    vec2(3.0, 3.0),
                );
                ui.painter().rect_filled(dot, 0.0, theme::WHITE);
            }
        }
    }
    let text_left = left + icon_width;
    let text_width = galley.size().x;
    ui.painter().galley(
        pos2(text_left, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::WHITE,
    );
    if dropdown {
        chevron(
            ui,
            gfx,
            pos2(text_left + text_width + 3.0 + 5.0, rect.center().y),
            false,
            theme::argb(0xFFF4F4F5),
        );
    }
    response
}

/// `FilterBox`: black field with a 1 px border (accent while filtering), funnel icon, watermark
/// and a clear button. Returns true when the text changed.
pub fn filter_box(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    id: Id,
    rect: Rect,
    text: &mut String,
    watermark: &str,
) -> bool {
    let active = !text.is_empty();
    let border = if active {
        theme::ACCENT
    } else {
        theme::argb(0xFF333333)
    };
    ui.painter().rect_filled(rect, 0.0, theme::BLACK);
    ui.painter()
        .rect_stroke(rect, 0.0, (1.0, border), egui::StrokeKind::Inside);
    let funnel = Rect::from_min_size(
        pos2(rect.left() + 10.0, rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    gfx.paint_icon(ui, icons::FILTER_GEOMETRY, funnel, vec2(16.0, 16.0), border);
    let clear_width = if active { 26.0 } else { 0.0 };
    let field = Rect::from_min_max(
        pos2(funnel.right() + 10.0, rect.top() + 1.0),
        pos2(rect.right() - 4.0 - clear_width, rect.bottom() - 1.0),
    );
    let edit = egui::TextEdit::singleline(text)
        .id(id)
        .frame(egui::Frame::NONE)
        .font(theme::body(theme::BODY_SIZE))
        .text_color(theme::WHITE)
        .hint_text(egui::RichText::new(watermark).color(theme::argb(0xFF808080)))
        .align(egui::Align2::LEFT_CENTER);
    let changed = ui.put(field, edit).changed();
    if active {
        let clear = Rect::from_min_size(
            pos2(rect.right() - 4.0 - clear_width, rect.top()),
            vec2(clear_width, rect.height()),
        );
        let response = ui.interact(clear, id.with("clear"), Sense::click());
        gfx.paint_icon(ui, icons::CROSS, clear, vec2(12.0, 12.0), theme::ACCENT);
        if response.clicked() {
            text.clear();
            return true;
        }
    }
    changed
}

/// An expander chevron (`ArmaArrowGeometry`, 12 × 7), flipped when expanded.
pub fn chevron(ui: &egui::Ui, gfx: &mut Gfx, center: egui::Pos2, expanded: bool, color: Color32) {
    if let Some(texture) = gfx.icon(ui.ctx(), icons::ARMA_ARROW, vec2(12.0, 7.0)) {
        let rect = super::gfx::snap(ui, Rect::from_center_size(center, vec2(12.0, 7.0)));
        let uv = if expanded {
            Rect::from_min_max(pos2(0.0, 1.0), pos2(1.0, 0.0))
        } else {
            super::gfx::FULL_UV
        };
        ui.painter().image(texture.id(), rect, uv, color);
    }
}

/// A message in the status bar (`MainWindowProgressBarControl`): 17 px text at a 25 px margin,
/// made of coloured runs.
pub fn status_message(ui: &egui::Ui, win: Rect, runs: &[(&str, Color32)]) {
    let bar = Rect::from_min_max(
        pos2(
            win.left() + theme::NAV_WIDTH,
            win.bottom() - theme::STATUS_HEIGHT,
        ),
        win.max,
    );
    let mut job = egui::text::LayoutJob::default();
    for (text, color) in runs {
        job.append(
            text,
            0.0,
            egui::TextFormat::simple(theme::body(17.0), *color),
        );
    }
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(bar.left() + 25.0, bar.center().y - galley.size().y / 2.0),
        galley,
        theme::WHITE,
    );
}

/// The status bar's progress fill (`DefaultContentProgressBarStyle`): `#99999999` from the left.
pub fn status_progress(ui: &egui::Ui, win: Rect, fraction: f32) {
    let bar = Rect::from_min_max(
        pos2(
            win.left() + theme::NAV_WIDTH,
            win.bottom() - theme::STATUS_HEIGHT,
        ),
        win.max,
    );
    let fill = Rect::from_min_size(bar.min, vec2(bar.width() * fraction, bar.height()));
    ui.painter().rect_filled(fill, 0.0, theme::argb(0x99999999));
}

/// The drop pad shown while files are dragged over the window: a dimmed panel with a dashed
/// 2 px white border, 20 px in from the window edge.
pub fn drop_pad(ui: &egui::Ui, win: Rect) {
    let rect = win.shrink(20.0);
    ui.painter().rect_filled(rect, 0.0, theme::STRONG_PANEL);
    let dashes = egui::Shape::dashed_line(
        &[
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
            rect.left_top(),
        ],
        egui::Stroke::new(2.0, theme::WHITE),
        4.0,
        4.0,
    );
    ui.painter().extend(dashes);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "Drop the mods, the presets, or missions here.",
        theme::body(17.0),
        theme::WHITE,
    );
}

/// `UpdatedTextBoxStyle2`: black, 1 px `#33FFFFFF` border (accent while focused), `#EEFFFFFF`
/// text. Returns true when the text changed.
pub fn text_box(ui: &mut egui::Ui, id: Id, rect: Rect, text: &mut String) -> bool {
    ui.painter().rect_filled(rect, 0.0, theme::BLACK);
    let focused = ui.memory(|m| m.has_focus(id));
    let border = if focused {
        theme::ACCENT
    } else {
        theme::FLAT_BORDER
    };
    let edit = egui::TextEdit::singleline(text)
        .id(id)
        .frame(egui::Frame::NONE)
        .font(theme::body(theme::BODY_SIZE))
        .text_color(theme::argb(0xEEFFFFFF))
        .align(egui::Align2::LEFT_CENTER);
    let changed = ui.put(rect.shrink2(vec2(4.0, 1.0)), edit).changed();
    ui.painter()
        .rect_stroke(rect, 0.0, (1.0, border), egui::StrokeKind::Inside);
    changed
}

/// The number editor (Xceed `IntegerUpDown` as styled): `[20 −][1][value][1][20 +]`, black
/// spinner buttons that turn accent on hover. Returns true when the value changed.
#[allow(clippy::too_many_arguments)]
pub fn number_box(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    id: Id,
    rect: Rect,
    value: &mut i64,
    min: i64,
    max: i64,
    step: i64,
) -> bool {
    let mut changed = false;
    ui.painter().rect_filled(rect, 0.0, theme::BLACK);
    ui.painter().rect_stroke(
        rect,
        0.0,
        (1.0, theme::FLAT_BORDER),
        egui::StrokeKind::Inside,
    );
    let down = Rect::from_min_size(rect.min, vec2(20.0, rect.height())).shrink(1.0);
    let up = Rect::from_min_size(
        pos2(rect.right() - 20.0, rect.top()),
        vec2(20.0, rect.height()),
    )
    .shrink(1.0);
    for (button, delta, angle) in [
        (down, -step, std::f32::consts::FRAC_PI_2),
        (up, step, -std::f32::consts::FRAC_PI_2),
    ] {
        let response = ui.interact(button, id.with(delta.signum()), Sense::click());
        ui.painter().rect_filled(
            button,
            0.0,
            if response.hovered() {
                theme::ACCENT
            } else {
                theme::BLACK
            },
        );
        if let Some(texture) = gfx.icon(ui.ctx(), icons::ARMA_ARROW, vec2(12.0, 7.0)) {
            egui::Image::new(&texture)
                .rotate(angle, vec2(0.5, 0.5))
                .tint(theme::WHITE)
                .paint_at(ui, Rect::from_center_size(button.center(), vec2(12.0, 7.0)));
        }
        if response.clicked() {
            *value = (*value + delta).clamp(min, max);
            changed = true;
        }
    }
    let field = Rect::from_min_max(
        pos2(down.right() + 2.0, rect.top() + 1.0),
        pos2(up.left() - 2.0, rect.bottom() - 1.0),
    );
    let mut text = value.to_string();
    let edit = egui::TextEdit::singleline(&mut text)
        .id(id)
        .frame(egui::Frame::NONE)
        .font(theme::body(theme::BODY_SIZE))
        .text_color(theme::WHITE)
        .align(egui::Align2::CENTER_CENTER);
    if ui.put(field, edit).changed() {
        let digits: String = text
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        if let Ok(v) = digits.parse::<i64>() {
            *value = v.clamp(min, max);
            changed = true;
        }
    }
    changed
}

/// `OnePartComboBoxStyle`: a black box with the value and an arrow that turns accent on hover,
/// opening a black list (`DefaultComboBoxItemStyle`, highlight `#444444`). Returns the picked
/// option's value.
pub fn combo(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    id: Id,
    rect: Rect,
    current: &str,
    options: &[(String, String)],
) -> Option<String> {
    let response = ui.interact(rect, id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let hot = response.hovered() || open;
    ui.painter()
        .rect_filled(rect, 0.0, if hot { theme::ACCENT } else { theme::BLACK });
    ui.painter().rect_stroke(
        rect,
        0.0,
        (1.0, theme::FLAT_BORDER),
        egui::StrokeKind::Inside,
    );
    let label = options
        .iter()
        .find(|(v, _)| v == current)
        .map_or(current, |(_, l)| l.as_str());
    let text_rect = Rect::from_min_max(rect.min + vec2(5.0, 0.0), rect.max - vec2(22.0, 0.0));
    let mut job = egui::text::LayoutJob::simple(
        label.to_owned(),
        theme::body(theme::BODY_SIZE),
        theme::WHITE,
        text_rect.width(),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        pos2(text_rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        theme::WHITE,
    );
    chevron(
        ui,
        gfx,
        pos2(rect.right() - 11.0, rect.center().y),
        open,
        theme::WHITE,
    );
    let mut picked = None;
    let frame = egui::Frame::NONE
        .fill(theme::BLACK)
        .stroke(egui::Stroke::new(1.0, theme::FLAT_BORDER));
    egui::Popup::menu(&response)
        .frame(frame)
        .width(rect.width())
        .gap(0.0)
        .show(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    for (value, label) in options {
                        let galley = ui.painter().layout_no_wrap(
                            label.clone(),
                            theme::body(theme::BODY_SIZE),
                            theme::WHITE,
                        );
                        let (item, item_response) = ui.allocate_exact_size(
                            vec2(rect.width() - 2.0, galley.size().y + 10.0),
                            Sense::click(),
                        );
                        if item_response.hovered() || value == current {
                            ui.painter().rect_filled(item, 0.0, theme::argb(0xFF444444));
                        }
                        ui.painter().galley(
                            pos2(item.left() + 10.0, item.top() + 5.0),
                            galley,
                            theme::WHITE,
                        );
                        if item_response.clicked() {
                            picked = Some(value.clone());
                            ui.close();
                        }
                    }
                });
        });
    picked
}

/// The 21 px "..." button beside file and folder fields: three 2 × 2 squares.
pub fn browse_button(ui: &mut egui::Ui, id: Id, rect: Rect) -> bool {
    let response = ui.interact(rect, id, Sense::click());
    ui.painter().rect_filled(
        rect,
        0.0,
        if response.hovered() {
            theme::ACCENT
        } else {
            theme::BLACK
        },
    );
    ui.painter().rect_stroke(
        rect,
        0.0,
        (1.0, theme::FLAT_BORDER),
        egui::StrokeKind::Inside,
    );
    for i in 0..3 {
        let dot = Rect::from_min_size(
            pos2(
                rect.center().x - 5.0 + i as f32 * 4.0,
                rect.center().y + 3.0,
            ),
            vec2(2.0, 2.0),
        );
        ui.painter().rect_filled(dot, 0.0, theme::WHITE);
    }
    response.clicked()
}

/// `CommandFlatButton` (`DefaultCommandFlatButton` when `default`): an arrow, an upper-case
/// Purista 17 title and a wrapped description at 0.7, padding 10, on a `#111111` → `#050505`
/// gradient (accent for the default) with a `#333333` border; `#808080` on hover. Laid out at
/// `pos` across `width`; returns the response and the height used.
#[allow(clippy::too_many_arguments)]
pub fn command_button(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    id: Id,
    pos: egui::Pos2,
    width: f32,
    title: &str,
    description: &str,
    default: bool,
) -> (egui::Response, f32) {
    let text_left = 10.0 + 30.0 + 10.0;
    let text_width = (width - text_left - 10.0).max(50.0);
    let title_galley = ui.painter().layout(
        title.to_uppercase(),
        theme::heading(17.0),
        Color32::WHITE,
        text_width,
    );
    let description_galley = ui.painter().layout(
        description.to_owned(),
        theme::body(theme::BODY_SIZE),
        Color32::WHITE.gamma_multiply(0.7),
        text_width,
    );
    let height = 10.0 + title_galley.size().y + description_galley.size().y + 10.0;
    let rect = Rect::from_min_size(pos, vec2(width, height.max(30.0)));
    let response = ui.interact(rect, id, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::argb(0xFF808080));
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    } else if default {
        ui.painter().rect_filled(rect, 0.0, theme::ACCENT);
    } else {
        let mut mesh = egui::Mesh::default();
        let (top, bottom) = (theme::argb(0xFF111111), theme::argb(0xFF050505));
        mesh.colored_vertex(rect.left_top(), top);
        mesh.colored_vertex(rect.right_top(), top);
        mesh.colored_vertex(rect.right_bottom(), bottom);
        mesh.colored_vertex(rect.left_bottom(), bottom);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().add(mesh);
    }
    ui.painter().rect_stroke(
        rect,
        0.0,
        (1.0, theme::argb(0xFF333333)),
        egui::StrokeKind::Inside,
    );
    let arrow = Rect::from_min_size(
        pos2(
            rect.left() + 10.0,
            rect.top() + 10.0 + title_galley.size().y / 2.0 - 7.0,
        ),
        vec2(25.0, 14.0),
    );
    gfx.paint_icon(
        ui,
        icons::COMMAND_ARROW,
        arrow,
        vec2(20.0, 14.0),
        Color32::WHITE,
    );
    let title_height = title_galley.size().y;
    ui.painter().galley(
        pos2(rect.left() + text_left, rect.top() + 10.0),
        title_galley,
        Color32::WHITE,
    );
    ui.painter().galley(
        pos2(rect.left() + text_left, rect.top() + 10.0 + title_height),
        description_galley,
        Color32::WHITE,
    );
    (response, rect.height())
}
