//! The borderless window frame (`MainWindowStyle`, `LauncherTitleBarStyle`): shadow, background
//! image, Arma logo, drag area, caption buttons and resize border.

use eframe::egui::{
    self, Color32, CursorIcon, Id, Rect, ResizeDirection, Sense, Vec2, ViewportCommand, pos2, vec2,
};

use super::gfx::{FULL_UV, Gfx};
use super::{icons, theme};

const LOGO: &str = "arma_logo.png";
const CAPTION_BUTTON: Vec2 = vec2(36.0, 32.0);

pub struct WindowState {
    pub maximized: bool,
    pub focused: bool,
}

pub fn window_state(ctx: &egui::Context) -> WindowState {
    ctx.input(|i| WindowState {
        maximized: i.viewport().maximized.unwrap_or(false),
        focused: i.viewport().focused.unwrap_or(true),
    })
}

/// The window's content rectangle inside the shadow margin.
pub fn content_rect(ui: &egui::Ui, state: &WindowState) -> Rect {
    let outer = ui.max_rect();
    if state.maximized {
        outer
    } else {
        outer.shrink(theme::SHADOW_DEPTH)
    }
}

/// Paints the shadow, base colour, background image and logo, and makes the title row drag the
/// window. Call before anything interactive so widgets on top take the pointer.
pub fn paint_frame(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    win: Rect,
    state: &WindowState,
    background: Option<&str>,
) {
    let painter = ui.painter().clone();
    if !state.maximized {
        let color = if state.focused {
            theme::SHADOW_ACTIVE
        } else {
            theme::SHADOW_INACTIVE
        };
        let shadow = egui::epaint::Shadow {
            offset: [0, 0],
            blur: 24,
            spread: 0,
            color,
        };
        painter.add(shadow.as_shape(win, 0));
    }
    painter.rect_filled(win, 0.0, theme::WINDOW_BACKGROUND);

    let clipped = painter.with_clip_rect(win);
    if let Some(texture) = background.and_then(|b| gfx.bitmap(ui.ctx(), b)) {
        // Stretch="Fill" StretchDirection="UpOnly": native size until the window outgrows it.
        let size = texture.size_vec2().max(win.size());
        clipped.image(
            texture.id(),
            Rect::from_min_size(win.min, size),
            FULL_UV,
            Color32::WHITE,
        );
    }

    let title = Rect::from_min_size(win.min, vec2(win.width(), theme::TITLE_HEIGHT));
    let drag = ui.interact(title, Id::unique("title-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!state.maximized));
    }

    if let Some(texture) = gfx.bitmap(ui.ctx(), LOGO) {
        let size = texture.size_vec2() * (72.0 / texture.size_vec2().y);
        let tint = if state.focused {
            Color32::WHITE
        } else {
            Color32::from_white_alpha(128)
        };
        clipped.image(
            texture.id(),
            Rect::from_min_size(win.min + vec2(48.0, 32.0), size),
            FULL_UV,
            tint,
        );
    }
}

/// The caption row at the top right: OPTIONS, then minimise, maximise/restore and exit.
/// Returns the OPTIONS button, for its menu.
pub fn caption_buttons(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    win: Rect,
    state: &WindowState,
) -> egui::Response {
    let ctx = ui.ctx().clone();
    let mut right = win.right();
    let mut button =
        |ui: &mut egui::Ui, gfx: &mut Gfx, id: &str, icon: &'static str, icon_size: Vec2| {
            // Each button has a 1 px left margin.
            let rect =
                Rect::from_min_size(pos2(right - CAPTION_BUTTON.x, win.top()), CAPTION_BUTTON);
            right -= CAPTION_BUTTON.x + 1.0;
            let response = ui.interact(rect, Id::unique(id), Sense::click());
            if response.hovered() {
                ui.painter().rect_filled(rect, 0.0, theme::CAPTION_HOVER);
            }
            let tint = if response.is_pointer_button_down_on() {
                theme::ACCENT
            } else {
                theme::WHITE
            };
            gfx.paint_icon(ui, icon, rect, icon_size, tint);
            response.clicked()
        };
    // Glyphs are stretched uniformly into a 16 × 16 box.
    if button(
        ui,
        gfx,
        "caption-exit",
        icons::EXIT,
        uniform_size(12.905, 9.75),
    ) {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
    let (icon, size) = if state.maximized {
        (icons::RESTORE, uniform_size(38.0, 38.0))
    } else {
        (icons::MAXIMIZE, uniform_size(16.0, 13.0))
    };
    if button(ui, gfx, "caption-maximize", icon, size) {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!state.maximized));
    }
    if button(
        ui,
        gfx,
        "caption-minimize",
        icons::MINIMIZE,
        uniform_size(15.0, 3.0),
    ) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }

    // OPTIONS: 16 px gear + 5 px gap + label, inside a 4 px horizontal margin, 20 px left of the
    // caption buttons.
    let font = theme::body(theme::BODY_SIZE);
    let label_width = ui
        .painter()
        .layout_no_wrap("OPTIONS".into(), font.clone(), theme::WHITE)
        .size()
        .x;
    let width = 4.0 + 16.0 + 5.0 + label_width + 4.0;
    let rect = Rect::from_min_size(
        pos2(right + 1.0 - 20.0 - width, win.top()),
        vec2(width, 32.0),
    );
    let response = ui.interact(rect, Id::unique("caption-options"), Sense::click());
    let color = if response.is_pointer_button_down_on() {
        theme::ACCENT
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::CAPTION_HOVER);
        theme::WHITE
    } else {
        theme::OPTIONS_FOREGROUND
    };
    let gear = Rect::from_min_size(
        pos2(rect.left() + 4.0, rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    gfx.paint_icon(ui, icons::OPTIONS_WHEEL, gear, vec2(16.0, 16.0), color);
    let label = ui.painter().layout_no_wrap("OPTIONS".into(), font, color);
    let text_pos = pos2(gear.right() + 5.0, rect.center().y - label.size().y / 2.0);
    ui.painter().galley(text_pos, label, color);
    response
}

/// The size a `w` × `h` glyph gets when stretched uniformly into a 16 × 16 box.
fn uniform_size(w: f32, h: f32) -> Vec2 {
    let scale = (16.0 / w).min(16.0 / h);
    vec2(w * scale, h * scale)
}

/// The resize border (6/3/3/6 px) and the bottom-right grip. Call last so it sits on top.
pub fn resize_border(ui: &mut egui::Ui, win: Rect, state: &WindowState) {
    if state.maximized {
        return;
    }
    let [l, t, r, b] = theme::RESIZE_BORDER;
    let zones = [
        (
            Rect::from_min_max(win.min, pos2(win.left() + l, win.bottom())),
            ResizeDirection::West,
            CursorIcon::ResizeWest,
        ),
        (
            Rect::from_min_max(pos2(win.right() - r, win.top()), win.max),
            ResizeDirection::East,
            CursorIcon::ResizeEast,
        ),
        (
            Rect::from_min_max(win.min, pos2(win.right(), win.top() + t)),
            ResizeDirection::North,
            CursorIcon::ResizeNorth,
        ),
        (
            Rect::from_min_max(pos2(win.left(), win.bottom() - b), win.max),
            ResizeDirection::South,
            CursorIcon::ResizeSouth,
        ),
        (
            Rect::from_min_size(win.min, vec2(l, t).max(vec2(8.0, 8.0))),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNorthWest,
        ),
        (
            Rect::from_min_max(
                pos2(win.right() - 8.0, win.top()),
                pos2(win.right(), win.top() + 8.0),
            ),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNorthEast,
        ),
        (
            Rect::from_min_max(
                pos2(win.left(), win.bottom() - 8.0),
                pos2(win.left() + 8.0, win.bottom()),
            ),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeSouthWest,
        ),
        // The grip: 11 × 11 at a 2 px margin from the corner.
        (
            Rect::from_min_max(win.max - vec2(13.0, 13.0), win.max),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeSouthEast,
        ),
    ];
    for (i, (rect, direction, cursor)) in zones.into_iter().enumerate() {
        let response = ui.interact(rect, Id::unique(("resize", i)), Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if response.drag_started() {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
    paint_grip(ui, win);
}

/// `ResizeGripStyle`: a staircase of 2 × 2 dots on a 3 px grid, `#66FFFFFF`.
fn paint_grip(ui: &egui::Ui, win: Rect) {
    let origin = win.max - vec2(2.0 + 11.0, 2.0 + 11.0);
    // Path M9,0 L11,0 L11,11 L0,11 L0,9 L3,9 L3,6 L6,6 L6,3 L9,3, filled with 2 × 2 dots every 3 px.
    for row in 0..4 {
        for col in 0..4 {
            let (x, y) = (col as f32 * 3.0, row as f32 * 3.0);
            if x + y >= 9.0 {
                let dot = Rect::from_min_size(origin + vec2(x, y), vec2(2.0, 2.0));
                ui.painter().rect_filled(dot, 0.0, theme::RESIZE_GRIP);
            }
        }
    }
}

/// Known Steam branches: display name and ribbon colour (`KnownBranchData`, `Branch_RibbonColor_*`).
fn branch_style(branch: &str) -> Option<(&'static str, Color32)> {
    let style = match branch.to_ascii_lowercase().as_str() {
        "development" => ("Development", theme::argb(0xFF00BAFB)),
        "legacy" => ("Legacy", theme::argb(0xFFB3B3B3)),
        "rc" => ("Release Candidate", theme::argb(0xFFA8184B)),
        "manwar" => ("Make Arma Not War", theme::argb(0xFF003C52)),
        "staging" => ("Staging", theme::argb(0xFFA40800)),
        "profiling" => ("Profiling", theme::argb(0xFF86CD4D)),
        "apex_sneak_preview" => ("Apex Sneak Preview (Build)", theme::argb(0xFF228A36)),
        _ => return None,
    };
    Some(style)
}

/// The branch ribbon across the top-left corner (`BranchRibbon`): a 150 × 32 band, shifted 32 px
/// up and rotated −45° about the centre of a 117 × 117 corner box, with the branch name fitted
/// into 105 × 25.
pub fn branch_ribbon(ui: &egui::Ui, win: Rect, branch: &str) {
    let Some((name, color)) = branch_style(branch) else {
        return;
    };
    let center = win.min + vec2(58.5, 58.5);
    let (sin, cos) = (-45f32).to_radians().sin_cos();
    // Band-local coordinates: the band's grid spans x 0..150, y 0..32 and sits at y −48..−16
    // relative to the box centre.
    let to_screen = |x: f32, y: f32| {
        let (lx, ly) = (x - 75.0, y - 48.0);
        center + vec2(lx * cos - ly * sin, lx * sin + ly * cos)
    };
    let band =
        [(32.0, 0.0), (118.0, 0.0), (150.0, 32.0), (0.0, 32.0)].map(|(x, y)| to_screen(x, y));
    let painter = ui.painter().with_clip_rect(win);
    painter.add(egui::Shape::convex_polygon(
        band.to_vec(),
        color,
        egui::Stroke::NONE,
    ));

    let probe = painter.layout_no_wrap(name.into(), theme::body(theme::BODY_SIZE), Color32::WHITE);
    let scale = (105.0 / probe.size().x).min(25.0 / probe.size().y);
    let galley = painter.layout_no_wrap(
        name.into(),
        theme::body(theme::BODY_SIZE * scale),
        Color32::WHITE,
    );
    let text_center = to_screen(75.0, 16.0);
    let half = galley.size() / 2.0;
    let pos = text_center - vec2(half.x * cos - half.y * sin, half.x * sin + half.y * cos);
    painter.add(
        egui::epaint::TextShape::new(pos, galley, Color32::WHITE).with_angle((-45f32).to_radians()),
    );
}
