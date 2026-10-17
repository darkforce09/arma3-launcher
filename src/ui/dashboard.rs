//! The Dashboard page (`Views/Dashboard/DashboardPage.xaml`): latest report, My Units, social
//! links and a rotating banner.

use eframe::egui::{
    self, Align2, Color32, Id, Rect, Sense, UiBuilder, pos2, text::LayoutJob, vec2,
};

use super::gfx::{FULL_UV, Gfx, snap};
use super::widgets::{self, TAB_CHROME_HEIGHT, tab_control};
use super::{icons, theme};
use crate::config::LauncherConfig;
use crate::news::Post;
use crate::units::{Unit, Units};

const BOTTOM_TABS_HEIGHT: f32 = 170.0;
const SOCIAL_HEIGHT: f32 = 40.0;
const BANNER_HEIGHT: f32 = 89.0;
const REPORT_HEIGHT: f32 = 90.0;
const BI_LOGO: &str = "bi_logo_mono_white_123x46.png";
const UNITS_LOGO: &str = "units_logo.png";

struct Banner {
    background: &'static str,
    overlay_left: &'static str,
    overlay_right: &'static str,
    url: &'static str,
    weight: u32,
    /// Shown only when none of these DLC are owned.
    for_dlc: &'static [u32],
}

/// `BannersViewModel.BuildDefaultBannersList`.
const BANNERS: [Banner; 17] = [
    Banner {
        background: "1_apex/background_left.jpg",
        overlay_left: "1_apex/overlay_left.png",
        overlay_right: "1_apex/overlay_right.png",
        url: "https://arma3.com/apex",
        weight: 100,
        for_dlc: &[395180],
    },
    Banner {
        background: "2_workshop/background_left.jpg",
        overlay_left: "2_workshop/overlay_left.png",
        overlay_right: "2_workshop/overlay_right.png",
        url: "http://steamcommunity.com/workshop/browse/?appid=107410&requiredtags[]=Mod",
        weight: 50,
        for_dlc: &[],
    },
    Banner {
        background: "3_jets/background_left.jpg",
        overlay_left: "3_jets/overlay_left.png",
        overlay_right: "3_jets/overlay_right.png",
        url: "https://arma3.com/dlc/jets",
        weight: 100,
        for_dlc: &[601670, 612480],
    },
    Banner {
        background: "6_low/background_left.jpg",
        overlay_left: "6_low/overlay_left.png",
        overlay_right: "6_low/overlay_right.png",
        url: "https://arma3.com/dlc/lawsofwar",
        weight: 100,
        for_dlc: &[571710, 612480],
    },
    Banner {
        background: "7_heli/background_left.jpg",
        overlay_left: "7_heli/overlay_left.png",
        overlay_right: "7_heli/overlay_right.png",
        url: "https://arma3.com/dlc/helicopters",
        weight: 100,
        for_dlc: &[304380, 304400],
    },
    Banner {
        background: "8_mark/background_left.jpg",
        overlay_left: "8_mark/overlay_left.png",
        overlay_right: "8_mark/overlay_right.png",
        url: "https://arma3.com/dlc/marksmen",
        weight: 100,
        for_dlc: &[332350, 304400],
    },
    Banner {
        background: "11_tacops/background_left.png",
        overlay_left: "11_tacops/overlay_left.png",
        overlay_right: "11_tacops/overlay_right.png",
        url: "https://arma3.com/dlc/tacops",
        weight: 100,
        for_dlc: &[744950, 612480],
    },
    Banner {
        background: "12_tanks/background_left.png",
        overlay_left: "12_tanks/overlay_left.png",
        overlay_right: "12_tanks/overlay_right.png",
        url: "https://arma3.com/dlc/tanks",
        weight: 100,
        for_dlc: &[304380, 612480],
    },
    Banner {
        background: "13_creator/background_left.png",
        overlay_left: "13_creator/overlay_left.png",
        overlay_right: "13_creator/overlay_right.png",
        url: "https://arma3.com/dlc/creator",
        weight: 100,
        for_dlc: &[],
    },
    Banner {
        background: "14_contact/background_left.png",
        overlay_left: "14_contact/overlay_left.png",
        overlay_right: "14_contact/overlay_right.png",
        url: "https://arma3.com/dlc/contact",
        weight: 100,
        for_dlc: &[1021790],
    },
    Banner {
        background: "16_gm/background_left.png",
        overlay_left: "16_gm/overlay_left.png",
        overlay_right: "16_gm/overlay_right.png",
        url: "https://store.steampowered.com/app/1042220",
        weight: 100,
        for_dlc: &[1042220],
    },
    Banner {
        background: "16_gm/background_left.png",
        overlay_left: "17_vn/overlay_left.png",
        overlay_right: "17_vn/overlay_right.png",
        url: "https://store.steampowered.com/app/1227700",
        weight: 100,
        for_dlc: &[1227700],
    },
    Banner {
        background: "16_gm/background_left.png",
        overlay_left: "18_csla/overlay_left.png",
        overlay_right: "18_csla/overlay_right.png",
        url: "https://store.steampowered.com/app/1294440",
        weight: 100,
        for_dlc: &[1294440],
    },
    Banner {
        background: "16_gm/background_left.png",
        overlay_left: "19_ws/overlay_left.png",
        overlay_right: "19_ws/overlay_right.png",
        url: "https://store.steampowered.com/app/1681170",
        weight: 100,
        for_dlc: &[1681170],
    },
    Banner {
        background: "16_gm/background_left.png",
        overlay_left: "20_spe/overlay_left.png",
        overlay_right: "20_spe/overlay_right.png",
        url: "https://store.steampowered.com/app/1175380",
        weight: 100,
        for_dlc: &[1175380],
    },
    Banner {
        background: "16_gm/background_left.png",
        overlay_left: "21_rf/overlay_left.png",
        overlay_right: "21_rf/overlay_right.png",
        url: "https://store.steampowered.com/app/2647760",
        weight: 100,
        for_dlc: &[2647760],
    },
    Banner {
        background: "16_gm/background_left.png",
        overlay_left: "22_ef/overlay_left.png",
        overlay_right: "22_ef/overlay_right.png",
        url: "https://store.steampowered.com/app/2647830",
        weight: 100,
        for_dlc: &[2647830],
    },
];

