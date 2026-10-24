//! The Mods page (`Views/Mods/AddonManagement*.xaml`): toolbar, loaded and available mods with
//! drag-and-drop load order, row details, and the PRESET popup.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align2, Color32, Id, Rect, Sense, UiBuilder, pos2, vec2};

use super::dependencies::{self, DependencyDialog};
use super::gfx::{FULL_UV, Gfx, snap};
use super::menu::{self, MessageBox};
use super::widgets::{
    ToolbarIcon, checkbox16, chevron, filter_box, flat_button, section_header, tab_control,
    toolbar, toolbar_button,
};
use super::{icons, theme};
use crate::config::LauncherConfig;
use crate::library::Library;
use crate::mods::{self, Mod, Source, Status};
use crate::steam::{self, ItemDetails, Steam};
use crate::vanilla::local_mods::LocalMods;
use crate::vanilla::preset_html::{self, ExportMod};
use crate::vanilla::{self, preset};

const HEADER_HEIGHT: f32 = 50.0;
const HINT_HEIGHT: f32 = 50.0;
const ROW_HEIGHT: f32 = 40.0;
const ROW_GAP: f32 = 1.0;
const DETAIL_HEIGHT: f32 = 214.0;
const PRESET_BUTTON_HEIGHT: f32 = 38.0;

/// What the page needs besides the library.
pub struct ModsEnv<'a> {
    pub config: &'a LauncherConfig,
    pub steam: Option<&'a Steam>,
    pub launcher_data: &'a Path,
    pub local: &'a mut LocalMods,
    pub message: &'a mut Option<MessageBox>,
    /// Workshop items subscribed here, polled until Steam has installed them.
    pub awaiting_items: &'a mut Vec<u64>,
    /// Steam details by workshop item, and author names by owner, for the dependency dialogs.
    pub workshop: &'a HashMap<u64, ItemDetails>,
    pub authors: &'a HashMap<u64, String>,
}

enum DialogResult {
    LocalMod(PathBuf),
    WatchedFolder(PathBuf),
    ImportPreset(PathBuf),
    ExportPreset(PathBuf, String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Loaded,
    Available,
}

enum Line {
    Header(Section),
    Hint,
    Row { index: usize, section: Section },
}

struct CollectionDialog {
    url: String,
    save_preset: bool,
    /// Set once Steam has been asked for this published file.
    pending_id: Option<u64>,
    focused: bool,
}

/// A workshop collection whose details have arrived.
pub struct FoundCollection {
    pub title: String,
    pub children: Vec<u64>,
    pub save_preset: bool,
}

/// A preset name vanilla will accept, taken from the collection title.
pub fn collection_preset_name(title: &str) -> String {
    let name: String = title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '_')
        .collect();
    let name: String = name.trim().chars().take(30).collect();
    let name = name.trim().to_owned();
    if preset::is_valid_name(&name) {
        name
    } else {
        "Collection".into()
    }
}

pub struct ModsPage {
    filter: String,
    expanded: HashSet<String>,
    dragging: Option<String>,
    preset_open: bool,
    save_as: Option<String>,
    renaming: Option<(String, String)>,
    pending_import: Option<PendingImport>,
    dependency_dialog: Option<DependencyDialog>,
    collection: Option<CollectionDialog>,
    /// Mods to load once Steam has described a collection.
    queued_load: Option<Vec<String>>,
    sizes: HashMap<PathBuf, Option<u64>>,
    sizes_tx: Sender<(PathBuf, u64)>,
    sizes_rx: Receiver<(PathBuf, u64)>,
    dialogs_tx: Sender<DialogResult>,
    dialogs_rx: Receiver<DialogResult>,
}

impl ModsPage {
    pub fn new() -> Self {
        let (sizes_tx, sizes_rx) = channel();
        let (dialogs_tx, dialogs_rx) = channel();
        Self {
            filter: String::new(),
            expanded: HashSet::new(),
            dragging: None,
            preset_open: false,
            save_as: None,
            renaming: None,
            pending_import: None,
            dependency_dialog: None,
            collection: None,
            queued_load: None,
            sizes: HashMap::new(),
            sizes_tx,
            sizes_rx,
            dialogs_tx,
            dialogs_rx,
        }
    }

    fn matches(&self, m: &Mod) -> bool {
        self.filter.is_empty() || m.name.to_lowercase().contains(&self.filter.to_lowercase())
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        lib: &mut Library,
        env: &mut ModsEnv,
    ) {
        self.handle_dialogs(lib, env);
        if let Some(ids) = self.queued_load.take() {
            self.request_load(ids, None, lib, env);
        }
        while let Ok((path, size)) = self.sizes_rx.try_recv() {
            self.sizes.insert(path, Some(size));
        }

        let (_, content) = tab_control(ui, "mods-tabs", page, &["Mods"], 0);
        let preset_button = self.preset_button(ui, gfx, page, lib);

        let bar = Rect::from_min_size(content.min, vec2(content.width(), 40.0));
        self.toolbar(ui, gfx, bar, env);

        let body = Rect::from_min_max(pos2(content.left(), bar.bottom() + 1.0), content.max);
        ui.painter().rect_filled(body, 0.0, theme::PANEL);
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        self.list(&mut body_ui, gfx, body, lib, env);

        if self.preset_open {
            self.preset_popup(ui, gfx, page, preset_button, lib, env);
        }
        self.import_prompts(ui, lib, env);
        self.dependency_prompts(ui, gfx, lib, env);
        self.collection_prompt(ui, gfx, env);
    }

    /// The PRESET toggle and its popup, for other pages that show them (DLC). Call after the
    /// page's content so the popup draws on top.
    pub fn preset_controls(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        lib: &mut Library,
        env: &mut ModsEnv,
    ) {
        self.handle_dialogs(lib, env);
        let toggle = self.preset_button(ui, gfx, page, lib);
        if self.preset_open {
            self.preset_popup(ui, gfx, page, toggle, lib, env);
        }
        self.import_prompts(ui, lib, env);
        self.dependency_prompts(ui, gfx, lib, env);
    }

