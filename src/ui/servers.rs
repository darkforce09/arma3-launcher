//! The Servers page (`Views/Servers/ServersPage.xaml`, `InternetServersTabControl`): Steam
//! server lists by tab, search and filters, sortable rows with details, favourites, Direct
//! Connect and the join flow.

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align2, Color32, Id, Rect, Sense, UiBuilder, pos2, vec2};
use serde::{Deserialize, Serialize};

use super::gfx::{FULL_UV, Gfx, snap};
use super::widgets::{
    self, checkbox16, chevron, combo, flat_button, flat_button_width, tab_control, text_box,
    toolbar,
};
use super::{menu, theme};
use crate::library::Library;
use crate::mods::Status;
use crate::servers::rules::{Rules, ServerMod};
use crate::servers::{self, BiLists, Filters, Server, Sort};
use crate::steam::{ServerListHandle, ServerListKind, Steam, SteamEvent};
use crate::units::Units;

type Key = (Ipv4Addr, u16);

const ROW_HEIGHT: f32 = 40.0;
const DETAIL_HEIGHT: f32 = 200.0;
const HEADER_HEIGHT: f32 = 30.0;
/// Rules queries in flight at once.
const RULES_IN_FLIGHT: usize = 8;
/// Rules queries in flight while an Advanced filter waits on the servers' rules.
const FILTER_RULES_IN_FLIGHT: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Favorites,
    Recent,
    Official,
    Friends,
    Internet,
    Units,
}

impl Tab {
    const ALL: [Tab; 6] = [
        Tab::Favorites,
        Tab::Recent,
        Tab::Official,
        Tab::Friends,
        Tab::Internet,
        Tab::Units,
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Favorites => "Favorites",
            Tab::Recent => "Recent",
            Tab::Official => "Official",
            Tab::Friends => "Friends",
            Tab::Internet => "Internet",
            Tab::Units => "Units",
        }
    }

    /// The `ServerBrowser.Settings.json` key.
    fn key(self) -> &'static str {
        match self {
            Tab::Favorites => "favorites",
            Tab::Recent => "recent",
            Tab::Official => "official",
            Tab::Friends => "friends",
            Tab::Internet => "internet",
            Tab::Units => "units",
        }
    }

    /// Official and Internet both show the Internet list (official servers only / without them).
    fn list(self) -> ServerListKind {
        match self {
            Tab::Favorites => ServerListKind::Favorites,
            Tab::Recent => ServerListKind::History,
            Tab::Friends => ServerListKind::Friends,
            Tab::Official | Tab::Internet => ServerListKind::Internet,
            Tab::Units => ServerListKind::Units,
        }
    }
}

/// `ServerBrowser.Settings.json`: per tab, the search text, sort and direction.
#[derive(Default, Serialize, Deserialize)]
struct TabSettings {
    #[serde(default)]
    filters: Vec<String>,
    #[serde(default)]
    sort: String,
    #[serde(rename = "sort-reversed", default)]
    sort_reversed: bool,
}

struct Request {
    id: u64,
    kind: ServerListKind,
    stage: usize,
    _handle: Option<ServerListHandle>,
}

/// What the page asks the app to do.
pub enum ServersAction {
    /// Launch and connect; with `setup`, load exactly these mods and DLC first. The flags are
    /// the user's answers to the join prompts.
    Join {
        addr: Ipv4Addr,
        port: u16,
        password: Option<String>,
        setup: Option<Setup>,
        disable_file_patching: bool,
        enable_battleye: bool,
    },
    /// HOST SERVER: start the game with `-setupHost`.
    Host,
    /// Subscribe to these workshop items (missing server mods).
    Subscribe(Vec<u64>),
}

/// The mods (identifiers) and optional DLC (app ids) a server needs.
#[derive(Clone, Debug)]
pub struct Setup {
    pub mods: Vec<String>,
    pub dlcs: Vec<u32>,
}

/// The launcher settings the join prompts depend on.
pub struct JoinSettings {
    pub file_patching: bool,
    pub headless_client: bool,
    pub battleye: bool,
}

/// A join on its way through the prompts that follow the mods dialog
/// (`JoinRequestHandlerAsync`): file patching, BattlEye, then the password.
#[derive(Clone)]
struct PendingJoin {
    addr: Ipv4Addr,
    port: u16,
    setup: Option<Setup>,
    password_required: bool,
    denies_file_patching: bool,
    requires_battleye: bool,
    disable_file_patching: bool,
    enable_battleye: bool,
    /// The prompts already passed.
    step: u8,
}

impl PendingJoin {
    fn new(server: &Server, setup: Option<Setup>, headless_client: bool) -> Self {
        PendingJoin {
            addr: server.addr,
            port: server.port,
            setup,
            password_required: server.password,
            denies_file_patching: server.tags.denies_file_patching(headless_client),
            requires_battleye: server.tags.battleye,
            disable_file_patching: false,
            enable_battleye: false,
            step: 0,
        }
    }

    fn action(self, password: Option<String>) -> ServersAction {
        ServersAction::Join {
            addr: self.addr,
            port: self.port,
            password,
            setup: self.setup,
            disable_file_patching: self.disable_file_patching,
            enable_battleye: self.enable_battleye,
        }
    }
}

#[derive(Clone, Copy)]
enum Prompt {
    FilePatching,
    BattlEye,
}

enum Dialog {
    DirectConnect {
        address: String,
        port: String,
        password: String,
    },
    Password {
        join: PendingJoin,
        password: String,
    },
    Join {
        key: Key,
    },
    /// "Setup mods and join" after subscribing: waits for Steam to install `ids`, then joins
    /// (`WaitScreen`).
    Downloading {
        key: Key,
        ids: Vec<u64>,
    },
    Prompt {
        prompt: Prompt,
        join: PendingJoin,
    },
}

pub struct ServersPage {
    version_rx: Option<Receiver<Option<(u32, u32, u32)>>>,
    tab: Tab,
    lists: HashMap<ServerListKind, HashMap<Key, Server>>,
    request: Option<Request>,
    next_request: u64,
    refreshed_at: HashMap<ServerListKind, f64>,
    rules: HashMap<Key, Option<Rules>>,
    rules_pending: HashSet<Key>,
    filters: HashMap<&'static str, Filters>,
    sort: HashMap<&'static str, (Sort, bool)>,
    filters_open: bool,
    /// The Filters popup's tab: 0 Basic, 1 Advanced.
    filters_tab: usize,
    /// The Filters button, which opens and closes the popup itself.
    filters_toggle: Rect,
    expanded: Option<Key>,
    dialog: Option<Dialog>,
    bi: BiLists,
    bi_rx: Option<Receiver<BiLists>>,
    version: Option<(u32, u32, u32)>,
    launcher_data: PathBuf,
    loaded_settings: bool,
    units: Option<Units>,
    /// Short hashes of local mods by identifier, for matching server mods.
    hashes: HashMap<String, u32>,
    /// Mods hashed or being hashed.
    hashing: HashSet<String>,
    hash_tx: Sender<(String, Option<u32>)>,
    hash_rx: Receiver<(String, Option<u32>)>,
}