pub struct Dashboard {
    banner: &'static Banner,
}

impl Dashboard {
    /// Picks the banner by weighted random among those for DLC the user doesn't own.
    pub fn new(owned_app_ids: &[u32], seed: u64) -> Self {
        let eligible: Vec<&'static Banner> = BANNERS
            .iter()
            .filter(|b| !b.for_dlc.iter().any(|id| owned_app_ids.contains(id)))
            .collect();
        let total: u64 = eligible.iter().map(|b| u64::from(b.weight)).sum();
        let mut pick = seed % total.max(1);
        let banner = eligible
            .iter()
            .find(|b| {
                let hit = pick < u64::from(b.weight);
                pick = pick.saturating_sub(u64::from(b.weight));
                hit
            })
            .copied()
            .unwrap_or(&BANNERS[1]);
        Self { banner }
    }

    pub fn preload_images(&self) -> Vec<String> {
        let banners = [
            self.banner.background,
            self.banner.overlay_left,
            self.banner.overlay_right,
        ];
        let mut names: Vec<String> = banners.iter().map(|b| format!("banners/{b}")).collect();
        names.extend([BI_LOGO, UNITS_LOGO, "newsicon_spotrep36.png"].map(String::from));
        names
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        config: &LauncherConfig,
        page: Rect,
        latest: Option<&Post>,
        units: Option<&Units>,
    ) {
        let bottom = Rect::from_min_max(
            pos2(page.left(), page.bottom() - BOTTOM_TABS_HEIGHT),
            page.max,
        );
        self.stay_in_touch(ui, gfx, config, bottom);

        let top = Rect::from_min_max(page.min, pos2(page.right(), bottom.top()));
        let mut scroll_ui = ui.new_child(UiBuilder::new().max_rect(top));
        scroll_ui.set_clip_rect(top);
        egui::ScrollArea::vertical()
            .id_salt("dashboard-scroll")
            .show(&mut scroll_ui, |ui| {
                let width = ui.available_width();
                let origin = ui.cursor().min;
                let reports =
                    Rect::from_min_size(origin, vec2(width, TAB_CHROME_HEIGHT + REPORT_HEIGHT));
                latest_reports(ui, gfx, config, reports, latest);
                let units_top = reports.bottom() + 5.0;
                let units = my_units(ui, gfx, config, pos2(origin.x, units_top), width, units);
                ui.allocate_rect(
                    Rect::from_min_max(origin, pos2(origin.x + width, units.bottom() + 5.0)),
                    Sense::hover(),
                );
            });
    }