    /// The PRESET toggle in the tab strip (`PresetManagerButtonStyle`).
    fn preset_button(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        lib: &Library,
    ) -> Rect {
        let (name, modified) = lib.preset_label();
        let font = theme::body(theme::BODY_SIZE);
        let label =
            ui.painter()
                .layout_no_wrap("PRESET".into(), font.clone(), theme::SECONDARY_LABEL);
        let value = ui
            .painter()
            .layout_no_wrap(name, font.clone(), theme::WHITE);
        let star = if modified { 10.0 } else { 0.0 };
        let width = 10.0 + label.size().x + 6.0 + value.size().x + star + 10.0 + 12.0 + 8.0;
        let rect = Rect::from_min_size(
            pos2(page.right() - width, page.top() + 2.0),
            vec2(width, PRESET_BUTTON_HEIGHT),
        );
        let response = ui.interact(rect, Id::unique("preset-toggle"), Sense::click());
        let background = if self.preset_open {
            theme::BLACK
        } else if response.hovered() {
            theme::argb(0x40FFFFFF)
        } else {
            theme::TAB_STRIP
        };
        ui.painter().rect_filled(rect, 0.0, background);
        let mut x = rect.left() + 10.0;
        let y = rect.center().y;
        let label_width = label.size().x;
        ui.painter().galley(
            pos2(x, y - label.size().y / 2.0),
            label,
            theme::SECONDARY_LABEL,
        );
        x += label_width + 6.0;
        let value_width = value.size().x;
        ui.painter()
            .galley(pos2(x, y - value.size().y / 2.0), value, theme::WHITE);
        x += value_width;
        if modified {
            ui.painter().text(
                pos2(x + 1.0, y),
                Align2::LEFT_CENTER,
                "*",
                font,
                theme::accent(),
            );
        }
        chevron(
            ui,
            gfx,
            pos2(rect.right() - 8.0 - 6.0, y),
            self.preset_open,
            theme::argb(0xFFF4F4F5),
        );
        if response.clicked() {
            self.preset_open = !self.preset_open;
            self.save_as = None;
            self.renaming = None;
        }
        rect
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, gfx: &mut Gfx, bar: Rect, env: &mut ModsEnv) {
        toolbar(ui, bar);
        let mut x = bar.left();
        let workshop = toolbar_button(
            ui,
            gfx,
            Id::unique("mods-workshop"),
            &mut x,
            bar,
            ToolbarIcon::Image("toolbar_button_addsteammod.png"),
            "Steam Workshop",
        );
        if workshop.clicked() {
            env.config.open_url("WorkshopWithModsUrl");
        }
        let local = toolbar_button(
            ui,
            gfx,
            Id::unique("mods-local"),
            &mut x,
            bar,
            ToolbarIcon::Image("toolbar_button_addlocalmod.png"),
            "Local mod",
        );
        if local.clicked() {
            self.pick_folder(DialogResult::LocalMod);
        }
        let more = toolbar_button(
            ui,
            gfx,
            Id::unique("mods-more"),
            &mut x,
            bar,
            ToolbarIcon::MoreDots,
            "More",
        );
        egui::Popup::menu(&more)
            .frame(menu::frame())
            .width(260.0)
            .show(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                if menu::item(
                    ui,
                    "Unsubscribe all Steam Workshop mods",
                    env.steam.is_some(),
                ) {
                    *env.message = Some(MessageBox {
                        caption: "Unsubscribe all".into(),
                        body:
                            "Unsubscribe from every Steam Workshop mod? Steam removes their files."
                                .into(),
                        buttons: vec!["Unsubscribe all".into(), "Cancel".into()],
                        confirm: Some(menu::Confirm::UnsubscribeAll),
                    });
                }
                menu::separator(ui);
                if menu::item(ui, "Export list of mods", true) {
                    self.save_file("Arma 3 Mods.html", "Mods".into());
                }
                if menu::item(ui, "Import list of mods", true) {
                    self.open_file();
                }
                menu::separator(ui);
                if menu::item(ui, "Add watched folder...", true) {
                    self.pick_folder(DialogResult::WatchedFolder);
                }
                if menu::item(ui, "Subscribe to collection...", true) {
                    self.collection = Some(CollectionDialog {
                        url: String::new(),
                        save_preset: true,
                        pending_id: None,
                        focused: false,
                    });
                }
            });

