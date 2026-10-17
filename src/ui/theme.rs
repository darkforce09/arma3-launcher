//! Colours, fonts and metrics of the vanilla launcher's default theme (`SharedResources`
//! `colors.xaml` / `brushes.xaml` / `fonts.xaml` and the launcher's `DefaultStyle`).

use eframe::egui::{Color32, FontFamily, FontId};

/// A WPF `#AARRGGBB` colour (WPF blends in sRGB space, as egui does).
pub const fn argb(v: u32) -> Color32 {
    let a = (v >> 24) & 0xFF;
    Color32::from_rgba_premultiplied(
        premultiply(v >> 16, a),
        premultiply(v >> 8, a),
        premultiply(v, a),
        a as u8,
    )
}

const fn premultiply(channel: u32, alpha: u32) -> u8 {
    (((channel & 0xFF) * alpha + 127) / 255) as u8
}

pub const ACCENT: Color32 = argb(0xFFD18F21);
pub const ACCENT_HOVER: Color32 = argb(0xFFEFA933);
pub const ACCENT_PRESSED: Color32 = argb(0xFFF6A112);
/// `Color.SteamBlue.*`: PLAY with optional DLC, the "Optional" DLC tag.
pub const STEAM_BLUE: Color32 = argb(0xFF01BAFB);
pub const STEAM_BLUE_HOVER: Color32 = argb(0xFF4DCFFC);
pub const STEAM_BLUE_PRESSED: Color32 = argb(0xFF4194EC);
pub const WINDOW_BACKGROUND: Color32 = argb(0xFF191919);
pub const WHITE: Color32 = Color32::WHITE;
pub const BLACK: Color32 = Color32::BLACK;
/// `StrongPanelBackgroundColor` / `OverlayBackgroundColor`: toolbars, dashboard panels.
pub const STRONG_PANEL: Color32 = argb(0xAA000000);
/// `PanelBackgroundColor`: list and page panels.
pub const PANEL: Color32 = argb(0x55000000);
/// Mod/DLC list rows: normal, expanded, hovered.
pub const ROW: Color32 = argb(0xCC000000);
pub const ROW_EXPANDED: Color32 = argb(0xFF000000);
pub const ROW_HOVER: Color32 = argb(0xEE222222);
pub const HINT_TEXT: Color32 = argb(0xFF808080);
pub const SECONDARY_LABEL: Color32 = argb(0xFFC0C0C0);
pub const DETAIL_KEY: Color32 = argb(0xFFBBBBBB);
pub const DIM_NAME: Color32 = argb(0xFFA9A9A9);
pub const EMPTY_HINT: Color32 = argb(0xFF696969);
pub const AUTO_INCLUDED: Color32 = argb(0xFF3ADC8A);
pub const SECONDARY_GREEN: Color32 = argb(0xFF00FF97);
pub const ERROR: Color32 = argb(0xFFED1C23);
pub const CAPTION_HOVER: Color32 = argb(0xC0000000);
pub const OPTIONS_FOREGROUND: Color32 = argb(0xCCFFFFFF);
pub const NAV_HOVER_FROM: Color32 = argb(0x33FFFFFF);
pub const NAV_HOVER_TO: Color32 = argb(0x11FFFFFF);
pub const TAB_STRIP: Color32 = argb(0x66000000);
pub const TAB_HOT: Color32 = argb(0x7FFFFFFF);
pub const TAB_SELECTED: Color32 = argb(0xCFFFFFFF);
pub const TAB_FOREGROUND: Color32 = argb(0xFFF5F5F5);
pub const TAB_CONTENT: Color32 = argb(0x33000000);
pub const BUTTON_HOVER_OVERLAY: Color32 = argb(0x33FFFFFF);
pub const BUTTON_PRESSED_OVERLAY: Color32 = argb(0x22FFFFFF);
pub const SPLIT_DIVIDER: Color32 = argb(0x20FFFFFF);
pub const RESIZE_GRIP: Color32 = argb(0x66FFFFFF);
pub const NEWS_EXCERPT: Color32 = argb(0xEEFFFFFF);
pub const NEWS_META: Color32 = argb(0xEEC8C8C8);
pub const UNIT_SLOT_TEXT: Color32 = argb(0x99FFFFFF);
pub const UNIT_CHECK_STROKE: Color32 = argb(0xCCFFFFFF);
pub const FLAT_BORDER: Color32 = argb(0x33FFFFFF);
pub const SHADOW_ACTIVE: Color32 = argb(0xD9000000);
pub const SHADOW_INACTIVE: Color32 = argb(0x2B000000);

/// Font faces read from the launcher's resources (`resources/defaulttheme/fonts/<file>`; spaces are
/// stored URL-encoded).
pub const FONT_FILES: [(&str, &str); 7] = [
    ("roboto-regular", "roboto-regular.ttf"),
    ("roboto-bold", "roboto-bold.ttf"),
    ("roboto-italic", "roboto-italic.ttf"),
    ("roboto-bolditalic", "roboto-bolditalic.ttf"),
    ("roboto-light", "roboto-light.ttf"),
    ("roboto-medium", "roboto-medium.ttf"),
    ("purista-light", "purista%20light.otf"),
];

/// Body text: `PuristaFamily`, which resolves to Roboto Regular.
pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// `HeadingFamily` (`#Purista` at normal weight) and `HeadingLight`: the family has no 400 face and
/// WPF picks Light (checked against the real launcher).
pub fn heading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("purista-light".into()))
}

pub const BODY_SIZE: f32 = 14.0;
pub const HEADER_SIZE: f32 = 17.0;

/// Window geometry at the default 970 × 690 size.
pub const SHADOW_DEPTH: f32 = 16.0;
pub const TITLE_HEIGHT: f32 = 78.0;
pub const NAV_WIDTH: f32 = 236.0;
pub const NAV_TOP: f32 = 130.0;
pub const NAV_ITEM_HEIGHT: f32 = 43.0;
pub const NAV_ACCENT_WIDTH: f32 = 10.0;
pub const STATUS_HEIGHT: f32 = 46.0;
pub const TAB_HEIGHT: f32 = 40.0;
/// Resize border: left, top, right, bottom.
pub const RESIZE_BORDER: [f32; 4] = [6.0, 3.0, 3.0, 6.0];