impl ServersPage {
    /// Reads the game version (for Steam's version filters) in the background.
    pub fn new(launcher_data: PathBuf, game_dir: PathBuf) -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(servers::exe_version(&game_dir));
        });
        let (hash_tx, hash_rx) = channel();
        Self {
            version_rx: Some(rx),
            tab: Tab::Favorites,
            lists: HashMap::new(),
            request: None,
            next_request: 1,
            refreshed_at: HashMap::new(),
            rules: HashMap::new(),
            rules_pending: HashSet::new(),
            filters: HashMap::new(),
            sort: HashMap::new(),
            filters_open: false,
            filters_tab: 0,
            filters_toggle: Rect::NOTHING,
            expanded: None,
            dialog: None,
            bi: BiLists::default(),
            bi_rx: None,
            version: None,
            launcher_data,
            loaded_settings: false,
            units: None,
            hashes: HashMap::new(),
            hashing: HashSet::new(),
            hash_tx,
            hash_rx,
        }
    }

    /// Hashes installed mods off the UI thread; a mod is hashed again once Steam has updated it.
    fn update_hashes(&mut self, library: &Library, ctx: &egui::Context) {
        while let Ok((id, hash)) = self.hash_rx.try_recv() {
            if let Some(hash) = hash {
                self.hashes.insert(id, hash);
            }
        }
        let mut todo = Vec::new();
        for m in &library.mods {
            if m.launchable() {
                if !self.hashing.contains(&m.id) {
                    self.hashing.insert(m.id.clone());
                    todo.push((m.id.clone(), m.path.clone()));
                }
            } else if matches!(m.status, Status::Busy | Status::InstallationQueued) {
                self.hashing.remove(&m.id);
                self.hashes.remove(&m.id);
            }
        }
        if todo.is_empty() {
            return;
        }
        let (tx, ctx) = (self.hash_tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            for (id, path) in todo {
                let _ = tx.send((id, crate::mods::short_hash(&path)));
            }
            ctx.request_repaint();
        });
    }

    /// The local mod for a server mod (`ModMatching`): by workshop id, then short hash, then
    /// normalised name.
    fn match_mod<'a>(
        &self,
        m: &ServerMod,
        library: Option<&'a Library>,
    ) -> Option<&'a crate::mods::Mod> {
        let library = library?;
        if m.published_id > 0
            && let Some(local) = library.find(&crate::mods::steam_id(m.published_id))
        {
            return Some(local);
        }
        if m.hash != 0
            && let Some(local) = library
                .mods
                .iter()
                .find(|l| self.hashes.get(&l.id) == Some(&m.hash))
        {
            return Some(local);
        }
        let normalize = |s: &str| {
            s.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase()
        };
        let name = normalize(&m.name);
        library.mods.iter().find(|l| normalize(&l.name) == name)
    }

    /// (matched, missing) counts of a server's mods.
    fn mods_status(&self, rules: &Rules, library: Option<&Library>) -> (usize, usize) {
        let mods: Vec<_> = rules.mods.iter().filter(|m| !m.optional_dlc).collect();
        let ok = mods
            .iter()
            .filter(|m| self.match_mod(m, library).is_some())
            .count();
        (ok, mods.len() - ok)
    }

    /// The query stages for a list: the five version stages, or one per unit server.
    fn stages(&self, kind: ServerListKind) -> Vec<Vec<(&'static str, String)>> {
        if kind == ServerListKind::Units {
            servers::unit_stages(&self.units.as_ref().map(Units::servers).unwrap_or_default())
        } else {
            servers::stages(&servers::version_tag(self.version))
        }
    }

    fn load_settings(&mut self) {
        self.loaded_settings = true;
        let Ok(bytes) = std::fs::read(self.launcher_data.join("ServerBrowser.Settings.json"))
        else {
            return;
        };
        let Ok(all) = serde_json::from_slice::<HashMap<String, TabSettings>>(
            bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes),
        ) else {
            return;
        };
        for tab in Tab::ALL {
            if let Some(s) = all.get(tab.key()) {
                let text = s.filters.first().cloned().unwrap_or_default();
                self.filters.insert(
                    tab.key(),
                    Filters {
                        text,
                        ..Default::default()
                    },
                );
                let sort = Sort::ALL
                    .iter()
                    .find(|(_, name)| name.replace(' ', "").eq_ignore_ascii_case(&s.sort))
                    .map_or(Sort::None, |(s, _)| *s);
                self.sort.insert(tab.key(), (sort, s.sort_reversed));
            }
        }
    }

    /// Writes the per-tab search text and sort back to `ServerBrowser.Settings.json`.
    pub fn save_settings(&self) {
        if !self.loaded_settings {
            return;
        }
        let all: HashMap<&str, TabSettings> = Tab::ALL
            .iter()
            .map(|tab| {
                let (sort, reversed) = self.sort.get(tab.key()).copied().unwrap_or_default();
                let name = Sort::ALL
                    .iter()
                    .find(|(s, _)| *s == sort)
                    .map_or("None", |(_, n)| n)
                    .replace(' ', "");
                let text = self
                    .filters
                    .get(tab.key())
                    .map(|f| f.text.clone())
                    .unwrap_or_default();
                (
                    tab.key(),
                    TabSettings {
                        filters: vec![text],
                        sort: name,
                        sort_reversed: reversed,
                    },
                )
            })
            .collect();
        if let Ok(json) = serde_json::to_string(&all) {
            let _ = std::fs::write(self.launcher_data.join("ServerBrowser.Settings.json"), json);
        }
    }

    /// Takes Steam's server events; call every frame.
    pub fn handle(&mut self, event: SteamEvent, steam: &Steam, now: f64) {
        match event {
            SteamEvent::Server { request, server } => {
                if let Some(r) = &self.request
                    && r.id == request
                {
                    self.lists
                        .entry(r.kind)
                        .or_default()
                        .insert(server.key(), server);
                }
            }
            SteamEvent::ServersDone { request } => {
                if let Some(r) = self.request.take_if(|r| r.id == request) {
                    // The next filter stage, until all have run.
                    let stages = self.stages(r.kind);
                    if r.stage + 1 < stages.len() {
                        self.start_stage(steam, r.kind, r.stage + 1, now);
                    }
                }
            }
            SteamEvent::Rules { addr, rules } => {
                self.rules_pending.remove(&addr);
                self.rules.insert(addr, rules);
            }
            _ => {}
        }
    }

    fn start_stage(&mut self, steam: &Steam, kind: ServerListKind, stage: usize, now: f64) {
        let id = self.next_request;
        self.next_request += 1;
        let filters = self.stages(kind).swap_remove(stage);
        // Releasing the previous request (dropping its handle) cancels it.
        self.request = None;
        let handle = steam.request_servers(kind, &filters, id);
        self.request = Some(Request {
            id,
            kind,
            stage,
            _handle: handle,
        });
        self.refreshed_at.insert(kind, now);
    }

    fn refresh(&mut self, steam: &Steam, now: f64, force: bool) {
        let kind = self.tab.list();
        // Vanilla doesn't refetch the Internet list within a minute, others within 10 s.
        let throttle = if kind == ServerListKind::Internet {
            60.0
        } else {
            10.0
        };
        let recent = self
            .refreshed_at
            .get(&kind)
            .is_some_and(|t| now - t < throttle);
        if force || !recent {
            self.lists.insert(kind, HashMap::new());
            if self.stages(kind).is_empty() {
                // No unit servers: nothing to ask Steam.
                self.refreshed_at.insert(kind, now);
            } else {
                self.start_stage(steam, kind, 0, now);
            }
        }
    }

    fn refreshing(&self) -> bool {
        self.request
            .as_ref()
            .is_some_and(|r| r.kind == self.tab.list())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        steam: Option<&Steam>,
        library: Option<&Library>,
        owned_dlc: &dyn Fn(u32) -> bool,
        units: Option<&Units>,
        join: &JoinSettings,
    ) -> Option<ServersAction> {
        let now = ui.input(|i| i.time);
        if !self.loaded_settings {
            self.load_settings();
        }
        if self.units.is_none()
            && let Some(units) = units
        {
            // The Units tab lists nothing until the units are known.
            self.units = Some(units.clone());
            self.refreshed_at.remove(&ServerListKind::Units);
        }
        if let Some(library) = library {
            self.update_hashes(library, ui.ctx());
        }
        if self.bi_rx.is_none() {
            let (tx, rx) = channel();
            let (dir, ctx) = (self.launcher_data.clone(), ui.ctx().clone());
            std::thread::spawn(move || {
                let _ = tx.send(BiLists::load(&dir));
                ctx.request_repaint();
            });
            self.bi_rx = Some(rx);
        }
        if let Some(lists) = self.bi_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.bi = lists;
        }

        // The bottom bar: HOST SERVER and DIRECT CONNECT.
        let bottom = Rect::from_min_max(pos2(page.left(), page.bottom() - 50.0), page.max);
        let main = Rect::from_min_max(page.min, pos2(page.right(), bottom.top()));
        let headers: Vec<&str> = Tab::ALL.iter().map(|t| t.title()).collect();
        let index = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        let (picked, content) = tab_control(ui, "servers-tabs", main, &headers, index);
        let tab_changed = Tab::ALL[picked] != self.tab;
        self.tab = Tab::ALL[picked];
        let Some(steam) = steam else {
            ui.painter().rect_filled(content, 0.0, theme::PANEL);
            ui.painter().text(
                content.center(),
                Align2::CENTER_CENTER,
                "Steam is offline: servers can't be listed.",
                theme::body(theme::HEADER_SIZE),
                theme::ERROR,
            );
            return None;
        };
        if let Some(rx) = &self.version_rx {
            match rx.try_recv() {
                Ok(version) => {
                    self.version = version;
                    self.version_rx = None;
                }
                Err(_) => ui
                    .ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        if self.version_rx.is_none()
            && (tab_changed || !self.refreshed_at.contains_key(&self.tab.list()))
        {
            self.refresh(steam, now, false);
        }

        let mut action = None;
        let (rows, awaiting_rules) = self.visible_rows();
        let bar = Rect::from_min_size(content.min, vec2(content.width(), 40.0));
        self.toolbar(ui, gfx, bar, steam, now, rows.len());
        let mut y = bar.bottom() + 1.0;
        if self.refreshing() {
            stripes(
                ui,
                Rect::from_min_size(pos2(content.left(), y), vec2(content.width(), 5.0)),
                now,
            );
            ui.ctx().request_repaint();
        }
        y += 6.0;
        let header = Rect::from_min_size(
            pos2(content.left(), y),
            vec2(content.width(), HEADER_HEIGHT),
        );
        self.column_header(ui, gfx, header);
        let body = Rect::from_min_max(pos2(content.left(), header.bottom() + 1.0), content.max);
        ui.painter().rect_filled(body, 0.0, theme::PANEL);
        if self.tab == Tab::Units && rows.is_empty() && !self.refreshing() {
            // The tab's EmptyTemplate at opacity 0.7.
            ui.painter().text(
                body.center(),
                Align2::CENTER_CENTER,
                "Servers that belong to your unit(s) will be displayed here.",
                theme::body(theme::BODY_SIZE),
                theme::WHITE.gamma_multiply(0.7),
            );
        }

        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        let heights: Vec<f32> = rows
            .iter()
            .map(|k| {
                1.0 + ROW_HEIGHT
                    + if self.expanded == Some(*k) {
                        DETAIL_HEIGHT
                    } else {
                        0.0
                    }
            })
            .collect();
        let total: f32 = heights.iter().sum();
        let mut wanted_rules = Vec::new();
        let mut row_actions = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt(("server-list", self.tab.key()))
            .auto_shrink([false, false])
            .show_viewport(&mut body_ui, |ui, viewport| {
                ui.set_height(total);
                let origin = ui.min_rect().min;
                let mut top = 0.0;
                for (key, height) in rows.iter().zip(&heights) {
                    if top + height >= viewport.min.y && top <= viewport.max.y {
                        let rect = Rect::from_min_size(
                            origin + vec2(0.0, top + 1.0),
                            vec2(body.width(), height - 1.0),
                        );
                        if !self.rules.contains_key(key) {
                            wanted_rules.push(*key);
                        }
                        if let Some(a) = self.row(ui, gfx, rect, *key, library, owned_dlc) {
                            row_actions.push(a);
                        }
                    }
                    top += height;
                }
            });
        // Rules for the rows on screen, then for the servers an Advanced filter waits on, a few
        // at a time; more while a filter waits on them.
        let in_flight = if awaiting_rules.is_empty() {
            RULES_IN_FLIGHT
        } else {
            FILTER_RULES_IN_FLIGHT
        };
        for key in wanted_rules.into_iter().chain(awaiting_rules) {
            if self.rules_pending.len() >= in_flight {
                break;
            }
            if self.rules_pending.insert(key) {
                steam.query_rules(key.0, key.1);
            }
        }
        for a in row_actions {
            match a {
                RowAction::Toggle(key) => {
                    self.expanded = if self.expanded == Some(key) {
                        None
                    } else {
                        Some(key)
                    }
                }
                RowAction::Join(key) => {
                    if !self.rules.contains_key(&key) && self.rules_pending.insert(key) {
                        steam.query_rules(key.0, key.1);
                    }
                    self.dialog = Some(Dialog::Join { key });
                }
                RowAction::Favorite(key, on) => {
                    if let Some(s) = self.server(key) {
                        steam.set_favorite(s.addr, s.port, s.query_port, on);
                    }
                    let known = self.server(key).cloned();
                    let favorites = self.lists.entry(ServerListKind::Favorites).or_default();
                    match (on, known) {
                        (true, Some(server)) => {
                            favorites.insert(key, server);
                        }
                        (false, _) => {
                            favorites.remove(&key);
                        }
                        _ => {}
                    }
                }
            }
        }

        // Bottom bar.
        ui.painter().rect_filled(bottom, 0.0, theme::STRONG_PANEL);
        let mut right = bottom.right() - 10.0;
        for label in ["DIRECT CONNECT", "HOST SERVER"] {
            let w = flat_button_width(ui, label);
            let b = Rect::from_min_max(
                pos2(right - w, bottom.top() + 10.0),
                pos2(right, bottom.bottom() - 10.0),
            );
            right -= w + 10.0;
            if flat_button(ui, Id::unique(("servers-bottom", label)), b, label).clicked() {
                if label == "DIRECT CONNECT" {
                    self.dialog = Some(Dialog::DirectConnect {
                        address: String::new(),
                        port: "2302".into(),
                        password: String::new(),
                    });
                } else {
                    // HOST SERVER starts the game with -setupHost (`ServersViewModel`).
                    action = Some(ServersAction::Host);
                }
            }
        }

        if self.filters_open {
            self.filters_popup(
                ui,
                gfx,
                Rect::from_min_size(
                    pos2(bar.left(), bar.bottom()),
                    vec2(bar.width().max(600.0), 270.0),
                ),
            );
        }
        if let Some(a) = self.dialogs(ui, gfx, page, library, join) {
            action = Some(a);
        }
        action
    }

    fn server(&self, key: Key) -> Option<&Server> {
        self.lists.values().find_map(|l| l.get(&key))
    }

    /// The rows to show, sorted, and the servers whose rules an Advanced filter still waits on
    /// (hidden until they answer; a server that doesn't answer has vanilla's all-zero rules).
    fn visible_rows(&self) -> (Vec<Key>, Vec<Key>) {
        let empty = HashMap::new();
        let list = self.lists.get(&self.tab.list()).unwrap_or(&empty);
        let filters = self
            .filters
            .get(self.tab.key())
            .cloned()
            .unwrap_or_default();
        let mut rows: Vec<&Server> = list
            .values()
            .filter(|s| match self.tab {
                Tab::Official => self.bi.official.contains(&s.addr),
                Tab::Internet => !self.bi.official.contains(&s.addr),
                _ => true,
            })
            .filter(|s| filters.matches(s, &self.bi))
            .collect();
        let mut awaiting = Vec::new();
        if filters.needs_rules() {
            rows.retain(|s| match self.rules.get(&s.key()) {
                Some(Some(rules)) => filters.matches_rules(rules),
                Some(None) => filters.matches_rules(&Rules::default()),
                None => {
                    awaiting.push(s.key());
                    false
                }
            });
        }
        let (sort, reversed) = self.sort.get(self.tab.key()).copied().unwrap_or_default();
        sort.sort(&mut rows, reversed, &self.bi);
        (rows.into_iter().map(Server::key).collect(), awaiting)
    }

    fn toolbar(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        bar: Rect,
        steam: &Steam,
        now: f64,
        shown: usize,
    ) {
        toolbar(ui, bar);
        // Right: refresh / cancel, then "Servers: n / m".
        let refresh =
            Rect::from_min_size(pos2(bar.right() - 44.0, bar.top() + 4.0), vec2(40.0, 32.0));
        let r = widgets::overlay_button(ui, Id::unique("servers-refresh"), refresh);
        if self.refreshing() {
            gfx.paint_icon(
                ui,
                super::icons::CANCEL,
                refresh,
                vec2(16.0, 16.0),
                theme::WHITE,
            );
            if r.clicked() {
                self.request = None;
            }
        } else {
            gfx.paint_icon(
                ui,
                super::icons::REFRESH,
                refresh,
                vec2(16.0, 16.0),
                theme::WHITE,
            );
            if r.clicked() {
                self.refresh(steam, now, true);
            }
        }
        let total = self.lists.get(&self.tab.list()).map_or(0, HashMap::len);
        let filtering = shown != total;
        let mut job = egui::text::LayoutJob::default();
        let font = theme::body(theme::BODY_SIZE);
        job.append(
            "Servers: ",
            0.0,
            egui::TextFormat::simple(font.clone(), theme::HINT_TEXT),
        );
        job.append(
            &shown.to_string(),
            0.0,
            egui::TextFormat::simple(
                font.clone(),
                if filtering {
                    theme::ACCENT
                } else {
                    theme::WHITE
                },
            ),
        );
        job.append(
            " / ",
            0.0,
            egui::TextFormat::simple(font.clone(), theme::HINT_TEXT),
        );
        job.append(
            &total.to_string(),
            0.0,
            egui::TextFormat::simple(font.clone(), theme::WHITE),
        );
        let galley = ui.painter().layout_job(job);
        let count_left = refresh.left() - 10.0 - galley.size().x.max(100.0);
        ui.painter().galley(
            pos2(count_left, bar.center().y - galley.size().y / 2.0),
            galley,
            theme::WHITE,
        );

        // Fill: search box and the Filters toggle.
        let block = Rect::from_min_max(
            pos2(bar.left() + 2.0, bar.top() + 2.0),
            pos2(count_left - 10.0, bar.bottom() - 2.0),
        );
        let toggle_width = 70.0;
        let field = Rect::from_min_max(
            block.min,
            pos2(block.right() - toggle_width, block.bottom()),
        );
        let filters = self.filters.entry(self.tab.key()).or_default();
        let active = filters.is_active();
        widgets::filter_box(
            ui,
            gfx,
            Id::unique(("servers-search", self.tab.key())),
            field,
            &mut filters.text,
            "Search servers (Ctrl+F)",
        );
        let toggle = Rect::from_min_max(pos2(field.right(), block.top()), block.max);
        self.filters_toggle = toggle;
        let response = ui.interact(toggle, Id::unique("servers-filters-toggle"), Sense::click());
        let (fill, ink) = if self.filters_open {
            (theme::SECONDARY_LABEL, theme::BLACK)
        } else {
            (theme::argb(0xCC000000), theme::WHITE)
        };
        ui.painter().rect_filled(toggle, 0.0, fill);
        ui.painter().rect_stroke(
            toggle,
            0.0,
            (
                1.0,
                if active {
                    theme::ACCENT
                } else {
                    theme::argb(0xFF333333)
                },
            ),
            egui::StrokeKind::Inside,
        );
        ui.painter()
            .text(toggle.center(), Align2::CENTER_CENTER, "Filters", font, ink);
        if response.clicked() {
            self.filters_open = !self.filters_open;
        }
    }

    /// Column header (30 px): Name fills; Type 100, Mission 200, Players 80, Ping 80 and the
    /// sort menu at the right.
    fn column_header(&mut self, ui: &mut egui::Ui, gfx: &mut Gfx, rect: Rect) {
        ui.painter().rect_filled(rect, 0.0, theme::STRONG_PANEL);
        let font = theme::body(theme::BODY_SIZE);
        let color = theme::SECONDARY_LABEL;
        let cols = columns(rect);
        ui.painter().text(
            pos2(cols.name.left(), rect.center().y),
            Align2::LEFT_CENTER,
            "Name",
            font.clone(),
            color,
        );
        if cols.kind.is_positive() {
            ui.painter().text(
                pos2(cols.kind.left(), rect.center().y),
                Align2::LEFT_CENTER,
                "Type",
                font.clone(),
                color,
            );
            ui.painter().text(
                pos2(cols.mission.left(), rect.center().y),
                Align2::LEFT_CENTER,
                "Mission",
                font.clone(),
                color,
            );
            ui.painter().text(
                pos2(cols.players.left(), rect.center().y),
                Align2::LEFT_CENTER,
                "Players",
                font.clone(),
                color,
            );
        }
        ui.painter().text(
            pos2(cols.ping.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            "Ping",
            font.clone(),
            color,
        );
        let sort_rect = Rect::from_min_max(pos2(rect.right() - 175.0, rect.top()), rect.max);
        let response = ui.interact(sort_rect, Id::unique("servers-sort"), Sense::click());
        let (sort, reversed) = self.sort.get(self.tab.key()).copied().unwrap_or_default();
        let label = Sort::ALL
            .iter()
            .find(|(s, _)| *s == sort)
            .map_or("None", |(_, n)| n);
        let sort_color = if response.hovered() {
            theme::ACCENT
        } else {
            color
        };
        let icon = if response.hovered() {
            "sort_hover.png"
        } else {
            "sort.png"
        };
        if let Some(texture) = gfx.bitmap(ui.ctx(), icon) {
            let r = Rect::from_min_size(
                pos2(sort_rect.right() - 28.0, rect.center().y - 9.0),
                vec2(18.0, 18.0),
            );
            ui.painter()
                .image(texture.id(), snap(ui, r), FULL_UV, Color32::WHITE);
        }
        ui.painter().text(
            pos2(sort_rect.right() - 34.0, rect.center().y),
            Align2::RIGHT_CENTER,
            format!("Sort: {label}{}", if reversed { " \u{2191}" } else { "" }),
            font,
            sort_color,
        );
        let mut picked = None;
        egui::Popup::menu(&response)
            .frame(menu::frame())
            .width(200.0)
            .show(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                for (mode, name) in Sort::ALL {
                    if menu::item(ui, name, true) {
                        picked = Some(mode);
                    }
                }
            });
        if let Some(mode) = picked {
            let reversed = if mode == sort { !reversed } else { false };
            self.sort.insert(self.tab.key(), (mode, reversed));
        }
    }

    /// One server row (`FullRowCustomControl`) and its details when expanded.
    fn row(
        &self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        rect: Rect,
        key: Key,
        library: Option<&Library>,
        owned_dlc: &dyn Fn(u32) -> bool,
    ) -> Option<RowAction> {
        let server = self.server(key)?;
        let mut action = None;
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), ROW_HEIGHT));
        let response = ui.interact(header, Id::unique(("server-row", key)), Sense::click());
        // Also while the pointer is on the JOIN button drawn over the row; never under a dialog.
        let hovered = response.contains_pointer() && self.dialog.is_none();
        ui.painter().rect_filled(
            rect,
            0.0,
            if hovered || self.expanded == Some(key) {
                theme::argb(0xFF333333)
            } else {
                theme::argb(0x99000000)
            },
        );
        let y = header.center().y;
        let cols = columns(header);
        let font = theme::body(theme::BODY_SIZE);

        // Left icons: favourite (the unit's logo on the Units tab), password, BattlEye, server
        // state.
        let unit = self
            .units
            .as_ref()
            .and_then(|u| u.unit_for(server.addr, server.port));
        let fav_rect = Rect::from_min_size(
            pos2(header.left() + 1.0, header.top()),
            vec2(32.0, ROW_HEIGHT),
        );
        if self.tab == Tab::Units {
            if let Some(texture) = unit
                .and_then(|u| u.logo_url.as_deref())
                .and_then(|url| gfx.remote(ui.ctx(), url))
            {
                let size = texture.size_vec2();
                let scale = (32.0 / size.x).min(32.0 / size.y);
                ui.painter().image(
                    texture.id(),
                    snap(ui, Rect::from_center_size(fav_rect.center(), size * scale)),
                    FULL_UV,
                    Color32::WHITE,
                );
            }
        } else {
            let favorite = self
                .lists
                .get(&ServerListKind::Favorites)
                .is_some_and(|l| l.contains_key(&key));
            let fav = ui.interact(fav_rect, Id::unique(("server-fav", key)), Sense::click());
            let fav_icon = if fav.hovered() {
                "favorite_hover.png"
            } else {
                "favorite.png"
            };
            if fav.hovered() {
                ui.painter()
                    .rect_filled(fav_rect, 0.0, theme::argb(0x44000000));
            }
            icon_at(
                ui,
                gfx,
                fav_icon,
                fav_rect.center(),
                if favorite || fav.hovered() { 1.0 } else { 0.2 },
            );
            if fav.clicked() {
                action = Some(RowAction::Favorite(key, !favorite));
            }
        }
        icon_at(
            ui,
            gfx,
            "password.png",
            pos2(header.left() + 33.0 + 16.0, y),
            if server.password { 1.0 } else { 0.2 },
        );
        icon_at(
            ui,
            gfx,
            "battleye.png",
            pos2(header.left() + 65.0 + 16.0, y),
            if server.tags.battleye { 1.0 } else { 0.2 },
        );
        let state_icon = match self.rules.get(&key) {
            _ if servers::version_mismatch(&server.tags, self.version) => "ss_exemismatch.png",
            Some(Some(rules)) => {
                if self.mods_status(rules, library).1 == 0 {
                    "ss_modmatch.png"
                } else {
                    "ss_modmismatch.png"
                }
            }
            Some(None) => "ss_unknown.png",
            None => "ss_indeterminate.png",
        };
        icon_at(
            ui,
            gfx,
            state_icon,
            pos2(header.left() + 97.0 + 16.0, y),
            1.0,
        );

        // Returns where the text ends.
        let clipped = |ui: &egui::Ui, text: &str, rect: Rect, color: Color32| {
            let mut job = egui::text::LayoutJob::simple(
                text.to_owned(),
                theme::body(theme::BODY_SIZE),
                color,
                rect.width() - 6.0,
            );
            job.wrap.max_rows = 1;
            job.wrap.break_anywhere = true;
            let g = ui.painter().layout_job(job);
            let right = rect.left() + g.size().x;
            ui.painter()
                .galley(pos2(rect.left(), y - g.size().y / 2.0), g, color);
            right
        };
        let mut name_rect = cols.name;
        if unit.is_some() {
            // Room for the units icon after the name (margin 10).
            name_rect.max.x -= 34.0;
        }
        let name_end = clipped(ui, server.display_name(), name_rect, theme::WHITE);
        if unit.is_some()
            && let Some(texture) = gfx.bitmap(ui.ctx(), "units_icon.png")
        {
            let size = texture.size_vec2().min(vec2(24.0, 24.0));
            let icon = Rect::from_min_size(pos2(name_end + 10.0, y - size.y / 2.0), size);
            ui.painter()
                .image(texture.id(), snap(ui, icon), FULL_UV, Color32::WHITE);
            ui.interact(icon, Id::unique(("server-unit", key)), Sense::hover())
                .on_hover_text(
                    "This servers is listed as server associated with one of your units.",
                );
        }
        if cols.kind.is_positive() {
            clipped(ui, &server.game_type(), cols.kind, theme::WHITE);
            clipped(ui, &server.mission, cols.mission, theme::WHITE);
            let mut job = egui::text::LayoutJob::default();
            job.append(
                &server.players.to_string(),
                0.0,
                egui::TextFormat::simple(font.clone(), theme::WHITE),
            );
            job.append(
                &format!("/{}", server.max_players),
                0.0,
                egui::TextFormat::simple(font.clone(), theme::SECONDARY_LABEL),
            );
            let g = ui.painter().layout_job(job);
            ui.painter().galley(
                pos2(cols.players.left(), y - g.size().y / 2.0),
                g,
                theme::WHITE,
            );
        }
        ui.painter().text(
            pos2(cols.ping.right() - 10.0, y),
            Align2::RIGHT_CENTER,
            format!("{} ms", server.ping),
            font.clone(),
            theme::argb(servers::ping_color(server.ping)),
        );

        // Join area (on hover) and the expand arrow.
        if hovered {
            let join = Rect::from_min_size(
                pos2(cols.join.left() + 5.0, header.top() + 6.0),
                vec2(120.0, ROW_HEIGHT - 12.0),
            );
            let locked = server.tags.locked;
            let j = ui.interact(join, Id::unique(("server-join", key)), Sense::click());
            let fill = if locked {
                theme::argb(0xFF800000)
            } else if j.hovered() {
                theme::ACCENT_HOVER
            } else {
                theme::ACCENT
            };
            ui.painter().rect_filled(join, 0.0, fill);
            ui.painter().text(
                join.center(),
                Align2::CENTER_CENTER,
                if locked { "LOCKED" } else { "JOIN" },
                theme::heading(17.0),
                theme::WHITE,
            );
            if j.clicked() && !locked {
                action = Some(RowAction::Join(key));
            }
        }
        chevron(
            ui,
            gfx,
            pos2(header.right() - 20.0 - 6.0, y),
            self.expanded == Some(key),
            theme::argb(0xFFF5F5F5),
        );
        if response.clicked() && action.is_none() {
            action = Some(RowAction::Toggle(key));
        }

        if self.expanded == Some(key) {
            let detail = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
            self.details(ui, detail, server, library, owned_dlc);
        }
        action
    }

    /// `ServerDetailCustomControl`: a 180 px label column and the values.
    fn details(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        server: &Server,
        library: Option<&Library>,
        owned_dlc: &dyn Fn(u32) -> bool,
    ) {
        let rules = self.rules.get(&server.key()).cloned().flatten();
        let named = |v: u8, names: [&str; 3]| {
            names
                .get(v as usize)
                .copied()
                .unwrap_or("Custom")
                .to_owned()
        };
        let yes_no = |b: bool| if b { "Enabled" } else { "Disabled" }.to_owned();
        let mut rows: Vec<(&str, String, Color32)> = vec![
            ("Map:", server.map.clone(), theme::WHITE),
            (
                "Server state:",
                servers::SESSION_STATES
                    .get(server.tags.session_state as usize)
                    .unwrap_or(&"None")
                    .to_string(),
                theme::WHITE,
            ),
        ];
        if let Some(r) = &rules {
            rows.push((
                "Difficulty:",
                named(r.difficulty, ["Recruit", "Regular", "Veteran"]),
                theme::WHITE,
            ));
            rows.push((
                "AI level:",
                named(r.ai_level, ["Novice", "Normal", "Expert"]),
                theme::WHITE,
            ));
            rows.push(("3rd person view:", yes_no(r.third_person), theme::WHITE));
            let (ok, missing) = self.mods_status(r, library);
            let optional_missing = r
                .mods
                .iter()
                .filter(|m| m.optional_dlc && !owned_dlc(m.published_id as u32))
                .count();
            let color = if missing > 0 {
                theme::ERROR
            } else {
                theme::SECONDARY_GREEN
            };
            rows.push((
                "Mods:",
                format!(
                    "{ok} ok / {missing} missing{}",
                    if optional_missing > 0 {
                        format!(", {optional_missing} DLC not owned")
                    } else {
                        String::new()
                    }
                ),
                color,
            ));
        } else {
            rows.push(("Mods:", "Querying the server...".into(), theme::HINT_TEXT));
        }
        rows.push((
            "Server type:",
            if server.tags.dedicated {
                "Dedicated"
            } else {
                "Player hosted"
            }
            .into(),
            theme::WHITE,
        ));
        rows.push((
            "IP address:",
            format!("{}:{}", server.addr, server.port),
            theme::WHITE,
        ));
        let ours = self
            .version
            .map(|(a, b, c)| format!("{a}.{b:02}.{c}"))
            .unwrap_or_default();
        let theirs = format!(
            "{}.{:02}.{}",
            server.tags.version / 100,
            server.tags.version % 100,
            server.tags.build
        );
        let mismatch = servers::version_mismatch(&server.tags, self.version);
        rows.push((
            "Game version:",
            format!("{theirs} (yours: {ours})"),
            if mismatch { theme::ERROR } else { theme::WHITE },
        ));
        let mut y = rect.top() + 5.0;
        for (key, value, color) in rows {
            ui.painter().text(
                pos2(rect.left() + 180.0, y),
                Align2::RIGHT_TOP,
                key,
                theme::body(theme::BODY_SIZE),
                theme::DIM_NAME,
            );
            ui.painter().text(
                pos2(rect.left() + 190.0, y),
                Align2::LEFT_TOP,
                value,
                theme::body(theme::BODY_SIZE),
                color,
            );
            y += 20.0;
        }
    }

    /// The Filters popup (`FilterControl`): Basic and Advanced tabs.
    fn filters_popup(&mut self, ui: &mut egui::Ui, gfx: &mut Gfx, rect: Rect) {
        // Middle: above the page, below the combo box lists (Foreground).
        let layer = egui::LayerId::new(egui::Order::Middle, Id::unique("servers-filters"));
        let mut popup = ui.new_child(UiBuilder::new().layer_id(layer).max_rect(rect));
        popup.interact(
            rect,
            Id::unique("servers-filters-bg"),
            Sense::click_and_drag(),
        );
        popup.painter().rect_filled(rect, 0.0, theme::BLACK);
        popup.painter().rect_stroke(
            rect,
            0.0,
            (1.0, theme::argb(0xFF333333)),
            egui::StrokeKind::Inside,
        );
        let ui = &mut popup;
        let (tab, content) = tab_control(
            ui,
            "servers-filter-tabs",
            rect.shrink(10.0),
            &["Basic", "Advanced"],
            self.filters_tab,
        );
        self.filters_tab = tab;
        let f = self.filters.entry(self.tab.key()).or_default();
        let font = theme::body(theme::BODY_SIZE);
        let label_right = content.left() + 150.0;
        let label = |ui: &egui::Ui, y: f32, text: &str| {
            ui.painter().text(
                pos2(label_right, y + 13.0),
                Align2::RIGHT_CENTER,
                text,
                theme::body(theme::BODY_SIZE),
                theme::WHITE,
            );
        };
        let mut y = content.top() + 8.0;
        if tab == 1 {
            let levels = |names: [&str; 4]| -> Vec<(String, String)> {
                std::iter::once(("".to_owned(), "(Any)".to_owned()))
                    .chain(
                        names
                            .iter()
                            .enumerate()
                            .map(|(i, n)| (i.to_string(), (*n).into())),
                    )
                    .collect()
            };
            let combos: [(&str, &mut Option<u8>, [&str; 4]); 2] = [
                (
                    "Difficulty:",
                    &mut f.difficulty,
                    ["Recruit", "Regular", "Veteran", "Custom"],
                ),
                (
                    "AI Level:",
                    &mut f.ai_level,
                    ["Novice", "Normal", "Expert", "Custom"],
                ),
            ];
            for (text, value, names) in combos {
                label(ui, y, text);
                let current = value.map(|v| v.to_string()).unwrap_or_default();
                if let Some(picked) = combo(
                    ui,
                    gfx,
                    Id::unique(("servers-filter-level", text)),
                    Rect::from_min_size(pos2(label_right + 8.0, y), vec2(220.0, 26.0)),
                    &current,
                    &levels(names),
                ) {
                    *value = picked.parse().ok();
                }
                y += 32.0;
            }
            let groups: [(&str, &mut Option<bool>, [&str; 2]); 3] = [
                (
                    "Flight Model:",
                    &mut f.advanced_flight_model,
                    ["Standard", "Advanced"],
                ),
                (
                    "3rd Person View:",
                    &mut f.third_person,
                    ["Enabled", "Disabled"],
                ),
                (
                    "Weapon Crosshair:",
                    &mut f.crosshair,
                    ["Enabled", "Disabled"],
                ),
            ];
            for (text, value, [first, second]) in groups {
                label(ui, y, text);
                // Standard flight model and the "Disabled" options are `false`.
                let (first_value, second_value) = if text == "Flight Model:" {
                    (false, true)
                } else {
                    (true, false)
                };
                if let Some(picked) = toggle_group(
                    ui,
                    text,
                    pos2(label_right + 8.0, y),
                    &[
                        ("Any", None),
                        (first, Some(first_value)),
                        (second, Some(second_value)),
                    ],
                    *value,
                ) {
                    *value = picked;
                }
                y += 32.0;
            }
        } else {
            let texts: [(&str, &mut String); 4] = [
                ("Game type:", &mut f.game_type),
                ("Server name:", &mut f.server_name),
                ("Map name:", &mut f.map),
                ("Mission name:", &mut f.mission),
            ];
            for (text, value) in texts {
                label(ui, y, text);
                text_box(
                    ui,
                    Id::unique(("servers-filter", text)),
                    Rect::from_min_size(pos2(label_right + 8.0, y), vec2(220.0, 26.0)),
                    value,
                );
                y += 32.0;
            }
            label(ui, y, "BattlEye:");
            if let Some(picked) = toggle_group(
                ui,
                "BattlEye:",
                pos2(label_right + 8.0, y),
                &[("Any", None), ("Yes", Some(true)), ("No", Some(false))],
                f.battleye,
            ) {
                f.battleye = picked;
            }
            y += 32.0;
            label(ui, y, "Max ping:");
            let mut ping = f.max_ping.map(|p| p.to_string()).unwrap_or_default();
            if text_box(
                ui,
                Id::unique("servers-ping"),
                Rect::from_min_size(pos2(label_right + 8.0, y), vec2(80.0, 26.0)),
                &mut ping,
            ) {
                f.max_ping = ping.trim().parse().ok();
            }
            // Checkboxes in a second column.
            let mut cy = content.top() + 8.0;
            let cx = content.left() + 410.0;
            let checks: [(&str, &mut bool); 5] = [
                ("Official only", &mut f.official_only),
                ("No password", &mut f.no_password),
                ("Hide full", &mut f.hide_full),
                ("Hide empty", &mut f.hide_empty),
                ("Hide monetized", &mut f.hide_monetized),
            ];
            for (text, value) in checks {
                if checkbox16(
                    ui,
                    gfx,
                    Id::unique(("servers-check", text)),
                    pos2(cx + 8.0, cy + 13.0),
                    *value,
                )
                .clicked()
                {
                    *value = !*value;
                }
                ui.painter().text(
                    pos2(cx + 24.0, cy + 13.0),
                    Align2::LEFT_CENTER,
                    text,
                    font.clone(),
                    theme::WHITE,
                );
                cy += 32.0;
            }
        }
        // A click outside closes the popup, unless it lands in an open combo box list.
        let clicked_outside = ui.input(|i| i.pointer.primary_clicked())
            && ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| !rect.contains(p) && !self.filters_toggle.contains(p))
            && !egui::Popup::is_any_open(ui.ctx());
        if clicked_outside {
            self.filters_open = false;
        }
    }

    /// Direct Connect, the join dialog, its download wait, the join prompts and the password.
    fn dialogs(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        library: Option<&Library>,
        settings: &JoinSettings,
    ) -> Option<ServersAction> {
        let dialog = self.dialog.take()?;
        let win = ui.max_rect();
        // Prompts are message boxes, which bring their own overlay.
        if !matches!(dialog, Dialog::Prompt { .. }) {
            ui.interact(
                win,
                Id::unique("servers-dialog-overlay"),
                Sense::click_and_drag(),
            );
            ui.painter().rect_filled(win, 0.0, theme::STRONG_PANEL);
        }
        let font = theme::body(theme::BODY_SIZE);
        let frame = |ui: &egui::Ui, size: egui::Vec2, title: &str| {
            let rect = Rect::from_center_size(page.center(), size);
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
                title,
                theme::body(theme::HEADER_SIZE),
                theme::WHITE,
            );
            rect
        };
        let footer = |ui: &mut egui::Ui, rect: Rect, labels: &[&str]| -> Option<usize> {
            let mut right = rect.right() - 10.0;
            let mut clicked = None;
            for (i, label) in labels.iter().enumerate().rev() {
                let w = flat_button_width(ui, label).max(90.0);
                let b = Rect::from_min_max(
                    pos2(right - w, rect.bottom() - 40.0),
                    pos2(right, rect.bottom() - 10.0),
                );
                right -= w + 10.0;
                if i == 0 {
                    ui.painter().rect_filled(b, 0.0, theme::ACCENT);
                    let r = widgets::overlay_button(ui, Id::unique(("dialog-button", *label)), b);
                    ui.painter().text(
                        b.center(),
                        Align2::CENTER_CENTER,
                        *label,
                        theme::body(theme::BODY_SIZE),
                        theme::WHITE,
                    );
                    if r.clicked() {
                        clicked = Some(i);
                    }
                } else if flat_button(ui, Id::unique(("dialog-button", *label)), b, label).clicked()
                {
                    clicked = Some(i);
                }
            }
            clicked
        };
        match dialog {
            Dialog::DirectConnect {
                mut address,
                mut port,
                mut password,
            } => {
                let rect = frame(ui, vec2(400.0, 260.0), "Direct connect");
                let mut y = rect.top() + 46.0;
                for (label, value) in [
                    ("Server name or address:", &mut address),
                    ("Password:", &mut password),
                    ("Port:", &mut port),
                ] {
                    ui.painter().text(
                        pos2(rect.left() + 15.0, y),
                        Align2::LEFT_TOP,
                        label,
                        font.clone(),
                        theme::WHITE,
                    );
                    text_box(
                        ui,
                        Id::unique(("direct", label)),
                        Rect::from_min_size(
                            pos2(rect.left() + 15.0, y + 20.0),
                            vec2(rect.width() - 30.0, 26.0),
                        ),
                        value,
                    );
                    y += 54.0;
                }
                match footer(ui, rect, &["Connect", "Cancel"]) {
                    Some(0) => {
                        let host = address.trim();
                        let (host, inline_port) = host
                            .rsplit_once(':')
                            .map_or((host, None), |(h, p)| (h, p.parse::<u16>().ok()));
                        let port = inline_port
                            .or_else(|| port.trim().parse().ok())
                            .unwrap_or(2302);
                        let resolved = host.parse::<Ipv4Addr>().ok().or_else(|| {
                            use std::net::ToSocketAddrs;
                            (host, port)
                                .to_socket_addrs()
                                .ok()?
                                .find_map(|a| match a.ip() {
                                    std::net::IpAddr::V4(v4) => Some(v4),
                                    _ => None,
                                })
                        });
                        if let Some(addr) = resolved {
                            return Some(ServersAction::Join {
                                addr,
                                port,
                                password: Some(password).filter(|p| !p.is_empty()),
                                setup: None,
                                disable_file_patching: false,
                                enable_battleye: false,
                            });
                        }
                        self.dialog = Some(Dialog::DirectConnect {
                            address,
                            port: port.to_string(),
                            password,
                        });
                    }
                    Some(_) => {}
                    None => {
                        self.dialog = Some(Dialog::DirectConnect {
                            address,
                            port,
                            password,
                        })
                    }
                }
            }
            Dialog::Password { join, mut password } => {
                let rect = frame(ui, vec2(300.0, 170.0), "Password");
                ui.painter().text(
                    pos2(rect.left() + 15.0, rect.top() + 46.0),
                    Align2::LEFT_TOP,
                    "This server requires a password to join.",
                    font.clone(),
                    theme::WHITE,
                );
                text_box(
                    ui,
                    Id::unique("join-password"),
                    Rect::from_min_size(
                        pos2(rect.left() + 15.0, rect.top() + 70.0),
                        vec2(rect.width() - 30.0, 26.0),
                    ),
                    &mut password,
                );
                match footer(ui, rect, &["Connect", "Cancel"]) {
                    Some(0) => return Some(join.action(Some(password))),
                    Some(_) => {}
                    None => self.dialog = Some(Dialog::Password { join, password }),
                }
            }
            Dialog::Prompt { prompt, mut join } => {
                // `SBFilePatchingMismatch_PatchingNotAllowed` and `SBBattlEyeRequired`.
                let (caption, body, yes) = match prompt {
                    Prompt::FilePatching => (
                        "File patching not allowed",
                        "The server you want to join does not allow file patching.",
                        "Disable file patching and join",
                    ),
                    Prompt::BattlEye => (
                        "BattlEye Anti-Cheat Required",
                        "The server you want to join requires BattlEye to be enabled. Do you want to enable it automatically?",
                        "Enable BattlEye and join",
                    ),
                };
                let message = menu::MessageBox {
                    caption: caption.into(),
                    body: body.into(),
                    buttons: vec![yes.into(), "Cancel".into()],
                    confirm: None,
                };
                match message.show(ui, win) {
                    Some(0) => {
                        match prompt {
                            Prompt::FilePatching => join.disable_file_patching = true,
                            Prompt::BattlEye => join.enable_battleye = true,
                        }
                        return self.continue_join(join, settings);
                    }
                    Some(_) => {}
                    None => self.dialog = Some(Dialog::Prompt { prompt, join }),
                }
            }
            Dialog::Join { key } => {
                let server = self.server(key)?.clone();
                let rect = frame(
                    ui,
                    vec2(page.width().min(640.0), page.height().min(480.0)),
                    "Join server",
                );
                let mut y = rect.top() + 46.0;
                ui.painter().text(
                    pos2(rect.left() + 15.0, y),
                    Align2::LEFT_TOP,
                    server.display_name(),
                    theme::heading(17.0),
                    theme::ACCENT,
                );
                y += 28.0;
                let rules = self.rules.get(&key).cloned();
                let list_rect = Rect::from_min_max(
                    pos2(rect.left() + 15.0, y),
                    pos2(rect.right() - 15.0, rect.bottom() - 54.0),
                );
                ui.painter().rect_filled(list_rect, 0.0, theme::BLACK);
                // Workshop mods to subscribe to, and subscribed ones Steam hasn't installed yet.
                let mut missing_workshop = Vec::new();
                let mut not_installed = Vec::new();
                match &rules {
                    None => {
                        ui.painter().text(
                            list_rect.center(),
                            Align2::CENTER_CENTER,
                            "Querying the server's mods...",
                            font.clone(),
                            theme::HINT_TEXT,
                        );
                        ui.ctx().request_repaint();
                    }
                    Some(None) => {
                        ui.painter().text(
                            list_rect.center(),
                            Align2::CENTER_CENTER,
                            "The server didn't send its mod list.",
                            font.clone(),
                            theme::HINT_TEXT,
                        );
                    }
                    Some(Some(r)) => {
                        let mut list_ui =
                            ui.new_child(UiBuilder::new().max_rect(list_rect.shrink(6.0)));
                        list_ui.set_clip_rect(list_rect);
                        egui::ScrollArea::vertical().id_salt("join-mods").show(
                            &mut list_ui,
                            |ui| {
                                for m in r.mods.iter().filter(|m| !m.optional_dlc) {
                                    let local = self.match_mod(m, library);
                                    let (status, color) = match (local, m.published_id) {
                                        (Some(l), _) if l.launchable() => {
                                            ("Installed", theme::SECONDARY_GREEN)
                                        }
                                        (Some(l), _) if l.workshop_id().is_some() => {
                                            not_installed.extend(l.workshop_id());
                                            (l.status.label(), theme::ACCENT)
                                        }
                                        (Some(l), _) => (l.status.label(), theme::ERROR),
                                        (None, id) if id > 0 => {
                                            missing_workshop.push(id);
                                            ("Subscribe on the Steam Workshop", theme::ACCENT)
                                        }
                                        (None, _) => ("Not found", theme::ERROR),
                                    };
                                    let (row, _) = ui.allocate_exact_size(
                                        vec2(list_rect.width() - 12.0, 22.0),
                                        Sense::hover(),
                                    );
                                    ui.painter().text(
                                        pos2(row.left(), row.center().y),
                                        Align2::LEFT_CENTER,
                                        &m.name,
                                        font.clone(),
                                        theme::WHITE,
                                    );
                                    ui.painter().text(
                                        pos2(row.right(), row.center().y),
                                        Align2::RIGHT_CENTER,
                                        status,
                                        font.clone(),
                                        color,
                                    );
                                }
                            },
                        );
                    }
                }
                let first = if !missing_workshop.is_empty() {
                    "Subscribe to missing mods"
                } else {
                    "Setup mods and join"
                };
                match footer(ui, rect, &[first, "Join only", "Cancel"]) {
                    Some(0) if !missing_workshop.is_empty() || !not_installed.is_empty() => {
                        // Subscribe, wait for Steam to install everything, then join
                        // (`SetupModsCommandExecute`).
                        let mut ids = missing_workshop.clone();
                        ids.extend(not_installed);
                        self.dialog = Some(Dialog::Downloading { key, ids });
                        if !missing_workshop.is_empty() {
                            return Some(ServersAction::Subscribe(missing_workshop));
                        }
                    }
                    Some(choice @ (0 | 1)) => {
                        let setup = if choice == 0 {
                            self.setup(&key, library)
                        } else {
                            None
                        };
                        let join = PendingJoin::new(&server, setup, settings.headless_client);
                        return self.continue_join(join, settings);
                    }
                    Some(_) => {}
                    None => self.dialog = Some(Dialog::Join { key }),
                }
            }
            Dialog::Downloading { key, ids } => {
                let server = self.server(key)?.clone();
                let rect = frame(
                    ui,
                    vec2(page.width().min(640.0), page.height().min(480.0)),
                    "Join server",
                );
                ui.painter().text(
                    pos2(rect.left() + 15.0, rect.top() + 46.0),
                    Align2::LEFT_TOP,
                    server.display_name(),
                    theme::heading(17.0),
                    theme::ACCENT,
                );
                let body = Rect::from_min_max(
                    pos2(rect.left() + 15.0, rect.top() + 74.0),
                    pos2(rect.right() - 15.0, rect.bottom() - 54.0),
                );
                // `SetupModsTemplate`: an indeterminate bar, then one row per mod.
                let now = ui.input(|i| i.time);
                let bar = Rect::from_min_size(body.min, vec2(body.width(), 40.0));
                stripes(ui, bar, now);
                let mut y = bar.bottom();
                let server_names: HashMap<u64, String> = match self.rules.get(&key) {
                    Some(Some(r)) => r
                        .mods
                        .iter()
                        .map(|m| (m.published_id, m.name.clone()))
                        .collect(),
                    _ => HashMap::new(),
                };
                let mut all_installed = true;
                for id in &ids {
                    let local = library.and_then(|l| l.find(&crate::mods::steam_id(*id)));
                    let name = server_names
                        .get(id)
                        .cloned()
                        .or_else(|| local.map(|m| m.name.clone()))
                        .unwrap_or_else(|| id.to_string());
                    let progress = local.filter(|m| m.status == Status::Busy).and_then(|m| {
                        let (name, done, total) = library?.download.as_ref()?;
                        (*name == m.name).then(|| *done as f32 / (*total).max(1) as f32)
                    });
                    let (status, color) = match local.map(|m| m.status) {
                        Some(_) if local.is_some_and(crate::mods::Mod::launchable) => {
                            ("Installed".to_owned(), theme::SECONDARY_GREEN)
                        }
                        Some(Status::Busy) => (
                            format!("Downloading: {:.1} %", progress.unwrap_or(0.0) * 100.0),
                            theme::WHITE,
                        ),
                        Some(_) => ("Subscribed".to_owned(), theme::WHITE),
                        None => ("Waiting".to_owned(), theme::WHITE),
                    };
                    all_installed &= color == theme::SECONDARY_GREEN;
                    let row = Rect::from_min_size(pos2(body.left(), y), vec2(body.width(), 42.0));
                    if row.bottom() <= body.bottom() {
                        ui.painter().rect_filled(row, 0.0, theme::argb(0xFF222222));
                        if let Some(p) = progress {
                            let fill = Rect::from_min_size(
                                row.min,
                                vec2(row.width() * p.clamp(0.0, 1.0), row.height() - 1.0),
                            );
                            ui.painter()
                                .rect_filled(fill, 0.0, theme::ACCENT.gamma_multiply(0.5));
                        }
                        ui.painter().rect_filled(
                            Rect::from_min_max(pos2(row.left(), row.bottom() - 1.0), row.max),
                            0.0,
                            theme::argb(0xFF333333),
                        );
                        gfx.paint_icon(
                            ui,
                            super::icons::DOWNLOAD_ICON,
                            Rect::from_min_size(
                                pos2(row.left() + 5.0, row.center().y - 8.0),
                                vec2(16.0, 16.0),
                            ),
                            vec2(16.0, 16.0),
                            theme::WHITE,
                        );
                        ui.painter().text(
                            pos2(row.left() + 26.0, row.center().y),
                            Align2::LEFT_CENTER,
                            &name,
                            font.clone(),
                            color,
                        );
                        ui.painter().text(
                            pos2(row.right() - 5.0, row.center().y),
                            Align2::RIGHT_CENTER,
                            status,
                            font.clone(),
                            color,
                        );
                    }
                    y += 42.0;
                }
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(250));
                if all_installed {
                    // Load exactly the server's mods, now found locally, and join.
                    let setup = self.setup(&key, library);
                    let join = PendingJoin::new(&server, setup, settings.headless_client);
                    return self.continue_join(join, settings);
                }
                match footer(ui, rect, &["Cancel"]) {
                    Some(_) => {}
                    None => self.dialog = Some(Dialog::Downloading { key, ids }),
                }
            }
        }
        None
    }

    /// "Setup mods and join": exactly the server's mods found locally, and its optional DLC.
    fn setup(&self, key: &Key, library: Option<&Library>) -> Option<Setup> {
        let Some(Some(r)) = self.rules.get(key) else {
            return None;
        };
        Some(Setup {
            mods: r
                .mods
                .iter()
                .filter(|m| !m.optional_dlc)
                .filter_map(|m| self.match_mod(m, library))
                .map(|m| m.id.clone())
                .collect(),
            dlcs: r
                .mods
                .iter()
                .filter(|m| m.optional_dlc)
                .map(|m| m.published_id as u32)
                .collect(),
        })
    }

    /// The prompts after the mods dialog, in vanilla's order: file patching, BattlEye, then the
    /// password; returns the join once nothing is left to ask.
    fn continue_join(
        &mut self,
        mut join: PendingJoin,
        settings: &JoinSettings,
    ) -> Option<ServersAction> {
        if join.step == 0 {
            join.step = 1;
            if settings.file_patching && join.denies_file_patching {
                self.dialog = Some(Dialog::Prompt {
                    prompt: Prompt::FilePatching,
                    join,
                });
                return None;
            }
        }
        if join.step == 1 {
            join.step = 2;
            if !settings.battleye && join.requires_battleye {
                self.dialog = Some(Dialog::Prompt {
                    prompt: Prompt::BattlEye,
                    join,
                });
                return None;
            }
        }
        if join.password_required {
            self.dialog = Some(Dialog::Password {
                join,
                password: String::new(),
            });
            return None;
        }
        Some(join.action(None))
    }
}