        let width = (bar.right() - x - 6.0).clamp(250.0, 400.0).min(300.0);
        let filter_rect = Rect::from_min_size(
            pos2(bar.right() - 3.0 - width, bar.top() + 4.0),
            vec2(width, 32.0),
        );
        filter_box(
            ui,
            gfx,
            Id::unique("mods-filter"),
            filter_rect,
            &mut self.filter,
            "Search mods (Ctrl+F)",
        );
        if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
            ui.memory_mut(|m| m.request_focus(Id::unique("mods-filter")));
        }
    }

    fn lines(&self, lib: &Library) -> Vec<Line> {
        let mut lines = vec![Line::Header(Section::Loaded)];
        let mut any_loaded = false;
        for id in &lib.loaded {
            if let Some(index) = lib.mods.iter().position(|m| &m.id == id)
                && self.matches(&lib.mods[index])
            {
                lines.push(Line::Row {
                    index,
                    section: Section::Loaded,
                });
                any_loaded = true;
            }
        }
        if !any_loaded {
            lines.push(Line::Hint);
        }
        lines.push(Line::Header(Section::Available));
        for (index, m) in lib.mods.iter().enumerate() {
            if !lib.is_loaded(&m.id) && self.matches(m) {
                lines.push(Line::Row {
                    index,
                    section: Section::Available,
                });
            }
        }
        lines
    }

    fn line_height(&self, line: &Line, lib: &Library) -> f32 {
        match line {
            Line::Header(_) => HEADER_HEIGHT,
            Line::Hint => HINT_HEIGHT,
            Line::Row { index, .. } => {
                ROW_GAP
                    + ROW_HEIGHT
                    + if self.expanded.contains(&lib.mods[*index].id) {
                        DETAIL_HEIGHT
                    } else {
                        0.0
                    }
            }
        }
    }

    fn list(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        body: Rect,
        lib: &mut Library,
        env: &mut ModsEnv,
    ) {
        let lines = self.lines(lib);
        let heights: Vec<f32> = lines.iter().map(|l| self.line_height(l, lib)).collect();
        let total: f32 = heights.iter().sum();
        let loaded_count = lib.loaded.len();
        let available_count = lib.mods.len() - loaded_count;
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let mut drop_target: Option<(Section, usize)> = None;
        let mut actions: Vec<RowAction> = Vec::new();

        egui::ScrollArea::vertical()
            .id_salt("mods-list")
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                ui.set_height(total);
                let origin = ui.min_rect().min;
                let width = body.width();
                let mut y = 0.0;
                let mut loaded_row = 0usize;
                let mut available_top = f32::MAX;
                for (line, height) in lines.iter().zip(&heights) {
                    let rect = Rect::from_min_size(origin + vec2(0.0, y), vec2(width, *height));
                    if matches!(line, Line::Header(Section::Available)) {
                        available_top = rect.top();
                    }
                    // Drop position among the loaded rows.
                    if let (Some(p), Some(_)) = (pointer, &self.dragging) {
                        if let Line::Row {
                            section: Section::Loaded,
                            ..
                        } = line
                            && p.y >= rect.top()
                            && p.y < rect.bottom()
                        {
                            let index = if p.y < rect.center().y {
                                loaded_row
                            } else {
                                loaded_row + 1
                            };
                            drop_target = Some((Section::Loaded, index));
                        }
                        if matches!(line, Line::Header(Section::Loaded) | Line::Hint)
                            && rect.contains(p)
                        {
                            drop_target = Some((Section::Loaded, 0));
                        }
                    }
                    if rect.bottom() >= viewport.min.y + origin.y - 1.0
                        && rect.top() <= viewport.max.y + origin.y + 1.0
                    {
                        match line {
                            Line::Header(Section::Loaded) => {
                                if section_header(
                                    ui,
                                    Id::unique("unload-all"),
                                    rect,
                                    "Loaded mods",
                                    (loaded_count > 0).then_some(loaded_count),
                                    Some("Unload all"),
                                ) {
                                    actions.push(RowAction::UnloadAll);
                                }
                            }
                            Line::Header(Section::Available) => {
                                if section_header(
                                    ui,
                                    Id::unique("load-all"),
                                    rect,
                                    "Available mods",
                                    Some(available_count),
                                    Some("Load all"),
                                ) {
                                    actions.push(RowAction::LoadAll);
                                }
                            }
                            Line::Hint => {
                                ui.painter().text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    "(Drag the mods that you want loaded in the game here.)",
                                    theme::body(theme::BODY_SIZE),
                                    theme::EMPTY_HINT,
                                );
                            }
                            Line::Row { index, section } => {
                                let row =
                                    Rect::from_min_max(rect.min + vec2(0.0, ROW_GAP), rect.max);
                                if let Some(action) = self.row(
                                    ui,
                                    gfx,
                                    row,
                                    &lib.mods[*index],
                                    *section == Section::Loaded,
                                    lib.dependency_badge(&lib.mods[*index]),
                                    lib.status(&lib.mods[*index]),
                                    env,
                                ) {
                                    actions.push(action);
                                }
                            }
                        }
                    }
                    if let Line::Row {
                        section: Section::Loaded,
                        ..
                    } = line
                    {
                        loaded_row += 1;
                    }
                    y += height;
                }
                if let (Some(p), Some(_)) = (pointer, &self.dragging) {
                    if p.y >= available_top {
                        drop_target = Some((Section::Available, 0));
                    } else if drop_target.is_none() && p.y > origin.y {
                        drop_target = Some((Section::Loaded, loaded_row));
                    }
                }
            });

        // Drag and drop: an insertion line while dragging, the move on release.
        if let Some(dragged) = self.dragging.clone() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            if let Some(p) = pointer {
                let name = lib
                    .find(&dragged)
                    .map(|m| m.name.clone())
                    .unwrap_or_default();
                let ghost = Rect::from_min_size(
                    pos2(body.left() + 30.0, p.y - ROW_HEIGHT / 2.0),
                    vec2(body.width() - 60.0, ROW_HEIGHT),
                );
                let painter = ui.ctx().layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    Id::unique("mods-drag"),
                ));
                painter.rect_filled(ghost, 0.0, theme::argb(0xCC333333));
                painter.text(
                    pos2(ghost.left() + 15.0, ghost.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    theme::body(theme::BODY_SIZE),
                    theme::WHITE,
                );
            }
            if !ui.input(|i| i.pointer.any_down()) {
                match drop_target {
                    Some((Section::Loaded, index)) => {
                        match lib.loaded.iter().position(|l| *l == dragged) {
                            Some(from) => {
                                lib.move_loaded(from, if index > from { index - 1 } else { index })
                            }
                            None => self.request_load(vec![dragged.clone()], Some(index), lib, env),
                        }
                    }
                    Some((Section::Available, _)) => {
                        self.dependency_dialog =
                            dependencies::request_unload(vec![dragged.clone()], lib)
                    }
                    None => {}
                }
                self.dragging = None;
            }
        }

        for action in actions {
            self.apply(action, lib, env);
        }
    }

    /// One mod row (`ExpanderListViewItemStyle`) and, when expanded, its details.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        rect: Rect,
        m: &Mod,
        loaded: bool,
        badge: Option<(usize, bool)>,
        status: Status,
        env: &mut ModsEnv,
    ) -> Option<RowAction> {
        let mut action = None;
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_HEIGHT));
        let expanded = self.expanded.contains(&m.id);
        let response = ui.interact(
            header,
            Id::unique(("mod-row", &m.id)),
            Sense::click_and_drag(),
        );
        let background = if response.hovered() && self.dragging.is_none() {
            theme::ROW_HOVER
        } else if expanded {
            theme::ROW_EXPANDED
        } else {
            theme::ROW
        };
        ui.painter().rect_filled(rect, 0.0, background);
        if response.drag_started() {
            self.dragging = Some(m.id.clone());
        }
        if response.clicked() {
            if expanded {
                self.expanded.remove(&m.id);
            } else {
                self.expanded.insert(m.id.clone());
                self.request_size(&m.path);
            }
        }

        let y = header.center().y;
        // Drag grip: 2 columns of dots, 0.2 → 0.6 opacity on hover.
        let grip_alpha = if response.hovered() { 0.6 } else { 0.2 };
        for row in 0..4 {
            for col in 0..2 {
                let dot = Rect::from_min_size(
                    pos2(
                        header.left() + 10.0 + col as f32 * 3.0,
                        y - 6.0 + row as f32 * 4.0,
                    ),
                    vec2(2.0, 2.0),
                );
                ui.painter()
                    .rect_filled(dot, 0.0, theme::WHITE.gamma_multiply(grip_alpha));
            }
        }
        let check = checkbox16(
            ui,
            gfx,
            Id::unique(("mod-check", &m.id)),
            pos2(header.left() + 25.0 + 12.0, y),
            loaded,
        );
        if check.clicked() {
            action = Some(if loaded {
                RowAction::Unload(m.id.clone())
            } else {
                RowAction::Load(m.id.clone())
            });
        }
        let status_left = header.left() + 25.0 + 24.0 + 5.0;
        if let Some(icon) = status.icon()
            && let Some(texture) = gfx.bitmap(ui.ctx(), icon)
        {
            let r = Rect::from_min_size(pos2(status_left, y - 8.0), vec2(16.0, 16.0));
            ui.painter()
                .image(texture.id(), snap(ui, r), FULL_UV, Color32::WHITE);
        }

        // Name block: name, Steam icon, tags.
        let mut x = status_left + 16.0 + 10.0;
        let name_color = if m.launchable() {
            theme::WHITE
        } else {
            theme::DIM_NAME
        };
        let name =
            ui.painter()
                .layout_no_wrap(m.name.clone(), theme::body(theme::BODY_SIZE), name_color);
        let name_width = name.size().x;
        ui.painter()
            .galley(pos2(x, y - name.size().y / 2.0), name, name_color);
        x += name_width + 8.0;
        if m.is_workshop() {
            let steam = Rect::from_min_size(pos2(x, y - 10.0), vec2(20.0, 20.0));
            gfx.paint_icon(
                ui,
                icons::STEAM_ICON,
                steam,
                vec2(20.0, 20.0),
                theme::WHITE.gamma_multiply(0.5),
            );
            x += 20.0 + 8.0;
        }
        let tags = [
            (m.updated, "updated", theme::accent()),
            (m.auto_included, "auto-included", theme::AUTO_INCLUDED),
        ];
        for (show, tag, color) in tags {
            if show {
                let g =
                    ui.painter()
                        .layout_no_wrap(tag.into(), theme::body(theme::BODY_SIZE), color);
                let w = g.size().x;
                ui.painter()
                    .galley(pos2(x + 2.0, y - g.size().y / 2.0), g, color);
                x += w + 10.0;
            }
        }
        if !m.dependencies.is_empty() {
            let link = Rect::from_center_size(
                pos2(header.right() - 40.0 - 5.0 - 8.0, y),
                vec2(16.0, 13.0),
            );
            gfx.paint_icon(
                ui,
                icons::LINK,
                link,
                vec2(16.0, 13.0),
                theme::WHITE.gamma_multiply(grip_alpha),
            );
        }
        let arrow_color = if response.hovered() {
            theme::WHITE
        } else {
            theme::argb(0xFFF5F5F5)
        };
        chevron(
            ui,
            gfx,
            pos2(header.right() - 20.0 - 6.0, y),
            expanded,
            arrow_color,
        );

        response.context_menu(|ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.set_min_width(200.0);
            if menu::item(ui, "Open web page", m.url().is_some()) {
                action = Some(RowAction::OpenPage(m.id.clone()));
            }
            if menu::item(ui, "Open folder", m.path.is_dir()) {
                let _ = open::that_detached(&m.path);
            }
            if menu::item(ui, "Repair", m.is_workshop() && env.steam.is_some()) {
                action = Some(RowAction::Repair(m.id.clone()));
            }
            if menu::item(ui, "Remove mod", true) {
                action = Some(RowAction::Remove(m.id.clone()));
            }
        });

        if expanded {
            let detail = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
            if let Some(a) = self.details(ui, gfx, detail, m, badge, status, env) {
                action = Some(a);
            }
        }
        action
    }

    /// `DetailDataTemplate`: key/value rows on the left, preview and buttons on the right.
    #[allow(clippy::too_many_arguments)]
    fn details(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        rect: Rect,
        m: &Mod,
        badge: Option<(usize, bool)>,
        status: Status,
        env: &mut ModsEnv,
    ) -> Option<RowAction> {
        let mut action = None;
        let right = Rect::from_min_max(
            pos2(rect.right() - 256.0 - 13.0, rect.top() + 6.0),
            pos2(rect.right() - 13.0, rect.bottom() - 8.0),
        );
        let source = match &m.source {
            Source::Workshop(_) => "Steam Workshop".to_owned(),
            Source::Local { windows_path } => windows_path.clone(),
        };
        let size = match self.sizes.get(&m.path) {
            Some(Some(bytes)) => format!("{:.0} MB", *bytes as f64 / 1_048_576.0),
            _ => "…".to_owned(),
        };
        let last_update = m
            .last_update
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%d/%m/%Y %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let rows = [
            ("Source:", source),
            // `ExtensionDetail_Author_Unknown` when Steam hasn't named the author.
            (
                "Author:",
                if m.author.is_empty() {
                    "unknown".to_owned()
                } else {
                    m.author.clone()
                },
            ),
            ("Last update:", last_update),
            ("File size:", size),
            ("Status:", status.label().to_owned()),
        ];
        let mut y = rect.top() + 10.0;
        let key_right = rect.left() + 150.0;
        for (key, value) in rows {
            ui.painter().text(
                pos2(key_right, y),
                Align2::RIGHT_TOP,
                key,
                theme::body(theme::BODY_SIZE),
                theme::DETAIL_KEY,
            );
            let max = right.left() - key_right - 20.0;
            let mut job = egui::text::LayoutJob::simple(
                value,
                theme::body(theme::BODY_SIZE),
                theme::WHITE,
                max,
            );
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            let galley = ui.painter().layout_job(job);
            ui.painter()
                .galley(pos2(key_right + 10.0, y), galley, theme::WHITE);
            y += 24.0;
        }

        // Preview: workshop placeholder in a black box with a #3F3F3F border.
        let preview = Rect::from_min_size(right.min, vec2(256.0, 162.0));
        ui.painter().rect_filled(preview, 0.0, theme::BLACK);
        // The workshop thumbnail the vanilla launcher cached, else its placeholder image.
        let thumbnail = m.workshop_id().map(|id| {
            env.launcher_data
                .join("images")
                .join(id.to_string())
                .join("thumbnail.jpg")
        });
        let placeholder = if m.is_workshop() {
            "arma3_workshop_mod.jpg"
        } else {
            "arma3_workshop_addon.jpg"
        };
        let texture = match thumbnail.filter(|t| t.is_file()) {
            Some(path) => gfx.file(ui.ctx(), &path).cloned(),
            None => gfx.bitmap(ui.ctx(), placeholder).cloned(),
        };
        if let Some(texture) = texture {
            let size = texture.size_vec2();
            let scale = (preview.width() / size.x).min(preview.height() / size.y);
            ui.painter().image(
                texture.id(),
                Rect::from_center_size(preview.center(), size * scale),
                FULL_UV,
                Color32::WHITE,
            );
        }
        ui.painter().rect_stroke(
            preview,
            0.0,
            (1.0, theme::argb(0xFF3F3F3F)),
            egui::StrokeKind::Inside,
        );

        // Four equal buttons (`DetailDataTemplate`): Workshop page, dependencies with their count
        // badge, More (repair, open folder), remove.
        let buttons = Rect::from_min_size(
            pos2(preview.left(), preview.bottom() + 4.0),
            vec2(256.0, 38.0),
        );
        let w = buttons.width() / 4.0;
        let enabled = [m.url().is_some(), m.is_workshop(), true, true];
        for (i, enabled) in enabled.into_iter().enumerate() {
            let b = Rect::from_min_size(
                pos2(buttons.left() + i as f32 * w, buttons.top()),
                vec2(w, buttons.height()),
            );
            ui.painter().rect_stroke(
                b,
                0.0,
                (1.0, theme::argb(0xFF3F3F3F)),
                egui::StrokeKind::Inside,
            );
            let response = ui.interact(
                b,
                Id::unique(("mod-detail-button", &m.id, i)),
                Sense::click(),
            );
            if enabled && response.hovered() {
                ui.painter()
                    .rect_filled(b.shrink(1.0), 0.0, theme::argb(0x7FFFFFFF));
            }
            let tint = if enabled {
                theme::WHITE
            } else {
                theme::WHITE.gamma_multiply(0.4)
            };
            match i {
                0 => gfx.paint_icon(ui, icons::OPEN_EXTERNAL, b, vec2(16.0, 16.0), tint),
                1 => {
                    gfx.paint_icon(ui, icons::LINK, b, vec2(21.5, 17.0), tint);
                    if let Some((count, satisfied)) = badge {
                        dependency_badge(ui, b, count, satisfied);
                    }
                }
                2 => {
                    // Three 3 × 3 squares.
                    for d in 0..3 {
                        let dot = Rect::from_min_size(
                            pos2(b.center().x - 7.5 + d as f32 * 6.0, b.center().y - 1.5),
                            vec2(3.0, 3.0),
                        );
                        ui.painter().rect_filled(dot, 0.0, tint);
                    }
                }
                _ => gfx.paint_icon(ui, icons::DELETE_ICON, b, vec2(16.0, 16.0), tint),
            }
            if i == 2 {
                let response = response.on_hover_text("Displays more options");
                egui::Popup::menu(&response)
                    .frame(menu::frame())
                    .width(200.0)
                    .show(|ui| {
                        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                        let can_repair = m.is_workshop() && env.steam.is_some();
                        if menu::item(ui, "Repair", can_repair) {
                            action = Some(RowAction::Repair(m.id.clone()));
                        }
                        if menu::item(ui, "Open folder", m.path.is_dir()) {
                            action = Some(RowAction::OpenFolder(m.id.clone()));
                        }
                    });
                continue;
            }
            if i == 1 {
                let _ = response
                    .clone()
                    .on_hover_text("Opens the dependencies view.");
            }
            if enabled && response.clicked() {
                action = Some(match i {
                    0 => RowAction::OpenPage(m.id.clone()),
                    1 => RowAction::ShowDependencies(m.id.clone()),
                    _ => RowAction::Remove(m.id.clone()),
                });
            }
        }
        action
    }

    fn apply(&mut self, action: RowAction, lib: &mut Library, env: &mut ModsEnv) {
        let filter = self.filter.to_lowercase();
        let visible = |m: &Mod| filter.is_empty() || m.name.to_lowercase().contains(&filter);
        match action {
            RowAction::Load(id) => self.request_load(vec![id], None, lib, env),
            RowAction::Unload(id) => {
                self.dependency_dialog = dependencies::request_unload(vec![id], lib)
            }
            // Every visible unloaded mod is an originator, so only missing dependencies ask.
            RowAction::LoadAll => {
                let ids: Vec<String> = lib
                    .mods
                    .iter()
                    .filter(|m| visible(m) && !lib.is_loaded(&m.id))
                    .map(|m| m.id.clone())
                    .collect();
                self.request_load(ids, None, lib, env)
            }
            RowAction::ShowDependencies(id) => {
                self.dependency_dialog = Some(DependencyDialog::list(id))
            }
            RowAction::UnloadAll => lib.unload_all(visible),
            RowAction::OpenPage(id) => {
                if let Some(url) = lib.find(&id).and_then(Mod::url) {
                    let _ = open::that_detached(url);
                }
            }
            RowAction::OpenFolder(id) => {
                if let Some(m) = lib.find(&id) {
                    let _ = open::that_detached(&m.path);
                }
            }
            RowAction::Repair(id) => {
                if let (Some(steam), Some(item)) =
                    (env.steam, lib.find(&id).and_then(Mod::workshop_id))
                {
                    steam.download(item);
                }
            }
            RowAction::Remove(id) => match lib.find(&id).map(|m| m.source.clone()) {
                Some(Source::Workshop(item)) => {
                    if let Some(steam) = env.steam {
                        steam.unsubscribe(item);
                    }
                }
                Some(Source::Local { windows_path }) => {
                    env.local
                        .user_directories
                        .retain(|d| !d.eq_ignore_ascii_case(&windows_path));
                    env.local
                        .known_local_mods
                        .retain(|d| !d.eq_ignore_ascii_case(&windows_path));
                    if let Err(e) = env.local.save(env.launcher_data) {
                        *env.message = Some(MessageBox::info(
                            "Remove mod",
                            format!("Could not save Local.json: {e}"),
                        ));
                    }
                    lib.remove(&id);
                }
                None => {}
            },
        }
    }

    fn request_size(&mut self, path: &Path) {
        if self.sizes.contains_key(path) {
            return;
        }
        self.sizes.insert(path.to_path_buf(), None);
        let (tx, path) = (self.sizes_tx.clone(), path.to_path_buf());
        std::thread::spawn(move || {
            let _ = tx.send((path.clone(), dir_size(&path)));
        });
    }

    fn pick_folder(&self, wrap: fn(PathBuf) -> DialogResult) {
        let tx = self.dialogs_tx.clone();
        std::thread::spawn(move || {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                let _ = tx.send(wrap(folder));
            }
        });
    }

    fn open_file(&self) {
        let tx = self.dialogs_tx.clone();
        std::thread::spawn(move || {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("Arma 3 preset", &["html", "htm"])
                .pick_file()
            {
                let _ = tx.send(DialogResult::ImportPreset(file));
            }
        });
    }

    fn save_file(&self, default_name: &str, preset_name: String) {
        let tx = self.dialogs_tx.clone();
        let default_name = default_name.to_owned();
        std::thread::spawn(move || {
            if let Some(file) = rfd::FileDialog::new()
                .set_file_name(default_name)
                .add_filter("HTML", &["html"])
                .save_file()
            {
                let _ = tx.send(DialogResult::ExportPreset(file, preset_name));
            }
        });
    }

    fn handle_dialogs(&mut self, lib: &mut Library, env: &mut ModsEnv) {
        while let Ok(result) = self.dialogs_rx.try_recv() {
            match result {
                DialogResult::LocalMod(folder) => {
                    let windows = vanilla::game_path(&folder);
                    if !env
                        .local
                        .user_directories
                        .iter()
                        .any(|d| d.eq_ignore_ascii_case(&windows))
                    {
                        env.local.user_directories.push(windows.clone());
                        env.local.known_local_mods.push(windows.clone());
                        if let Err(e) = env.local.save(env.launcher_data) {
                            *env.message = Some(MessageBox::info(
                                "Local mod",
                                format!("Could not save Local.json: {e}"),
                            ));
                        }
                    }
                    let m = mods::local_mod(&windows, folder, false);
                    if lib.find(&m.id).is_none() {
                        lib.mods.push(m);
                        lib.mods.sort_by_key(|m| m.name.to_lowercase());
                    }
                }
                DialogResult::WatchedFolder(folder) => {
                    let windows = vanilla::game_path(&folder);
                    if !env
                        .local
                        .autodetection_directories
                        .iter()
                        .any(|d| d.eq_ignore_ascii_case(&windows))
                    {
                        env.local.autodetection_directories.push(windows);
                        let _ = env.local.save(env.launcher_data);
                        *env.message = Some(MessageBox::info(
                            "Watched folder",
                            "The folder's mods appear the next time the launcher starts.",
                        ));
                    }
                }
                DialogResult::ImportPreset(file) => self.import_preset(&file, lib, env),
                DialogResult::ExportPreset(file, name) => {
                    let ids: Vec<String> = if name == "Mods" {
                        lib.mods.iter().map(|m| m.id.clone()).collect()
                    } else {
                        lib.preset(&name)
                            .map(|p| p.mods.clone())
                            .unwrap_or_default()
                    };
                    let export: Vec<(String, String, Option<u64>)> = ids
                        .iter()
                        .filter_map(|id| lib.find(id))
                        .map(|m| {
                            (
                                m.name.clone(),
                                m.path
                                    .file_name()
                                    .map(|f| f.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                                m.workshop_id(),
                            )
                        })
                        .collect();
                    let mods: Vec<ExportMod> = export
                        .iter()
                        .map(|(name, folder, id)| match id {
                            Some(id) => ExportMod::Workshop { name, id: *id },
                            None => ExportMod::Local {
                                name,
                                folder,
                                url: "",
                            },
                        })
                        .collect();
                    if let Err(e) = std::fs::write(&file, preset_html::export(&name, &mods, &[])) {
                        *env.message = Some(MessageBox::info(
                            "Export",
                            format!("Could not write {}: {e}", file.display()),
                        ));
                    }
                }
            }
        }
    }

    /// Handles a file dropped on the window: preset HTML files are imported, folders added as
    /// local mods.
    pub fn queue_drop(&self, path: PathBuf) {
        let ext = path.extension().map(|e| e.to_ascii_lowercase());
        let result = if path.is_dir() {
            DialogResult::LocalMod(path)
        } else if ext.as_deref().is_some_and(|e| e == "html" || e == "htm") {
            DialogResult::ImportPreset(path)
        } else {
            return;
        };
        let _ = self.dialogs_tx.send(result);
    }

    /// Starts importing a preset HTML file (`ImportPresetAsync`); a preset with the same name
    /// first asks whether to overwrite or rename.
    fn import_preset(&mut self, file: &Path, lib: &mut Library, env: &mut ModsEnv) {
        self.preset_open = false;
        let Some(imported) = std::fs::read(file)
            .ok()
            .and_then(|b| preset_html::import(&b))
        else {
            *env.message = Some(MessageBox::info("Import", "Unable to import preset."));
            return;
        };
        let mut name: String = imported
            .name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '_')
            .collect();
        name = name.trim().chars().take(30).collect();
        if name.is_empty() {
            name = "Imported preset".into();
        }
        let exists = lib.preset(&name).is_some();
        let import = PendingImport {
            imported,
            name,
            step: ImportStep::Exists,
        };
        if exists {
            self.pending_import = Some(import);
        } else {
            self.continue_import(import, lib, env);
        }
    }

    /// The import after the name is settled: without Steam nothing can be subscribed, so it
    /// stops; missing workshop mods are offered for subscription; then the preset is saved.
    fn continue_import(&mut self, mut import: PendingImport, lib: &mut Library, env: &mut ModsEnv) {
        if env.steam.is_none() {
            *env.message = Some(MessageBox::info(
                "Mod import",
                "Mods can't be subscribed because Steam is offline. Please try it again later.",
            ));
            return;
        }
        let missing: Vec<(u64, String)> = import
            .imported
            .workshop_ids
            .iter()
            .zip(&import.imported.workshop_names)
            .filter(|(id, _)| lib.find(&mods::steam_id(**id)).is_none())
            .map(|(id, name)| (*id, name.clone()))
            .collect();
        if missing.is_empty() {
            self.finish_import(&import.imported, &import.name, lib, env);
        } else {
            import.step = ImportStep::Subscribe(missing);
            self.pending_import = Some(import);
        }
    }

    /// Saves and activates the imported preset. Workshop mods stay in it before Steam has
    /// installed them; local mods are matched by name or folder, and unmatched ones listed.
    fn finish_import(
        &mut self,
        imported: &preset_html::Imported,
        name: &str,
        lib: &mut Library,
        env: &mut ModsEnv,
    ) {
        let mut ids: Vec<String> = imported
            .workshop_ids
            .iter()
            .map(|id| {
                let id = mods::steam_id(*id);
                lib.find(&id).map_or(id, |m| m.id.clone())
            })
            .collect();
        let mut not_found = Vec::new();
        for (meta_name, folder) in &imported.local {
            let found = lib.mods.iter().find(|m| {
                !m.is_workshop()
                    && (m.name.eq_ignore_ascii_case(meta_name)
                        || m.path
                            .file_name()
                            .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(folder)))
            });
            match found {
                Some(m) => ids.push(m.id.clone()),
                None => not_found.push(format!("- {meta_name} ({folder})")),
            }
        }
        let mut seen = HashSet::new();
        ids.retain(|id| seen.insert(id.clone()));
        lib.loaded = ids;
        lib.dlcs = imported.dlc_app_ids.clone();
        if let Err(e) = lib.save_preset(name) {
            *env.message = Some(MessageBox::info(
                "Import",
                format!("Could not save the preset: {e}"),
            ));
        } else if !not_found.is_empty() {
            *env.message = Some(MessageBox::info(
                "Mod import",
                format!(
                    "Some of the local mods in the import preset couldn't be found in your mod library.\n\nYou have to get them manually and then import the preset again:\n{}",
                    not_found.join("\n")
                ),
            ));
        }
    }

    /// Paste a workshop collection URL (`Subscribe to collection...`).
    fn collection_prompt(&mut self, ui: &mut egui::Ui, gfx: &mut Gfx, env: &mut ModsEnv) {
        let Some(dialog) = &mut self.collection else {
            return;
        };
        let win = ui.max_rect();
        ui.interact(
            win,
            Id::unique("collection-overlay"),
            Sense::click_and_drag(),
        );
        ui.painter().rect_filled(win, 0.0, theme::STRONG_PANEL);
        let rect = Rect::from_center_size(win.center(), vec2(460.0, 210.0));
        ui.painter().rect_filled(rect, 0.0, theme::argb(0xFF111111));
        ui.painter().rect_stroke(
            rect,
            0.0,
            (1.0, theme::argb(0xFF222222)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            pos2(rect.left() + 16.0, rect.top() + 22.0),
            Align2::LEFT_CENTER,
            "Workshop collection",
            theme::body(theme::HEADER_SIZE),
            theme::WHITE,
        );
        ui.painter().text(
            pos2(rect.left() + 16.0, rect.top() + 52.0),
            Align2::LEFT_CENTER,
            "Paste a Steam workshop collection link.",
            theme::body(theme::BODY_SIZE),
            theme::HINT_TEXT,
        );
        let field = Rect::from_min_size(
            pos2(rect.left() + 16.0, rect.top() + 70.0),
            vec2(428.0, 28.0),
        );
        ui.painter().rect_filled(field, 0.0, theme::BLACK);
        ui.painter().rect_stroke(
            field,
            0.0,
            (1.0, theme::argb(0xFF333333)),
            egui::StrokeKind::Inside,
        );
        let pending = dialog.pending_id.is_some();
        let edit = egui::TextEdit::singleline(&mut dialog.url)
            .id(Id::unique("collection-url"))
            .frame(egui::Frame::NONE)
            .font(theme::body(theme::BODY_SIZE))
            .text_color(theme::WHITE)
            .hint_text("https://steamcommunity.com/sharedfiles/filedetails/?id=")
            .interactive(!pending);
        let response = ui.put(field.shrink2(vec2(6.0, 1.0)), edit);
        if !dialog.focused {
            response.request_focus();
            dialog.focused = true;
        }
        let check = checkbox16(
            ui,
            gfx,
            Id::unique("collection-save"),
            pos2(rect.left() + 24.0, rect.top() + 122.0),
            dialog.save_preset,
        );
        if check.clicked() && !pending {
            dialog.save_preset = !dialog.save_preset;
        }
        ui.painter().text(
            pos2(rect.left() + 40.0, rect.top() + 122.0),
            Align2::LEFT_CENTER,
            "Save as preset",
            theme::body(theme::BODY_SIZE),
            theme::WHITE,
        );
        let subscribe = Rect::from_min_size(
            pos2(rect.right() - 196.0, rect.bottom() - 46.0),
            vec2(100.0, 30.0),
        );
        let cancel = Rect::from_min_size(
            pos2(rect.right() - 90.0, rect.bottom() - 46.0),
            vec2(74.0, 30.0),
        );
        let label = if pending {
            "Contacting Steam..."
        } else {
            "Subscribe"
        };
        let clicked =
            flat_button(ui, Id::unique("collection-subscribe"), subscribe, label).clicked();
        let submit = !pending
            && (clicked
                || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))));
        let cancel = flat_button(ui, Id::unique("collection-cancel"), cancel, "Cancel").clicked();
        if cancel {
            self.collection = None;
            return;
        }
        if !submit {
            return;
        }
        let url = self
            .collection
            .as_ref()
            .map(|d| d.url.clone())
            .unwrap_or_default();
        let Some(id) = steam::published_file_id(&url) else {
            *env.message = Some(MessageBox::info(
                "Workshop collection",
                "Paste a workshop collection link.",
            ));
            return;
        };
        let Some(steam) = env.steam else {
            *env.message = Some(MessageBox::info(
                "Workshop collection",
                "Mods can't be subscribed because Steam is offline. Please try it again later.",
            ));
            return;
        };
        steam.query_details(&[id]);
        if let Some(dialog) = &mut self.collection {
            dialog.pending_id = Some(id);
        }
    }

    /// Workshop details arrived. If they describe the collection the dialog is waiting on, the
    /// dialog closes and the caller subscribes to its mods.
    pub fn take_collection(&mut self, details: &[ItemDetails]) -> Option<FoundCollection> {
        let id = self
            .collection
            .as_ref()
            .and_then(|dialog| dialog.pending_id)?;
        let item = details.iter().find(|detail| detail.id == id)?;
        let save_preset = self
            .collection
            .as_ref()
            .is_some_and(|dialog| dialog.save_preset);
        self.collection = None;
        Some(FoundCollection {
            title: item.title.clone(),
            children: item.children.clone(),
            save_preset,
        })
    }

    /// Loads these mods on the next frame, through the dependency dialog.
    pub fn queue_load(&mut self, ids: Vec<String>) {
        self.queued_load = Some(ids);
    }

    /// Loads mods, asking first when they need dependencies that aren't loaded.
    pub fn request_load(
        &mut self,
        ids: Vec<String>,
        index: Option<usize>,
        lib: &mut Library,
        env: &ModsEnv,
    ) {
        self.dependency_dialog = dependencies::request_load(ids, index, lib, env.workshop);
    }

    /// The open dependency dialog, over the window.
    fn dependency_prompts(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        lib: &mut Library,
        env: &mut ModsEnv,
    ) {
        let Some(dialog) = &mut self.dependency_dialog else {
            return;
        };
        let sizes = &self.sizes;
        let size_of = |path: &Path| sizes.get(path).copied().flatten();
        let mut dialog_env = dependencies::DialogEnv {
            steam: env.steam,
            workshop: env.workshop,
            authors: env.authors,
            awaiting_items: env.awaiting_items,
            size_of: &size_of,
        };
        // An Area, so the dialog owns the pointer and the scroll wheel over the window.
        let win = ui.max_rect();
        let ctx = ui.ctx().clone();
        let open = egui::Area::new(Id::unique("dependency-dialog"))
            .order(egui::Order::Foreground)
            .fixed_pos(win.min)
            .constrain(false)
            .show(&ctx, |ui| {
                // Sizes the Area to the window: egui hit-tests layers by their area's rect.
                ui.allocate_rect(win, Sense::hover());
                let mut dialog_ui = ui.new_child(UiBuilder::new().max_rect(win));
                dialog_ui.set_clip_rect(win);
                dependencies::show(dialog, &mut dialog_ui, gfx, lib, &mut dialog_env)
            })
            .inner;
        if !open {
            self.dependency_dialog = None;
        }
    }

    /// The import's questions, as message boxes over the window.
    fn import_prompts(&mut self, ui: &mut egui::Ui, lib: &mut Library, env: &mut ModsEnv) {
        let Some(import) = self.pending_import.take() else {
            return;
        };
        let message = match &import.step {
            ImportStep::Exists => MessageBox {
                caption: "Preset already exists".into(),
                body: format!(
                    "Preset '{}' already exists. Do you wish to overwrite it or rename?",
                    import.name
                ),
                buttons: vec!["Overwrite".into(), "Rename".into(), "Cancel".into()],
                confirm: None,
            },
            ImportStep::Subscribe(missing) => MessageBox {
                caption: "Subscribe mods".into(),
                body: format!(
                    "Do you want subscribe these mods:\n{}",
                    missing
                        .iter()
                        .map(|(id, name)| {
                            if name.is_empty() {
                                format!("- {id}")
                            } else {
                                format!("- {name}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                buttons: vec!["Subscribe".into(), "No".into()],
                confirm: None,
            },
        };
        let win = ui.max_rect();
        let answer = message.show(ui, win);
        let exists = matches!(import.step, ImportStep::Exists);
        match (answer, exists) {
            (None, _) => self.pending_import = Some(import),
            (Some(0), true) => self.continue_import(import, lib, env),
            (Some(1), true) => {
                let mut renamed = import;
                let base = renamed.name.clone();
                let mut n = 1;
                renamed.name = loop {
                    let name: String = format!("{base} ({n})").chars().take(30).collect();
                    if lib.preset(&name).is_none() {
                        break name;
                    }
                    n += 1;
                };
                self.continue_import(renamed, lib, env);
            }
            (Some(0), false) => {
                if let (Some(steam), ImportStep::Subscribe(missing)) = (env.steam, &import.step) {
                    let ids: Vec<u64> = missing.iter().map(|(id, _)| *id).collect();
                    for id in &ids {
                        steam.subscribe(*id);
                    }
                    steam.query_details(&ids);
                    env.awaiting_items.extend(ids);
                }
                self.finish_import(&import.imported, &import.name, lib, env);
            }
            // Cancel, or No: vanilla stops the import there.
            (Some(_), _) => {}
        }
    }

    /// The preset popup (`AddonManagement.PresetPopupContent`): save, save as, the preset list
    /// with share/rename/delete, and import.
    fn preset_popup(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        toggle: Rect,
        lib: &mut Library,
        env: &mut ModsEnv,
    ) {
        // Right-aligned and as wide as its content (`MaxWidth` 641): the preset rows are
        // [name, min 240][24 share][24 rename][24 delete] in a 10 px margin and a 1 px border.
        let font = theme::body(theme::BODY_SIZE);
        let names = lib
            .presets
            .iter()
            .map(|(name, _)| {
                ui.painter()
                    .layout_no_wrap(format!("{name}  \u{2713}"), font.clone(), theme::WHITE)
                    .size()
                    .x
                    + 8.0
            })
            .fold(0.0f32, f32::max);
        let width = (names.max(240.0) + 72.0 + 21.0)
            .min(641.0)
            .min(page.width());
        let rect = Rect::from_min_max(
            pos2(page.right() - width, toggle.bottom()),
            pos2(page.right(), page.bottom()),
        );
        // An Area, so the popup owns the pointer and the scroll wheel over its rect.
        let ctx = ui.ctx().clone();
        egui::Area::new(Id::unique("preset-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .constrain(false)
            .show(&ctx, |ui| {
                ui.set_clip_rect(rect);
                self.preset_popup_content(ui, gfx, rect, toggle, lib, env);
            });
    }

    fn preset_popup_content(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        rect: Rect,
        toggle: Rect,
        lib: &mut Library,
        env: &mut ModsEnv,
    ) {
        ui.allocate_rect(rect, Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, theme::BLACK);
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(1.0, rect.height())),
            0.0,
            theme::argb(0xFF222222),
        );
        let inner = rect.shrink(10.0);
        let mut y = inner.top();

        let active = lib.active_preset.clone();
        let save = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 40.0));
        if active.is_some()
            && flat_button(ui, Id::unique("preset-save"), save, "SAVE").clicked()
            && let Some(name) = &active
            && let Err(e) = lib.save_preset(name)
        {
            *env.message = Some(MessageBox::info("Save preset", e.to_string()));
        }
        if active.is_some() {
            y += 44.0;
        }
        let save_as = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 40.0));
        match &mut self.save_as {
            None => {
                if flat_button(ui, Id::unique("preset-save-as"), save_as, "SAVE AS…").clicked() {
                    self.save_as = Some(String::new());
                }
            }
            Some(name) => {
                let field =
                    Rect::from_min_max(save_as.min, pos2(save_as.right() - 52.0, save_as.bottom()));
                ui.painter().rect_filled(field, 0.0, theme::BLACK);
                ui.painter().rect_stroke(
                    field,
                    0.0,
                    (1.0, theme::accent()),
                    egui::StrokeKind::Inside,
                );
                let edit = egui::TextEdit::singleline(name)
                    .id(Id::unique("preset-name"))
                    .frame(egui::Frame::NONE)
                    .font(theme::body(theme::BODY_SIZE))
                    .text_color(theme::WHITE)
                    .hint_text("Preset name")
                    .align(egui::Align2::LEFT_CENTER);
                let response = ui.put(field.shrink2(vec2(6.0, 1.0)), edit);
                response.request_focus();
                let valid = preset::is_valid_name(name);
                let ok =
                    Rect::from_min_size(pos2(field.right() + 4.0, save_as.top()), vec2(24.0, 40.0));
                let cancel = Rect::from_min_size(pos2(ok.right(), save_as.top()), vec2(24.0, 40.0));
                gfx.paint_icon(
                    ui,
                    icons::ACCEPT_ICON,
                    ok,
                    vec2(13.0, 12.0),
                    if valid {
                        theme::WHITE
                    } else {
                        theme::HINT_TEXT
                    },
                );
                gfx.paint_icon(
                    ui,
                    icons::CANCEL_ICON,
                    cancel,
                    vec2(13.0, 13.0),
                    theme::WHITE,
                );
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if valid
                    && (enter
                        || ui
                            .interact(ok, Id::unique("preset-ok"), Sense::click())
                            .clicked())
                {
                    let name = name.trim().to_owned();
                    if let Err(e) = lib.save_preset(&name) {
                        *env.message = Some(MessageBox::info("Save preset", e.to_string()));
                    }
                    self.save_as = None;
                } else if ui
                    .interact(cancel, Id::unique("preset-cancel"), Sense::click())
                    .clicked()
                    || ui.input(|i| i.key_pressed(egui::Key::Escape))
                {
                    self.save_as = None;
                }
            }
        }
        y += 44.0;

        let import = Rect::from_min_max(
            pos2(inner.left(), inner.bottom() - 80.0),
            pos2(inner.right(), inner.bottom() - 40.0),
        );
        if flat_button(ui, Id::unique("preset-import"), import, "Import").clicked() {
            self.preset_open = false;
            self.open_file();
        }

        let list = Rect::from_min_max(
            pos2(inner.left(), y),
            pos2(inner.right(), import.top() - 4.0),
        );
        let mut list_ui = ui.new_child(UiBuilder::new().max_rect(list));
        list_ui.set_clip_rect(list);
        let names: Vec<String> = lib.presets.iter().map(|(n, _)| n.clone()).collect();
        let mut pending: Option<PresetAction> = None;
        egui::ScrollArea::vertical()
            .id_salt("preset-list")
            .auto_shrink([false, false])
            .show(&mut list_ui, |ui| {
                for name in &names {
                    let (row, row_response) =
                        ui.allocate_exact_size(vec2(list.width(), 40.0), Sense::hover());
                    if row_response.hovered() {
                        ui.painter().rect_filled(row, 0.0, theme::argb(0x10FFFFFF));
                    }
                    let select = Rect::from_min_max(
                        row.min + vec2(0.0, 2.0),
                        pos2(row.right() - 72.0, row.bottom() - 2.0),
                    );
                    if let Some((from, to)) = &mut self.renaming
                        && from == name
                    {
                        let edit = egui::TextEdit::singleline(to)
                            .frame(egui::Frame::NONE)
                            .font(theme::body(theme::BODY_SIZE))
                            .text_color(theme::WHITE)
                            .align(egui::Align2::LEFT_CENTER);
                        ui.painter().rect_stroke(
                            select,
                            0.0,
                            (1.0, theme::accent()),
                            egui::StrokeKind::Inside,
                        );
                        let r = ui.put(select.shrink2(vec2(6.0, 1.0)), edit);
                        r.request_focus();
                        if r.lost_focus() {
                            if ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && preset::is_valid_name(to)
                            {
                                pending =
                                    Some(PresetAction::Rename(from.clone(), to.trim().to_owned()));
                            } else {
                                pending = Some(PresetAction::CancelRename);
                            }
                        }
                    } else {
                        let response = ui.interact(
                            select,
                            Id::unique(("preset-select", name)),
                            Sense::click(),
                        );
                        let color = if response.hovered() {
                            theme::WHITE
                        } else {
                            theme::argb(0xCCFFFFFF)
                        };
                        if response.hovered() {
                            ui.painter()
                                .rect_filled(select, 0.0, theme::argb(0xFF333333));
                        }
                        let label = if lib.active_preset.as_deref() == Some(name.as_str()) {
                            format!("{name}  \u{2713}")
                        } else {
                            name.clone()
                        };
                        ui.painter().text(
                            pos2(select.left() + 4.0, select.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            theme::body(theme::BODY_SIZE),
                            color,
                        );
                        if response.clicked() {
                            pending = Some(PresetAction::Apply(name.clone()));
                        }
                    }
                    let icons_row: [PresetIcon; 3] = [
                        (icons::SHARE, vec2(16.0, 16.0), PresetAction::Share),
                        (icons::EDIT_ICON, vec2(14.0, 4.0), PresetAction::StartRename),
                        (icons::DELETE_ICON, vec2(9.0, 12.0), PresetAction::Delete),
                    ];
                    for (i, (icon, size, make)) in icons_row.into_iter().enumerate() {
                        let b = Rect::from_min_size(
                            pos2(row.right() - 72.0 + i as f32 * 24.0, row.top()),
                            vec2(24.0, 40.0),
                        );
                        let response =
                            ui.interact(b, Id::unique(("preset-icon", name, i)), Sense::click());
                        let tint = if response.hovered() {
                            theme::WHITE
                        } else {
                            theme::argb(0xCCFFFFFF)
                        };
                        gfx.paint_icon(ui, icon, b, size, tint);
                        if response.clicked() {
                            pending = Some(make(name.clone()));
                        }
                    }
                }
            });

        match pending {
            Some(PresetAction::Apply(name)) => {
                lib.apply_preset(&name);
                self.preset_open = false;
            }
            Some(PresetAction::Share(name)) => {
                self.save_file(&format!("Arma 3 Preset {name}.html"), name)
            }
            Some(PresetAction::StartRename(name)) => self.renaming = Some((name.clone(), name)),
            Some(PresetAction::CancelRename) => self.renaming = None,
            Some(PresetAction::Rename(from, to)) => {
                if from != to
                    && let Err(e) = lib.rename_preset(&from, &to)
                {
                    *env.message = Some(MessageBox::info("Rename preset", e.to_string()));
                }
                self.renaming = None;
            }
            Some(PresetAction::Delete(name)) => {
                if let Err(e) = lib.delete_preset(&name) {
                    *env.message = Some(MessageBox::info("Delete preset", e.to_string()));
                }
            }
            None => {}
        }

        // Clicking outside the popup closes it.
        let clicked_outside = ui.input(|i| i.pointer.primary_clicked())
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| !rect.contains(p) && !toggle.contains(p));
        if clicked_outside {
            self.preset_open = false;
        }
    }
}