    fn stay_in_touch(&self, ui: &mut egui::Ui, gfx: &mut Gfx, config: &LauncherConfig, rect: Rect) {
        let clip = ui.clip_rect().intersect(rect);
        let mut ui = ui.new_child(UiBuilder::new().max_rect(rect));
        ui.set_clip_rect(clip);
        let (_, content) = tab_control(&mut ui, "stay-in-touch", rect, &["Stay in touch"], 0);

        let social = Rect::from_min_size(content.min, vec2(content.width(), SOCIAL_HEIGHT));
        ui.painter().rect_filled(social, 0.0, theme::STRONG_PANEL);
        let links = [
            (icons::TWITTER_LOGO, "Arma3TwitterUrl"),
            (icons::FACEBOOK_LOGO, "Arma3FacebookUrl"),
            (icons::YOUTUBE_LOGO, "Arma3YouTubeUrl"),
        ];
        let mut x = social.left();
        for (i, (icon, key)) in links.into_iter().enumerate() {
            let button = Rect::from_min_size(pos2(x, social.top()), vec2(80.0, SOCIAL_HEIGHT));
            x += 80.0;
            if flat_overlay_button(&mut ui, Id::unique(("social", i)), button) {
                config.open_url(key);
            }
            // Padding 10, Stretch="Uniform".
            gfx.paint_icon(
                &ui,
                icon,
                button.shrink(10.0),
                button.shrink(10.0).size(),
                theme::WHITE,
            );
        }
        let bi = Rect::from_min_size(pos2(x, social.top()), vec2(110.0, SOCIAL_HEIGHT));
        if flat_overlay_button(&mut ui, Id::unique("social-bi"), bi) {
            config.open_url("BiStudioUrl");
        }
        if let Some(texture) = gfx.bitmap(ui.ctx(), BI_LOGO) {
            // Padding 5, Uniform, DownOnly.
            let room = bi.shrink(5.0).size();
            let scale = (room.x / texture.size_vec2().x)
                .min(room.y / texture.size_vec2().y)
                .min(1.0);
            let image = Rect::from_center_size(bi.center(), texture.size_vec2() * scale);
            ui.painter()
                .image(texture.id(), snap(&ui, image), FULL_UV, Color32::WHITE);
        }

        let banner = Rect::from_min_size(
            pos2(content.left(), social.bottom() + 1.0),
            vec2(content.width(), BANNER_HEIGHT),
        );
        ui.painter().rect_filled(banner, 0.0, theme::STRONG_PANEL);
        let response = ui.interact(banner, Id::unique("banner"), Sense::click());
        let banner_painter = ui.painter().with_clip_rect(banner.intersect(clip));
        let layers = [
            (self.banner.background, egui::Align::LEFT),
            (self.banner.overlay_left, egui::Align::LEFT),
            (self.banner.overlay_right, egui::Align::RIGHT),
        ];
        for (name, align) in layers {
            if let Some(texture) = gfx.bitmap(ui.ctx(), &format!("banners/{name}")) {
                let size = texture.size_vec2();
                let x = if align == egui::Align::LEFT {
                    banner.left()
                } else {
                    banner.right() - size.x
                };
                banner_painter.image(
                    texture.id(),
                    Rect::from_min_size(pos2(x, banner.top()), size),
                    FULL_UV,
                    Color32::WHITE,
                );
            }
        }
        if response.hovered() {
            // BannerButtonStyle: a white overlay at 20 % (10 % pressed).
            let alpha = if response.is_pointer_button_down_on() {
                0.1
            } else {
                0.2
            };
            banner_painter.rect_filled(banner, 0.0, Color32::WHITE.gamma_multiply(alpha));
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() {
            let _ = open::that_detached(self.banner.url);
        }
    }
}

/// The "Latest reports" tab with the newest post (`SolitareNewsItemTemplate`).
fn latest_reports(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    config: &LauncherConfig,
    rect: Rect,
    post: Option<&Post>,
) {
    let (_, content) = tab_control(ui, "latest-reports", rect, &["Latest reports"], 0);
    let card = Rect::from_min_size(content.min, vec2(content.width(), REPORT_HEIGHT));
    ui.painter().rect_filled(card, 0.0, theme::STRONG_PANEL);

    // Open-in-browser button: 34 × 30, top right of the tab control, 5 px down.
    let open_button = Rect::from_min_size(
        pos2(rect.right() - 34.0, rect.top() + 5.0),
        vec2(34.0, 30.0),
    );
    if flat_overlay_button(ui, Id::unique("reports-open"), open_button) {
        config.open_url("DevArma3Url");
    }
    gfx.paint_icon(
        ui,
        icons::OPEN_EXTERNAL,
        open_button,
        vec2(26.0, 16.0),
        theme::WHITE,
    );

    let Some(post) = post else { return };
    let response = ui.interact(card, Id::unique("latest-report"), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(card, 0.0, theme::BUTTON_HOVER_OVERLAY);
    }
    if response.clicked() {
        let _ = open::that_detached(&post.url);
    }
    // Padding 5,10,15,10; 36 × 36 icon with 10 px margins either side.
    let inner = Rect::from_min_max(card.min + vec2(5.0, 10.0), card.max - vec2(15.0, 10.0));
    let icon = Rect::from_min_size(
        pos2(inner.left() + 10.0, inner.center().y - 18.0),
        vec2(36.0, 36.0),
    );
    if let Some(texture) = gfx.bitmap(ui.ctx(), post.icon()) {
        // The PNG is an opacity mask over the accent colour: tinting its alpha does the same.
        ui.painter()
            .image(texture.id(), snap(ui, icon), FULL_UV, theme::ACCENT);
    }

    let read =
        ui.painter()
            .layout_no_wrap("Read".into(), theme::body(theme::BODY_SIZE), theme::WHITE);
    let read_rect = Rect::from_min_size(
        pos2(
            inner.right() - read.size().x - 20.0,
            inner.center().y - (read.size().y + 6.0) / 2.0,
        ),
        read.size() + vec2(20.0, 6.0),
    );
    ui.painter()
        .galley(read_rect.center() - read.size() / 2.0, read, theme::WHITE);

    let text_left = icon.right() + 10.0;
    let text_width = read_rect.left() - text_left;
    let mut y = inner.top() + 2.0;
    let title = ui.painter().layout_no_wrap(
        post.title.clone(),
        theme::heading(theme::HEADER_SIZE),
        theme::ACCENT,
    );
    let title_height = title.size().y;
    ui.painter()
        .galley(pos2(text_left, y), title, theme::ACCENT);
    y += title_height + 2.0;
    let excerpt = single_line(ui, &post.excerpt_text(), text_width, theme::NEWS_EXCERPT);
    let excerpt_height = excerpt.size().y;
    ui.painter()
        .galley(pos2(text_left, y), excerpt, theme::NEWS_EXCERPT);
    y += excerpt_height + 2.0;
    let meta_color = theme::NEWS_META.gamma_multiply(0.7);
    let meta = ui.painter().layout_no_wrap(
        post.reported_by(),
        theme::body(theme::BODY_SIZE),
        meta_color,
    );
    ui.painter().galley(pos2(text_left, y), meta, meta_color);
}

/// The "My Units" tab: up to three unit tiles (empty slots after them), the two launch options,
/// and the loading / not connected / failed overlays. Returns its rect.
fn my_units(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    config: &LauncherConfig,
    origin: egui::Pos2,
    width: f32,
    units: Option<&Units>,
) -> Rect {
    const TILE_HEIGHT: f32 = 160.0;
    const OPTION_HEIGHT: f32 = 40.0;
    let height = TAB_CHROME_HEIGHT + 1.0 + TILE_HEIGHT + 2.0 * OPTION_HEIGHT + 1.0;
    let rect = Rect::from_min_size(origin, vec2(width, height));
    let (_, content) = tab_control(ui, "my-units", rect, &["My Units"], 0);
    ui.painter().rect_filled(content, 0.0, theme::STRONG_PANEL);
    let inner = content.shrink(1.0);
    let found: &[Unit] = match units {
        Some(Units::Found(list)) => list,
        _ => &[],
    };

    let tile_width = inner.width() / 3.0;
    for i in 0..3 {
        let tile = Rect::from_min_size(
            pos2(inner.left() + i as f32 * tile_width, inner.top()),
            vec2(tile_width, TILE_HEIGHT),
        )
        .shrink(1.0);
        let response = ui.interact(tile, Id::unique(("unit-tile", i)), Sense::click());
        if response.hovered() {
            ui.painter()
                .rect_filled(tile, 0.0, theme::BUTTON_HOVER_OVERLAY);
        }
        if let Some(unit) = found.get(i) {
            // UnitTileTemplate: logo (max 140 × 140, margin 20) and the name, Purista 18.
            let mut y = tile.top() + 20.0;
            if let Some(texture) = unit
                .logo_url
                .as_deref()
                .and_then(|url| gfx.remote(ui.ctx(), url))
            {
                let room = vec2(140.0, (TILE_HEIGHT - 20.0 - 20.0 - 30.0).min(140.0));
                let scale = (room.x / texture.size_vec2().x)
                    .min(room.y / texture.size_vec2().y)
                    .min(1.0);
                let size = texture.size_vec2() * scale;
                ui.painter().image(
                    texture.id(),
                    Rect::from_min_size(pos2(tile.center().x - size.x / 2.0, y), size),
                    FULL_UV,
                    Color32::WHITE,
                );
                y += size.y + 8.0;
            }
            let name = ui.painter().layout(
                unit.name.clone(),
                theme::heading(18.0),
                theme::WHITE,
                tile.width() - 20.0,
            );
            ui.painter().galley(
                pos2(
                    tile.center().x - name.size().x / 2.0,
                    y.min(tile.bottom() - name.size().y - 6.0),
                ),
                name,
                theme::WHITE,
            );
            if response.clicked() {
                let _ = open::that_detached(&unit.profile_url);
            }
            continue;
        }
        // EmptyUnitTileTemplate: faded Units logo (max 121 × 60, margin 40,15), caption, button.
        let mut y = tile.top() + 15.0;
        if let Some(texture) = gfx.bitmap(ui.ctx(), UNITS_LOGO) {
            let scale = (121.0 / texture.size_vec2().x)
                .min(60.0 / texture.size_vec2().y)
                .min(1.0);
            let size = texture.size_vec2() * scale;
            let logo = Rect::from_min_size(pos2(tile.center().x - size.x / 2.0, y), size);
            ui.painter().image(
                texture.id(),
                logo,
                FULL_UV,
                Color32::WHITE.gamma_multiply(0.1),
            );
            y += size.y + 15.0;
        }
        let caption_color = theme::UNIT_SLOT_TEXT.gamma_multiply(0.5);
        let caption = ui.painter().layout_no_wrap(
            "SLOT AVAILABLE".into(),
            theme::heading(18.0),
            caption_color,
        );
        let caption_height = caption.size().y;
        ui.painter().galley(
            pos2(tile.center().x - caption.size().x / 2.0, y),
            caption,
            caption_color,
        );
        y += caption_height + 8.0;
        let label = ui.painter().layout_no_wrap(
            "FIND A UNIT".into(),
            theme::body(theme::BODY_SIZE),
            theme::WHITE,
        );
        let button = Rect::from_center_size(
            pos2(tile.center().x, y + 14.0),
            label.size() + vec2(20.0, 10.0),
        );
        if flat_button(ui, Id::unique(("find-unit", i)), button) {
            config.open_url("UnitsUrl");
        }
        ui.painter()
            .galley(button.center() - label.size() / 2.0, label, theme::WHITE);
    }

    let options = [
        "Launch using the same unit as last time",
        "Launch without a unit selected",
    ];
    for (i, text) in options.into_iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(
                inner.left(),
                inner.top() + TILE_HEIGHT + i as f32 * OPTION_HEIGHT,
            ),
            vec2(inner.width(), OPTION_HEIGHT),
        )
        .shrink(5.0);
        // A 24 × 24 UnitCheckBox centred in a 50 px cell, then the label.
        let check =
            Rect::from_center_size(pos2(row.left() + 25.0, row.center().y), vec2(24.0, 24.0));
        ui.painter().rect_filled(check, 0.0, theme::BLACK);
        ui.painter().rect_stroke(
            check.shrink(0.5),
            0.0,
            (1.0, theme::UNIT_CHECK_STROKE),
            egui::StrokeKind::Inside,
        );
        let label =
            ui.painter()
                .layout_no_wrap(text.into(), theme::body(theme::BODY_SIZE), theme::WHITE);
        ui.painter().galley(
            pos2(
                row.left() + 50.0 + 5.0,
                row.center().y - label.size().y / 2.0,
            ),
            label,
            theme::WHITE,
        );
    }

    // Status overlays over the whole tab content.
    let font = theme::body(theme::BODY_SIZE);
    match units {
        None => {
            ui.painter().rect_filled(content, 0.0, theme::STRONG_PANEL);
            ui.painter().text(
                content.center(),
                Align2::CENTER_CENTER,
                "Acquiring data...",
                theme::heading(18.0),
                theme::WHITE,
            );
        }
        Some(Units::Failed) => {
            ui.painter().rect_filled(content, 0.0, theme::STRONG_PANEL);
            let c = content.center();
            ui.painter().text(
                c - vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                "Arma 3 Units data acquisition failed",
                theme::body(theme::HEADER_SIZE),
                theme::ERROR,
            );
            ui.painter().text(
                c + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                "Information about your unit(s) will not be available.",
                font,
                theme::ERROR.gamma_multiply(0.8),
            );
        }
        Some(Units::NotConnected) => {
            ui.painter().rect_filled(content, 0.0, theme::STRONG_PANEL);
            let area = content.shrink(40.0);
            let mut y = area.top();
            ui.painter().text(
                pos2(area.center().x, y),
                Align2::CENTER_TOP,
                "Where are my Arma 3 Units?",
                theme::body(theme::HEADER_SIZE),
                theme::argb(0xFFEAA32D),
            );
            y += 28.0;
            let description = ui.painter().layout(
                "Your Bohemia Account and Steam account have to be connected in order for your units to be displayed here and in-game.".into(),
                font.clone(),
                theme::NEWS_META.gamma_multiply(0.7),
                area.width(),
            );
            let dh = description.size().y;
            ui.painter().galley(
                pos2(area.center().x - description.size().x / 2.0, y),
                description,
                theme::WHITE,
            );
            y += dh + 10.0;
            let label = "Connect my Bohemia Account to my Steam account";
            let w = widgets::flat_button_width(ui, label);
            let button = Rect::from_min_size(pos2(area.center().x - w / 2.0, y), vec2(w, 30.0));
            if widgets::flat_button(ui, Id::unique("units-connect"), button, label).clicked() {
                config.open_url("ConnectBiAccountUrl");
            }
        }
        Some(Units::Found(_)) => {}
    }
    rect
}

/// One line of body text, cut with an ellipsis to `width`.
fn single_line(
    ui: &egui::Ui,
    text: &str,
    width: f32,
    color: Color32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple(text.to_owned(), theme::body(theme::BODY_SIZE), color, width);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

fn flat_overlay_button(ui: &mut egui::Ui, id: Id, rect: Rect) -> bool {
    widgets::overlay_button(ui, id, rect).clicked()
}

fn flat_button(ui: &mut egui::Ui, id: Id, rect: Rect) -> bool {
    widgets::flat_button(ui, id, rect, "").clicked()
}