enum RowAction {
    Toggle(Key),
    Join(Key),
    Favorite(Key, bool),
}

struct Columns {
    name: Rect,
    kind: Rect,
    mission: Rect,
    players: Rect,
    ping: Rect,
    join: Rect,
}

/// Row columns, right to left: arrow 40, join 130, ping 80, players 80, mission 200, type 100;
/// four 32 px icon cells on the left; the name fills the rest. Under 900 px only the name and
/// ping remain (`ShortRow`).
fn columns(rect: Rect) -> Columns {
    let short = rect.width() < 900.0;
    let mut right = rect.right() - 40.0;
    let mut take = |w: f32| {
        let r = Rect::from_min_max(pos2(right - w, rect.top()), pos2(right, rect.bottom()));
        right -= w;
        r
    };
    let join = take(130.0);
    let ping = take(80.0);
    let (players, mission, kind) = if short {
        (Rect::NOTHING, Rect::NOTHING, Rect::NOTHING)
    } else {
        (take(80.0), take(200.0), take(100.0))
    };
    let name = Rect::from_min_max(
        pos2(rect.left() + 4.0 * 32.0 + 10.0, rect.top()),
        pos2(right, rect.bottom()),
    );
    Columns {
        name,
        kind,
        mission,
        players,
        ping,
        join,
    }
}