/// A preset import waiting on the user (`ImportPresetAsync`).
struct PendingImport {
    imported: preset_html::Imported,
    /// The name it is saved under.
    name: String,
    step: ImportStep,
}

enum ImportStep {
    /// A preset with this name exists: overwrite, rename or cancel.
    Exists,
    /// These workshop mods (id, name) aren't in the library: subscribe or stop.
    Subscribe(Vec<(u64, String)>),
}

enum RowAction {
    Load(String),
    Unload(String),
    ShowDependencies(String),
    LoadAll,
    UnloadAll,
    OpenPage(String),
    OpenFolder(String),
    Repair(String),
    Remove(String),
}

/// A preset row icon: glyph, size, and the action it starts.
type PresetIcon = (&'static str, egui::Vec2, fn(String) -> PresetAction);

enum PresetAction {
    Apply(String),
    Share(String),
    StartRename(String),
    CancelRename,
    Rename(String, String),
    Delete(String),
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
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
    total
}

/// The dependency count badge (`DependenciesButtonStyle`): 17 × 17 in the button's bottom-right
/// corner, red while a dependency isn't satisfied, white once all are.
fn dependency_badge(ui: &egui::Ui, button: Rect, count: usize, satisfied: bool) {
    let rect = Rect::from_min_size(
        pos2(button.right() - 3.0 - 17.0, button.bottom() - 3.0 - 17.0),
        vec2(17.0, 17.0),
    );
    let fill = if satisfied {
        theme::WHITE
    } else {
        theme::ERROR
    };
    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().rect_stroke(
        rect,
        3.0,
        (1.0, theme::argb(0x99000000)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        count.to_string(),
        egui::FontId::new(11.0, egui::FontFamily::Name("roboto-medium".into())),
        Color32::BLACK,
    );
}
