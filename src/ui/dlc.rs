//! The DLC page (`Views/Dlc/DlcManagement*.xaml`): loaded and available optional DLC, the
//! default DLC, and per-DLC details with Get/Install buttons.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align2, Color32, Id, Rect, Sense, UiBuilder, pos2, vec2};

use super::gfx::{FULL_UV, Gfx, snap};
use super::theme;
use super::widgets::{
    ToolbarIcon, checkbox16, chevron, filter_box, section_header, tab_control, toolbar,
    toolbar_button,
};
use crate::config::LauncherConfig;
use crate::dlc::{DLCS, Dlc, DlcState};
use crate::library::Library;
use crate::steam::Steam;

const HEADER_HEIGHT: f32 = 50.0;
const ROW_HEIGHT: f32 = 40.0;
const DETAIL_HEIGHT: f32 = 214.0;
const NOT_OWNED: Color32 = theme::argb(0xFFFF0000);
const PURPLE: Color32 = theme::argb(0xFF800080);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Loaded,
    Available,
    Default,
}

pub struct DlcPage {
    filter: String,
    hide_unowned: bool,
    expanded: HashSet<u32>,
    /// Installed size in MB, measured off the UI thread when a row is first expanded.
    sizes: HashMap<u32, Option<f64>>,
    sizes_tx: Sender<(u32, f64)>,
    sizes_rx: Receiver<(u32, f64)>,
}

pub enum DlcAction {
    Install(u32),
}