/// `FlatToggleButton`s used as a radio group, 26 px high from `pos`; returns the clicked value.
fn toggle_group<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    pos: egui::Pos2,
    options: &[(&str, T)],
    current: T,
) -> Option<T> {
    let font = theme::body(theme::BODY_SIZE);
    let mut x = pos.x;
    let mut picked = None;
    for (label, value) in options {
        let w = flat_button_width(ui, label);
        let b = Rect::from_min_size(pos2(x, pos.y), vec2(w, 26.0));
        x += w + 2.0;
        let selected = current == *value;
        let r = ui.interact(
            b,
            Id::unique(("servers-toggle", id, *label)),
            Sense::click(),
        );
        ui.painter().rect_filled(
            b,
            0.0,
            if selected {
                theme::SECONDARY_LABEL
            } else {
                theme::argb(0xCC000000)
            },
        );
        ui.painter().rect_stroke(
            b,
            0.0,
            (1.0, theme::argb(0xFF333333)),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            b.center(),
            Align2::CENTER_CENTER,
            *label,
            font.clone(),
            if selected { theme::BLACK } else { theme::WHITE },
        );
        if r.clicked() {
            picked = Some(*value);
        }
    }
    picked
}

fn icon_at(ui: &egui::Ui, gfx: &mut Gfx, name: &str, center: egui::Pos2, opacity: f32) {
    if let Some(texture) = gfx.bitmap(ui.ctx(), name) {
        let size = texture.size_vec2().min(vec2(18.0, 18.0));
        ui.painter().image(
            texture.id(),
            snap(ui, Rect::from_center_size(center, size)),
            FULL_UV,
            Color32::WHITE.gamma_multiply(opacity),
        );
    }
}

/// The indeterminate progress stripes (`CoolProgressBarStyle`): diagonal bands moving right.
fn stripes(ui: &egui::Ui, rect: Rect, now: f64) {
    ui.painter().rect_filled(rect, 0.0, theme::STRONG_PANEL);
    let painter = ui.painter().with_clip_rect(rect);
    let offset = ((now % 0.35) / 0.35) as f32 * 20.0;
    let mut x = rect.left() - 20.0 + offset;
    while x < rect.right() + 20.0 {
        let band = [
            pos2(x + 4.0, rect.bottom()),
            pos2(x + 7.0, rect.top()),
            pos2(x + 16.0, rect.top()),
            pos2(x + 13.0, rect.bottom()),
        ];
        painter.add(egui::Shape::convex_polygon(
            band.to_vec(),
            Color32::GRAY.gamma_multiply(0.2),
            egui::Stroke::NONE,
        ));
        x += 20.0;
    }
}