impl DlcPage {
    pub fn new() -> Self {
        let (sizes_tx, sizes_rx) = channel();
        Self {
            filter: String::new(),
            hide_unowned: false,
            expanded: HashSet::new(),
            sizes: HashMap::new(),
            sizes_tx,
            sizes_rx,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        lib: &mut Library,
        states: &HashMap<u32, DlcState>,
        config: &LauncherConfig,
        steam: Option<&Steam>,
        game_dir: &Path,
    ) -> Option<DlcAction> {
        while let Ok((id, mb)) = self.sizes_rx.try_recv() {
            self.sizes.insert(id, Some(mb));
        }
        let (_, content) = tab_control(ui, "dlc-tabs", page, &["DLC"], 0);
        let bar = Rect::from_min_size(content.min, vec2(content.width(), 40.0));
        toolbar(ui, bar);
        let mut x = bar.left();
        if toolbar_button(
            ui,
            gfx,
            Id::unique("dlc-store"),
            &mut x,
            bar,
            ToolbarIcon::Image("toolbar_button_addsteammod.png"),
            "Steam Store",
        )
        .clicked()
        {
            config.open_url("Arma3SteamStoreDlcUrl");
        }
        let (eye, label) = if self.hide_unowned {
            ("eye_red.png", "Show not owned")
        } else {
            ("eye_white.png", "Hide not owned")
        };
        if toolbar_button(
            ui,
            gfx,
            Id::unique("dlc-unowned"),
            &mut x,
            bar,
            ToolbarIcon::Image(eye),
            label,
        )
        .clicked()
        {
            self.hide_unowned = !self.hide_unowned;
        }
        let width = (bar.right() - x - 6.0).clamp(200.0, 300.0);
        let filter_rect = Rect::from_min_size(
            pos2(bar.right() - 3.0 - width, bar.top() + 4.0),
            vec2(width, 32.0),
        );
        filter_box(
            ui,
            gfx,
            Id::unique("dlc-filter"),
            filter_rect,
            &mut self.filter,
            "Search DLC (Ctrl+F)",
        );

        let filter = self.filter.to_lowercase();
        let state = |d: &Dlc| states.get(&d.app_id).copied().unwrap_or_default();
        let visible = |d: &Dlc| {
            (filter.is_empty() || d.name.to_lowercase().contains(&filter))
                && (!self.hide_unowned || state(d).owned || d.mandatory)
        };
        let loaded: Vec<&'static Dlc> = lib
            .dlcs
            .iter()
            .filter_map(|id| crate::dlc::find(*id))
            .filter(|d| !d.mandatory && visible(d))
            .collect();
        let available: Vec<&'static Dlc> = DLCS
            .iter()
            .filter(|d| !d.mandatory && !lib.dlcs.contains(&d.app_id) && visible(d))
            .collect();
        let default: Vec<&'static Dlc> =
            DLCS.iter().filter(|d| d.mandatory && visible(d)).collect();

        let body = Rect::from_min_max(pos2(content.left(), bar.bottom() + 1.0), content.max);
        ui.painter().rect_filled(body, 0.0, theme::PANEL);
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        let mut action = None;
        let mut change: Option<(u32, bool)> = None;
        let mut bulk: Option<bool> = None;
        egui::ScrollArea::vertical().id_salt("dlc-list").auto_shrink([false, false]).show(&mut body_ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let width = body.width();
            let sections = [
                (Section::Loaded, "Loaded optional DLC", &loaded, Some("Unload all")),
                (Section::Available, "Available optional DLC", &available, Some("Load all")),
                (Section::Default, "Default DLC", &default, None),
            ];
            for (section, title, list, button) in sections {
                let (header, _) = ui.allocate_exact_size(vec2(width, HEADER_HEIGHT), Sense::hover());
                if section_header(ui, Id::unique(("dlc-section", title)), header, title, Some(list.len()), button) {
                    bulk = Some(section == Section::Available);
                }
                if section == Section::Loaded {
                    let (hint, _) = ui.allocate_exact_size(vec2(width, 30.0), Sense::hover());
                    ui.painter().text(
                        hint.center(),
                        Align2::CENTER_CENTER,
                        "(Compatibility is not guaranteed when loading multiple optional DLC at the same time.)",
                        theme::body(theme::BODY_SIZE),
                        theme::EMPTY_HINT,
                    );
                }
                for dlc in list.iter() {
                    ui.add_space(1.0);
                    let expanded = self.expanded.contains(&dlc.app_id);
                    let height = ROW_HEIGHT + if expanded { DETAIL_HEIGHT } else { 0.0 };
                    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                    match self.row(ui, gfx, rect, dlc, state(dlc), section == Section::Loaded, steam.is_some(), game_dir) {
                        Some(RowEvent::Toggle(on)) => change = Some((dlc.app_id, on)),
                        Some(RowEvent::Install) => action = Some(DlcAction::Install(dlc.app_id)),
                        Some(RowEvent::Get) => {
                            let url = config.get("Arma3SteamStoreOneDlcUrl").unwrap_or("https://store.steampowered.com/app/{0}");
                            let _ = open::that_detached(url.replace("{0}", &dlc.app_id.to_string()));
                        }
                        None => {}
                    }
                }
            }
        });
        if let Some((id, on)) = change {
            if on {
                if !lib.dlcs.contains(&id) {
                    lib.dlcs.push(id);
                }
            } else {
                lib.dlcs.retain(|d| *d != id);
            }
        }
        match bulk {
            Some(true) => {
                for d in &available {
                    if state(d).installed && !lib.dlcs.contains(&d.app_id) {
                        lib.dlcs.push(d.app_id);
                    }
                }
            }
            Some(false) => lib
                .dlcs
                .retain(|id| !loaded.iter().any(|d| d.app_id == *id)),
            None => {}
        }
        action
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        rect: Rect,
        dlc: &Dlc,
        state: DlcState,
        loaded: bool,
        steam: bool,
        game_dir: &Path,
    ) -> Option<RowEvent> {
        let mut event = None;
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_HEIGHT));
        let expanded = self.expanded.contains(&dlc.app_id);
        let response = ui.interact(header, Id::unique(("dlc-row", dlc.app_id)), Sense::click());
        let background = if response.hovered() {
            theme::ROW_HOVER
        } else if expanded {
            theme::ROW_EXPANDED
        } else {
            theme::ROW
        };
        ui.painter().rect_filled(rect, 0.0, background);
        if response.clicked() && !self.expanded.remove(&dlc.app_id) {
            self.expanded.insert(dlc.app_id);
        }
        let y = header.center().y;
        // Mandatory DLC have no checkbox; optional ones load only when installed.
        if !dlc.mandatory {
            let check = checkbox16(
                ui,
                gfx,
                Id::unique(("dlc-check", dlc.app_id)),
                pos2(header.left() + 25.0 + 12.0, y),
                loaded,
            );
            if check.clicked() && (state.installed || loaded) {
                event = Some(RowEvent::Toggle(!loaded));
            }
        }
        let icon_left = header.left() + 25.0 + 24.0 + 5.0;
        if let Some(texture) = gfx.bitmap(ui.ctx(), dlc.icon) {
            let r = Rect::from_min_size(pos2(icon_left, y - 12.0), vec2(24.0, 24.0));
            ui.painter()
                .image(texture.id(), snap(ui, r), FULL_UV, Color32::WHITE);
        }
        let name_color = if state.installed || dlc.mandatory {
            theme::WHITE
        } else {
            theme::DIM_NAME
        };
        ui.painter().text(
            pos2(icon_left + 24.0 + 10.0, y),
            Align2::LEFT_CENTER,
            dlc.name,
            theme::body(theme::BODY_SIZE),
            name_color,
        );

        // Tags, right-aligned before the arrow.
        let mut right = header.right() - 40.0 - 10.0;
        let mut tags: Vec<(&str, Color32)> = Vec::new();
        if !dlc.mandatory {
            tags.push(("Optional", theme::STEAM_BLUE));
        }
        if !state.owned && !dlc.mandatory {
            tags.push(("Not owned", NOT_OWNED));
        } else if !state.installed && !dlc.mandatory {
            tags.push(("Not installed", NOT_OWNED));
        }
        for (tag, color) in tags {
            let g = ui
                .painter()
                .layout_no_wrap(tag.into(), theme::body(theme::BODY_SIZE), color);
            right -= g.size().x;
            ui.painter()
                .galley(pos2(right, y - g.size().y / 2.0), g, color);
            right -= 12.0;
        }
        chevron(
            ui,
            gfx,
            pos2(header.right() - 20.0 - 6.0, y),
            expanded,
            theme::argb(0xFFF5F5F5),
        );

        if expanded {
            let detail = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
            let key_right = detail.left() + 150.0;
            if let std::collections::hash_map::Entry::Vacant(e) = self.sizes.entry(dlc.app_id) {
                e.insert(None);
                if let Some(path) = crate::fs_case::resolve(game_dir, dlc.folder) {
                    let (tx, id) = (self.sizes_tx.clone(), dlc.app_id);
                    std::thread::spawn(move || {
                        let _ = tx.send((id, dir_size_mb(&path)));
                    });
                }
            }
            let size = self.sizes.get(&dlc.app_id).copied().flatten();
            let rows = [
                ("Author:", dlc.author.to_owned()),
                (
                    "File size:",
                    size.map(|mb| format!("{mb:.0} MB")).unwrap_or_default(),
                ),
                (
                    "Purchased:",
                    if state.owned || dlc.mandatory {
                        "Yes"
                    } else {
                        "No"
                    }
                    .to_owned(),
                ),
                (
                    "Installed:",
                    if state.installed || dlc.mandatory {
                        "Yes"
                    } else {
                        "No"
                    }
                    .to_owned(),
                ),
            ];
            let mut y = detail.top() + 10.0;
            for (key, value) in rows {
                ui.painter().text(
                    pos2(key_right, y),
                    Align2::RIGHT_TOP,
                    key,
                    theme::body(theme::BODY_SIZE),
                    theme::DETAIL_KEY,
                );
                ui.painter().text(
                    pos2(key_right + 10.0, y),
                    Align2::LEFT_TOP,
                    value,
                    theme::body(theme::BODY_SIZE),
                    theme::WHITE,
                );
                y += 24.0;
            }
            // GET DLC / INSTALL DLC: 240 × 40, purple, 16 px.
            let button = Rect::from_min_size(pos2(key_right + 10.0, y + 10.0), vec2(240.0, 40.0));
            let label = if !state.owned && !dlc.mandatory {
                Some(("GET DLC", RowEvent::Get))
            } else if !state.installed && !dlc.mandatory && steam {
                Some(("INSTALL DLC", RowEvent::Install))
            } else {
                None
            };
            if let Some((text, e)) = label {
                let r = ui.interact(button, Id::unique(("dlc-get", dlc.app_id)), Sense::click());
                ui.painter().rect_filled(button, 0.0, PURPLE);
                if r.hovered() {
                    ui.painter()
                        .rect_filled(button, 0.0, theme::BUTTON_HOVER_OVERLAY);
                }
                ui.painter().text(
                    button.center(),
                    Align2::CENTER_CENTER,
                    text,
                    theme::body(16.0),
                    theme::WHITE,
                );
                if r.clicked() {
                    event = Some(e);
                }
            }
            // Preview: the DLC thumbnail in a black box with a #3F3F3F border.
            let preview = Rect::from_min_size(
                pos2(detail.right() - 13.0 - 256.0, detail.top() + 6.0),
                vec2(256.0, 128.0),
            );
            ui.painter().rect_filled(preview, 0.0, theme::BLACK);
            if let Some(texture) = gfx.bitmap(ui.ctx(), dlc.preview) {
                ui.painter()
                    .image(texture.id(), preview, FULL_UV, Color32::WHITE);
            }
            ui.painter().rect_stroke(
                preview,
                0.0,
                (1.0, theme::argb(0xFF3F3F3F)),
                egui::StrokeKind::Inside,
            );
        }
        event
    }
}

enum RowEvent {
    Toggle(bool),
    Install,
    Get,
}

fn dir_size_mb(path: &Path) -> f64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(entry.path()),
                Ok(_) => total += entry.metadata().map(|m| m.len()).unwrap_or(0),
                Err(_) => {}
            }
        }
    }
    total as f64 / 1_048_576.0
}
