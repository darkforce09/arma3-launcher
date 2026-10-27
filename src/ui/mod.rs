//! The launcher window: frame, navigation, pages, status bar and PLAY button.

mod anim;
mod battleye;
mod chrome;
mod dashboard;
mod dependencies;
mod dlc;
mod gfx;
mod icons;
mod menu;
mod mods;
mod nav;
mod news;
mod notifications;
mod options;
mod parameters;
mod rich;
mod servers;
pub(crate) mod theme;
mod widgets;

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use eframe::egui::{self, Align2, FontData, FontDefinitions, FontFamily, Rect, pos2};

use crate::assets::Assets;
use crate::config::LauncherConfig;
use crate::library::Library;
use crate::mods::{ScanInput, Status};
use crate::news::Post;
use crate::params::{self, Settings};
use crate::paths::GamePaths;
use crate::state::State;
use crate::steam::{ItemDetails, Steam, SteamEvent};
use crate::steam_cmd::SteamCommand;
use crate::vanilla::game_path;
use crate::vanilla::local_mods::LocalMods;
use crate::vanilla::user_config::{PRESET_SETTINGS, UserConfig};

const CONTACT_APP_ID: u32 = 1021790;
/// How often Steam is asked about downloads while some are running.
const DOWNLOAD_POLL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    News,
    Dlc,
    Parameters,
    Mods,
    BattlEye,
    Servers,
    /// Opened from the OPTIONS menu, not the navigation.
    Options,
}

impl Page {
    /// Navigation order (`LauncherViewModel`).
    pub const ALL: [Page; 7] = [
        Page::Dashboard,
        Page::News,
        Page::Dlc,
        Page::Parameters,
        Page::Mods,
        Page::BattlEye,
        Page::Servers,
    ];

    /// The page for `DefaultPageIndex` in `user.config` (`LauncherViewModel`).
    pub fn from_default_index(index: &str) -> Page {
        match index {
            "0" => Page::News,
            "1" => Page::Parameters,
            "2" => Page::Mods,
            "3" => Page::Dlc,
            "8" => Page::Servers,
            _ => Page::Dashboard,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::Dashboard => "Dashboard",
            Page::News => "News",
            Page::Dlc => "DLC",
            Page::Parameters => "Parameters",
            Page::Mods => "Mods",
            Page::BattlEye => "BattlEye",
            Page::Servers => "Servers",
            Page::Options => "Options",
        }
    }
}

/// Everything the window shows, loaded before it opens.
pub struct Startup {
    pub steam_command: Option<SteamCommand>,
    pub paths: GamePaths,
    pub assets: Option<Arc<Assets>>,
    pub config: LauncherConfig,
    pub news: Vec<Post>,
    pub owned_app_ids: Vec<u32>,
    /// The Steam beta branch, shown as a ribbon.
    pub branch: Option<String>,
}

pub struct LauncherApp<'a> {
    steam_command: Option<SteamCommand>,
    steam: Option<&'a Steam>,
    paths: GamePaths,
    config: LauncherConfig,
    news_page: news::NewsPage,
    state: State,
    gfx: gfx::Gfx,
    page: Page,
    nav: nav::Nav,
    dashboard: dashboard::Dashboard,
    mods_page: mods::ModsPage,
    library: Option<Library>,
    library_rx: Receiver<(Library, HashMap<u64, u64>)>,
    local_mods: LocalMods,
    settings: Settings,
    parameters_page: parameters::ParametersPage,
    favorites: Vec<String>,
    profiles: Vec<String>,
    allocators: Vec<String>,
    total_memory: i64,
    message: Option<menu::MessageBox>,
    /// Workshop details from Steam by item: titles of items not installed yet, sizes, authors and
    /// dependencies for the dependency dialogs.
    workshop: HashMap<u64, ItemDetails>,
    next_poll: f64,
    owns_contact: bool,
    dlc_page: dlc::DlcPage,
    options_page: options::OptionsPage,
    servers_page: servers::ServersPage,
    /// Arma 3 Units, once fetched (`None` while loading).
    units: Option<crate::units::Units>,
    units_rx: Receiver<crate::units::Units>,
    /// The game, while the launcher stays open after PLAY.
    game: Option<std::process::Child>,
    /// When the current game process was started, for the report written after it.
    game_started: Option<std::time::SystemTime>,
    /// A report check running after the game exits: the receiver and the shutdown mode to apply.
    rpt_check: Option<(Receiver<Option<std::path::PathBuf>>, String)>,
    /// The report the dialog offers to open.
    pending_rpt: Option<std::path::PathBuf>,
    /// Close the launcher once that dialog is answered (`CloseAfter`).
    close_after_report: bool,
    /// Collection mods waiting for Steam details before the dependency dialog: ids and the time
    /// the query was sent.
    collection_load: Option<(Vec<u64>, f64)>,
    /// The last game executable and arguments, to start again without BattlEye.
    last_game: Option<(&'static str, Vec<String>)>,
    /// BattlEye's status reports while the game started through it runs.
    battleye_rx: Option<Receiver<crate::battleye::Status>>,
    battleye_log: crate::battleye::Log,
    /// The BattlEye EULA, open before the first launch with BattlEye.
    eula: Option<Eula>,
    /// A PLAY (mods, extra arguments) waiting on the EULA.
    pending_play: Option<(Option<Vec<String>>, Vec<String>)>,
    /// Workshop items subscribed this session (here or in Steam), polled until Steam has
    /// installed them.
    awaiting_items: Vec<u64>,
    /// Workshop item → author SteamID64, and author → Steam persona name.
    item_owners: HashMap<u64, u64>,
    author_names: HashMap<u64, String>,
    notifications: notifications::Notifications,
    dlc_states: HashMap<u32, crate::dlc::DlcState>,
    next_dlc_refresh: f64,
    owned_app_ids: Vec<u32>,
    branch: Option<String>,
    /// The game command to run once the window has closed.
    launch: &'a mut Option<Vec<OsString>>,
}

impl<'a> LauncherApp<'a> {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        startup: Startup,
        steam: Option<&'a Steam>,
        launch: &'a mut Option<Vec<OsString>>,
    ) -> Self {
        if let Some(assets) = &startup.assets {
            install_fonts(&cc.egui_ctx, assets);
        }
        install_style(&cc.egui_ctx);
        if let Some(steam) = steam {
            steam.set_repaint(&cc.egui_ctx);
        }
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let dashboard = dashboard::Dashboard::new(&startup.owned_app_ids, seed);
        let mut gfx = gfx::Gfx::new(startup.assets);
        let mut preload: Vec<String> = ["bg_blue.jpg", "arma_logo.png"].map(String::from).to_vec();
        preload.extend(dashboard.preload_images());
        gfx.preload(
            &cc.egui_ctx,
            &preload.iter().map(String::as_str).collect::<Vec<_>>(),
        );

        let local_mods = LocalMods::load(&startup.paths.launcher_data);
        let user_config = UserConfig::find(&startup.paths.local_app_data());
        let active_preset = user_config
            .as_ref()
            .and_then(|c| c.get(PRESET_SETTINGS, "LastActivePreset"))
            .filter(|name| !name.is_empty() && name != "$");
        let state = State::load().unwrap_or_default();
        let library_rx = spawn_scan(
            &startup.paths,
            &local_mods,
            active_preset,
            state.workshop_timestamps.clone(),
            cc.egui_ctx.clone(),
        );
        let mut news_page = news::NewsPage::new(
            startup.news,
            startup.paths.launcher_data.clone(),
            state.news_seen_newest,
            &cc.egui_ctx,
        );
        news_page.unread.extend(state.news_unread.iter().copied());

        let settings = Settings::new(user_config, &startup.paths.game_dir);
        let page = Page::from_default_index(&settings.get("DefaultPageIndex"));
        let documents = startup.paths.documents();
        let servers_page = servers::ServersPage::new(
            startup.paths.launcher_data.clone(),
            startup.paths.game_dir.clone(),
        );
        Self {
            favorites: params::load_favorites(&startup.paths.launcher_data),
            profiles: params::profiles(&documents),
            allocators: params::allocators(&startup.paths.game_dir),
            total_memory: params::total_memory_mb(),
            parameters_page: parameters::ParametersPage::new(),
            settings,
            steam_command: startup.steam_command,
            steam,
            paths: startup.paths,
            config: startup.config,
            news_page,
            state,
            gfx,
            page,
            nav: nav::Nav::new(page),
            dashboard,
            mods_page: mods::ModsPage::new(),
            library: None,
            library_rx,
            local_mods,
            message: None,
            workshop: HashMap::new(),
            next_poll: 0.0,
            owns_contact: startup.owned_app_ids.contains(&CONTACT_APP_ID),
            dlc_page: dlc::DlcPage::new(),
            options_page: options::OptionsPage::new(),
            servers_page,
            game: None,
            game_started: None,
            rpt_check: None,
            pending_rpt: None,
            close_after_report: false,
            collection_load: None,
            last_game: None,
            battleye_rx: None,
            battleye_log: crate::battleye::Log::default(),
            eula: None,
            pending_play: None,
            awaiting_items: Vec::new(),
            item_owners: HashMap::new(),
            author_names: HashMap::new(),
            notifications: notifications::Notifications::default(),
            units: None,
            units_rx: spawn_units(steam.map(Steam::steam_id), cc.egui_ctx.clone()),
            dlc_states: HashMap::new(),
            next_dlc_refresh: 0.0,
            owned_app_ids: startup.owned_app_ids.clone(),
            branch: startup.branch,
            launch,
        }
    }

    /// Takes the scan result and Steam's news about workshop items.
    fn update_library(&mut self, ctx: &egui::Context) {
        if let Ok((mut library, timestamps)) = self.library_rx.try_recv() {
            self.state.workshop_timestamps = timestamps;
            if let Some(steam) = self.steam {
                let items = steam.subscribed_items();
                library.merge_steam_items(&items, &self.workshop);
                steam.query_details(&items.iter().map(|i| i.id).collect::<Vec<_>>());
            }
            self.library = Some(library);
        }
        let (Some(library), Some(steam)) = (&mut self.library, self.steam) else {
            return;
        };
        let mut found_collection = None;
        while let Some(event) = steam.try_event() {
            match event {
                SteamEvent::Details(details) => {
                    for d in &details {
                        self.workshop.insert(d.id, d.clone());
                        self.item_owners.insert(d.id, d.owner);
                        if let Some(name) = self.author_names.get(&d.owner) {
                            library.set_author(d.id, name);
                        }
                    }
                    library.merge_details(&details);
                    if found_collection.is_none() {
                        found_collection = self.mods_page.take_collection(&details);
                    }
                }
                SteamEvent::Author { owner, name } => {
                    if name.is_empty() {
                        continue;
                    }
                    for (item, _) in self.item_owners.iter().filter(|(_, o)| **o == owner) {
                        library.set_author(*item, &name);
                    }
                    self.author_names.insert(owner, name);
                }
                SteamEvent::Unsubscribed(id) => library.remove(&crate::mods::steam_id(id)),
                // Subscribed here or in Steam: listed and polled until Steam has installed it.
                SteamEvent::Subscribed(id) | SteamEvent::Installed(id) => {
                    if !self.awaiting_items.contains(&id) {
                        self.awaiting_items.push(id);
                    }
                    if !self.item_owners.contains_key(&id) {
                        steam.query_details(&[id]);
                    }
                    self.next_poll = 0.0;
                }
                server_event => {
                    self.servers_page
                        .handle(server_event, steam, ctx.input(|i| i.time))
                }
            }
        }
        if let Some(found) = found_collection {
            let mut children: Vec<u64> = found.children.into_iter().filter(|id| *id != 0).collect();
            children.sort_unstable();
            children.dedup();
            if children.is_empty() {
                self.message = Some(menu::MessageBox::info(
                    "Workshop collection",
                    "This workshop item has no mods.",
                ));
            } else {
                for id in &children {
                    steam.subscribe(*id);
                    if !self.awaiting_items.contains(id) {
                        self.awaiting_items.push(*id);
                    }
                }
                steam.query_details(&children);
                if found.save_preset {
                    let name = mods::collection_preset_name(&found.title);
                    let ids = children
                        .iter()
                        .map(|id| crate::mods::steam_id(*id))
                        .collect();
                    if let Err(e) = library.write_preset(&name, ids) {
                        self.message = Some(menu::MessageBox::info(
                            "Workshop collection",
                            format!("Could not save the preset: {e}"),
                        ));
                    }
                }
                self.collection_load = Some((children, ctx.input(|i| i.time)));
            }
        }
        let now = ctx.input(|i| i.time);
        if let Some((ids, since)) = self.collection_load.clone() {
            let ready = ids.iter().all(|id| self.workshop.contains_key(id)) || now - since > 5.0;
            if ready {
                self.collection_load = None;
                self.mods_page
                    .queue_load(ids.into_iter().map(crate::mods::steam_id).collect());
                self.page = Page::Mods;
            }
        }
        // Items subscribed this session count as busy until they're installed.
        self.awaiting_items.retain(|id| {
            library
                .find(&crate::mods::steam_id(*id))
                .is_none_or(|m| !m.launchable())
        });
        let busy = !self.awaiting_items.is_empty()
            || library
                .mods
                .iter()
                .any(|m| matches!(m.status, Status::Busy | Status::InstallationQueued));
        if busy && now >= self.next_poll {
            let merged = library.merge_steam_items(&steam.subscribed_items(), &self.workshop);
            // Items new to the library get their details (update time, dependencies, author).
            if !merged.added.is_empty() {
                steam.query_details(&merged.added);
            }
            // First installs get a "Mod was installed" notification (`ModInstalledNotification`).
            for id in &merged.installed {
                self.notifications.push(crate::mods::steam_id(*id));
            }
            self.next_poll = now + DOWNLOAD_POLL.as_secs_f64();
        }
        if busy {
            ctx.request_repaint_after(DOWNLOAD_POLL);
        }
    }

    /// Writes the loaded mods, the active preset, parameter values and favourites where the
    /// vanilla launcher reads them.
    fn save_session(&mut self) {
        if let Some(library) = &self.library {
            if let Err(e) = library.save_default_preset() {
                eprintln!("arma3-launcher: saving the loaded mods failed: {e}");
            }
            if let Some(config) = self.settings.config_mut() {
                config.set(
                    PRESET_SETTINGS,
                    "LastActivePreset",
                    &library.last_active_preset_setting(),
                );
            }
        }
        if let Err(e) = self.settings.save() {
            eprintln!("arma3-launcher: saving user.config failed: {e}");
        }
        if let Err(e) = params::save_favorites(&self.paths.launcher_data, &self.favorites) {
            eprintln!("arma3-launcher: saving Parameters.json failed: {e}");
        }
        self.state.news_seen_newest = self.news_page.newest().or(self.state.news_seen_newest);
        self.state.news_unread = self.news_page.unread.iter().copied().collect();
        self.state.save();
        self.servers_page.save_settings();
    }

    /// DLC ownership and install state: from Steam, refreshed every 60 s like the vanilla
    /// launcher, or from its caches when Steam isn't available.
    fn refresh_dlc_states(&mut self, now: f64) {
        if now < self.next_dlc_refresh {
            return;
        }
        self.next_dlc_refresh = now + 60.0;
        let installed = crate::dlc::installed_from_manifest(&self.paths.steamapps);
        for dlc in &crate::dlc::DLCS {
            let state = match self.steam {
                Some(steam) => steam.dlc_state(dlc.app_id),
                None => crate::dlc::DlcState {
                    owned: self.owned_app_ids.contains(&dlc.app_id),
                    installed: installed.contains(&dlc.app_id),
                },
            };
            self.dlc_states.insert(dlc.app_id, state);
        }
    }

    fn dlc_installed(&self, app_id: u32) -> bool {
        self.dlc_states.get(&app_id).is_some_and(|s| s.installed)
    }

    /// The `-mod` list for PLAY: loaded optional DLC folders first, then the loaded mods.
    fn session_mods(&self) -> Vec<String> {
        let Some(library) = &self.library else {
            return Vec::new();
        };
        let mut mods = crate::dlc::launch_paths(&library.dlcs, &self.paths.game_dir, |id| {
            self.dlc_installed(id)
        });
        mods.extend(library.launch_mod_paths());
        mods
    }

    /// The `-mod` list for playing with preset `name`.
    fn preset_mods(&self, name: &str) -> Vec<String> {
        let Some(library) = &self.library else {
            return Vec::new();
        };
        let dlcs = library
            .preset(name)
            .map(|p| p.dlcs.clone())
            .unwrap_or_default();
        let mut mods =
            crate::dlc::launch_paths(&dlcs, &self.paths.game_dir, |id| self.dlc_installed(id));
        mods.extend(library.preset_mod_paths(name));
        mods
    }

    /// Starts the game with the enabled parameters and `mod_paths` (the session's mods when
    /// `None`), then `extra` arguments: join (`-connect`, `-port`, `-password`) or host.
    fn play(&mut self, ctx: &egui::Context, mod_paths: Option<Vec<String>>, extra: Vec<String>) {
        let Some(command) = &self.steam_command else {
            self.message = Some(menu::MessageBox::info(
                "Start from Steam",
                format!(
                    "Start the launcher from Steam to play: set Arma 3's launch options to\n{} %command%",
                    std::env::current_exe().unwrap_or_default().display()
                ),
            ));
            return;
        };
        // Enabled parameters in definition order, then one -mod with the loaded mods followed by
        // the user's own Mods parameter (`GameParameterProcessorBase`).
        let request = (mod_paths.clone(), extra.clone());
        let (mut args, user_mods) = params::command_line(&self.settings);
        let mut mods = mod_paths.unwrap_or_else(|| self.session_mods());
        mods.extend(user_mods);
        args.push(format!("-mod={}", mods.join(";")));
        // Joining replaces any -connect, -port and -password parameters (`GameParameterProcessorBase`).
        if extra.iter().any(|a| a.starts_with("-connect=")) {
            args.retain(|a| {
                !["-connect=", "-port=", "-password="]
                    .iter()
                    .any(|p| a.to_ascii_lowercase().starts_with(p))
            });
        }
        args.extend(extra);
        let platform = self.settings.platform();
        // BattlEye is for the client and profiling builds, and never with -server.
        let battleye = self.settings.flag("UseBattleEye")
            && matches!(platform.as_str(), "" | "profiling")
            && !args.iter().any(|a| a.eq_ignore_ascii_case("-server"));
        // The first launch with BattlEye waits for its EULA (`BattlEyeEulaHelper`).
        if battleye && self.settings.flag("ShowBeEULA") {
            self.pending_play = Some(request);
            self.eula = Some(Eula::open(&self.paths.game_dir, ctx));
            return;
        }
        let exe = params::platform_exe(&platform);
        let game = command.game_command(exe, battleye, &args);
        self.last_game = Some((exe, args));
        self.save_session();
        self.start_game(ctx, game, battleye);
    }

    /// Hands over to the game or starts it and stays (`LauncherShutdownMode`). Staying, the
    /// BattlEye launcher's status reports are read from its output.
    fn start_game(&mut self, ctx: &egui::Context, game: Vec<OsString>, battleye: bool) {
        match self.settings.get("LauncherShutdownMode").as_str() {
            "CloseBefore" => {
                *self.launch = Some(game);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            mode => {
                let mut command = crate::steam_env::command_with_steam_env(&game);
                if battleye {
                    command.stdout(std::process::Stdio::piped());
                }
                match command.spawn() {
                    Ok(mut child) => {
                        self.game_started = Some(std::time::SystemTime::now());
                        self.battleye_rx = child.stdout.take().map(|output| {
                            let (tx, rx) = channel();
                            let ctx = ctx.clone();
                            std::thread::spawn(move || {
                                crate::battleye::read_statuses(output, |status| {
                                    let _ = tx.send(status);
                                    ctx.request_repaint();
                                })
                            });
                            rx
                        });
                        self.game = Some(child);
                        if mode == "Minimize" {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    }
                    Err(e) => {
                        self.message = Some(menu::MessageBox::info(
                            "Start the game",
                            format!("Starting the game failed: {e}"),
                        ))
                    }
                }
            }
        }
    }

    /// BattlEye's status reports go to the BattlEye page, which opens for warnings and errors;
    /// errors that stop the game offer a start without BattlEye
    /// (`GameExecutorBattlEyeStatusChanged`).
    fn update_battleye(&mut self) {
        let Some(rx) = &self.battleye_rx else { return };
        let statuses: Vec<_> = rx.try_iter().collect();
        for status in statuses {
            let time = chrono::Local::now().format("%H:%M:%S").to_string();
            self.battleye_log.push(&status, &time);
            if status.severity() != crate::battleye::Severity::Info {
                self.page = Page::BattlEye;
            }
            if let Some(error) = status.error_message() {
                self.message = Some(menu::MessageBox {
                    caption: "BattlEye error".into(),
                    body: format!("{error} Do you want to launch Arma 3 without BattlEye?"),
                    buttons: vec!["Yes".into(), "No".into()],
                    confirm: Some(menu::Confirm::PlayWithoutBattlEye),
                });
            }
        }
    }

    /// The EULA's answer: accepting stops asking; declining turns BattlEye off and says so.
    /// Either way the held-back PLAY goes on.
    fn answer_eula(&mut self, ctx: &egui::Context, accepted: bool) {
        self.eula = None;
        if accepted {
            self.settings.set_flag("ShowBeEULA", false);
            self.resume_play(ctx);
        } else {
            self.settings.set_flag("UseBattleEye", false);
            self.message = Some(menu::MessageBox {
                caption: "BattlEye EULA Declined".into(),
                body: "You have declined the BattlEye EULA license. Arma 3 will launch without BattlEye.\n\nTo enable or disable BattlEye go to the BattlEye page in Launcher.".into(),
                buttons: vec!["OK".into()],
                confirm: Some(menu::Confirm::ResumePlay),
            });
        }
    }

    fn resume_play(&mut self, ctx: &egui::Context) {
        if let Some((mods, extra)) = self.pending_play.take() {
            self.play(ctx, mods, extra);
        }
    }

    /// The status bar message (`MainProgressBarViewModel`), with download progress.
    fn status_line(&self, ui: &egui::Ui, win: Rect) {
        let white = theme::WHITE;
        let Some(library) = &self.library else {
            widgets::status_message(ui, win, &[("Loading data...", white)]);
            return;
        };
        if let Some((name, done, total)) = &library.download {
            let (done_mb, total_mb) = (*done as f64 / 1_048_576.0, *total as f64 / 1_048_576.0);
            let percent = done_mb / total_mb * 100.0;
            widgets::status_progress(ui, win, (*done as f32 / *total as f32).clamp(0.0, 1.0));
            let text =
                format!("Updating mod {name}: {percent:.0} % - {done_mb:.1} / {total_mb:.1} MB");
            widgets::status_message(ui, win, &[(&text, white)]);
        } else if library
            .mods
            .iter()
            .any(|m| m.status == Status::InstallationQueued)
        {
            widgets::status_message(
                ui,
                win,
                &[("Waiting for Steam to download mod updates", white)],
            );
        } else if library.mods.iter().any(|m| m.updated) {
            widgets::status_message(
                ui,
                win,
                &[
                    ("Some mods were recently ", white),
                    ("updated", theme::accent()),
                ],
            );
        } else if self.steam.is_none() {
            widgets::status_message(
                ui,
                win,
                &[("Updates unavailable - Steam is offline", white)],
            );
        } else if library.mods.iter().any(|m| m.is_workshop()) {
            widgets::status_message(ui, win, &[("Mods are up-to-date - Steam is online", white)]);
        } else {
            widgets::status_message(ui, win, &[("Ready to play - Steam is online", white)]);
        }
    }

    /// Runs what the Servers page asked for.
    fn server_action(&mut self, ctx: &egui::Context, action: servers::ServersAction) {
        match action {
            servers::ServersAction::Join {
                addr,
                port,
                password,
                setup,
                disable_file_patching,
                enable_battleye,
            } => {
                // The answers to the join prompts.
                if disable_file_patching {
                    self.settings.set_flag("EnableFilePatching", false);
                }
                if enable_battleye {
                    self.settings.set_flag("UseBattleEye", true);
                }
                if let (Some(setup), Some(library)) = (setup, &mut self.library) {
                    library.loaded = setup.mods;
                    library.dlcs = setup
                        .dlcs
                        .into_iter()
                        .filter(|id| self.dlc_states.get(id).is_some_and(|s| s.installed))
                        .collect();
                    library.active_preset = None;
                }
                let mut extra = vec![format!("-connect={addr}"), format!("-port={port}")];
                extra.extend(
                    password
                        .filter(|p| !p.is_empty())
                        .map(|p| format!("-password={p}")),
                );
                self.play(ctx, None, extra);
            }
            servers::ServersAction::Host => self.play(ctx, None, vec!["-setupHost".into()]),
            servers::ServersAction::Subscribe(ids) => {
                // The join dialog waits for these, then joins.
                if let Some(steam) = self.steam {
                    for id in &ids {
                        steam.subscribe(*id);
                    }
                    steam.query_details(&ids);
                }
                self.awaiting_items.extend(ids);
                self.next_poll = 0.0;
            }
        }
    }

    /// The OPTIONS menu (`CaptionContextMenuStyle`): Launcher Options, web links, and the
    /// version block.
    fn options_menu(&mut self, button: &egui::Response) {
        let mut open_options = false;
        egui::Popup::menu(button)
            .frame(menu::frame())
            .width(260.0)
            .show(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                open_options = menu::item(ui, "Launcher Options", true);
                let latest_rpt = crate::rpt::newest(&self.paths.arma_local());
                if menu::item(ui, "Open latest RPT", latest_rpt.is_some())
                    && let Some(path) = latest_rpt
                {
                    let _ = open::that_detached(path);
                }
                menu::separator(ui);
                for (label, key) in [("Feedback", "FeedbackUrl"), ("Support", "SupportUrl")] {
                    if menu::item(ui, &format!("{label} \u{2197}"), true) {
                        self.config.open_url(key);
                    }
                }
                menu::separator(ui);
                for (label, key) in [
                    ("Bohemia Interactive", "BiStudioUrl"),
                    ("Arma 3", "Arma3Url"),
                    ("Community Wiki", "CommunityWikiUrl"),
                    ("Steam Workshop", "WorkshopUrl"),
                ] {
                    if menu::item(ui, &format!("{label} \u{2197}"), true) {
                        self.config.open_url(key);
                    }
                }
                menu::separator(ui);
                let branch = self.branch.clone().unwrap_or_else(|| "Main".into());
                let build = self.paths.build_id().unwrap_or_default();
                let version = env!("CARGO_PKG_VERSION");
                for (key, value) in [
                    ("Game:", build.as_str()),
                    ("Branch:", branch.as_str()),
                    ("Launcher:", version),
                ] {
                    let mut job = egui::text::LayoutJob::default();
                    job.append(
                        key,
                        0.0,
                        egui::TextFormat::simple(theme::body(theme::BODY_SIZE), theme::HINT_TEXT),
                    );
                    job.append(
                        value,
                        6.0,
                        egui::TextFormat::simple(theme::body(theme::BODY_SIZE), theme::WHITE),
                    );
                    let galley = ui.painter().layout_job(job);
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), galley.size().y + 8.0),
                        egui::Sense::hover(),
                    );
                    ui.painter().galley(
                        pos2(rect.left() + 14.0, rect.top() + 4.0),
                        galley,
                        theme::WHITE,
                    );
                }
            });
        if open_options {
            self.page = Page::Options;
        }
    }

    /// While the game runs with the launcher open: restore a minimised launcher, or close it,
    /// when the game ends (`LauncherShutdownMode`). A report newer than the launch that records
    /// an error is offered first.
    fn watch_game(&mut self, ctx: &egui::Context) {
        if self.game.is_some() {
            let still_running = matches!(self.game.as_mut().unwrap().try_wait(), Ok(None));
            if still_running {
                ctx.request_repaint_after(Duration::from_secs(1));
            } else {
                self.game = None;
                let mode = self.settings.get("LauncherShutdownMode");
                let dir = self.paths.arma_local();
                let started = self.game_started;
                let (tx, rx) = channel();
                std::thread::spawn(move || {
                    let path = started.and_then(|started| {
                        let path = crate::rpt::newest(&dir)?;
                        let modified = path.metadata().ok()?.modified().ok()?;
                        (modified >= started && crate::rpt::has_error(&path)).then_some(path)
                    });
                    let _ = tx.send(path);
                });
                self.rpt_check = Some((rx, mode));
            }
        }
        let Some((rx, mode)) = &self.rpt_check else {
            return;
        };
        let Ok(found) = rx.try_recv() else {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        };
        let mode = mode.clone();
        self.rpt_check = None;
        if let Some(path) = found {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "report".into());
            self.pending_rpt = Some(path);
            self.close_after_report = mode == "CloseAfter";
            if mode == "Minimize" {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            }
            self.message = Some(menu::MessageBox {
                caption: "Arma 3 report".into(),
                body: format!("Arma 3 ended with errors in {name}.\n\nOpen the report?"),
                buttons: vec!["Open".into(), "Close".into()],
                confirm: Some(menu::Confirm::OpenRpt),
            });
        } else {
            self.finish_shutdown(ctx, &mode);
        }
    }

    fn finish_shutdown(&self, ctx: &egui::Context, mode: &str) {
        match mode {
            "Minimize" => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false)),
            "CloseAfter" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
    }

    fn handle_confirm(&mut self, ctx: &egui::Context, confirm: menu::Confirm) {
        match confirm {
            menu::Confirm::ResumePlay => self.resume_play(ctx),
            menu::Confirm::PlayWithoutBattlEye => {
                // `ReRunGameWithLastParametersWithoutBattlEye`.
                if let (Some(command), Some((exe, args))) = (&self.steam_command, &self.last_game) {
                    let game = command.game_command(exe, false, args);
                    self.start_game(ctx, game, false);
                }
            }
            menu::Confirm::OpenRpt => {
                if let Some(path) = self.pending_rpt.take() {
                    let _ = open::that_detached(path);
                }
            }
            menu::Confirm::UnsubscribeAll => {
                if let (Some(steam), Some(library)) = (self.steam, &self.library) {
                    for id in library
                        .mods
                        .iter()
                        .filter_map(crate::mods::Mod::workshop_id)
                    {
                        steam.unsubscribe(id);
                    }
                }
            }
        }
    }
}

impl eframe::App for LauncherApp<'_> {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.launch.is_none() {
            self.save_session();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::set_index(&self.settings.get("ThemeIndex"));
        #[cfg(windows)]
        crate::platform::disable_rounded_corners();
        self.update_library(&ctx);
        let state = chrome::window_state(&ctx);
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect)
            && !state.maximized
        {
            self.state.window_size = Some([rect.width(), rect.height()]);
        }
        if let Some(steam) = self.steam {
            steam.set_active(state.focused || self.page == Page::Servers);
        }
        let win = chrome::content_rect(ui, &state);
        let background = options::theme_background(&self.settings.get("ThemeIndex"));
        chrome::paint_frame(ui, &mut self.gfx, win, &state, background);
        if let Some(branch) = &self.branch {
            chrome::branch_ribbon(ui, win, branch);
        }
        let options_button = chrome::caption_buttons(ui, &mut self.gfx, win, &state);
        self.options_menu(&options_button);
        self.watch_game(&ctx);
        self.update_battleye();
        self.refresh_dlc_states(ctx.input(|i| i.time));
        if let Ok(units) = self.units_rx.try_recv() {
            self.units = Some(units);
        }
        self.news_page.update();
        if let Some(page) = self
            .nav
            .show(ui, win, self.page, self.news_page.unread.len())
        {
            self.page = page;
        }

        let mut page_rect = Rect::from_min_max(
            pos2(
                win.left() + theme::NAV_WIDTH,
                win.top() + theme::TITLE_HEIGHT,
            ),
            pos2(win.right(), win.bottom() - theme::STATUS_HEIGHT),
        );
        // The notification bar docks below the page.
        if let Some(library) = &mut self.library
            && !self.notifications.is_empty()
        {
            let bar = Rect::from_min_max(
                pos2(page_rect.left(), page_rect.bottom() - notifications::HEIGHT),
                page_rect.max,
            );
            page_rect.max.y = bar.top();
            // Load mod goes through the Mods page, which asks about dependencies.
            if let Some(id) = self.notifications.show(ui, &mut self.gfx, bar, library) {
                let env = mods::ModsEnv {
                    config: &self.config,
                    steam: self.steam,
                    launcher_data: &self.paths.launcher_data,
                    local: &mut self.local_mods,
                    message: &mut self.message,
                    awaiting_items: &mut self.awaiting_items,
                    workshop: &self.workshop,
                    authors: &self.author_names,
                };
                self.mods_page.request_load(vec![id], None, library, &env);
                self.page = Page::Mods;
            }
        }
        match self.page {
            Page::Dashboard => self.dashboard.show(
                ui,
                &mut self.gfx,
                &self.config,
                page_rect,
                self.news_page.posts.first(),
                self.units.as_ref(),
            ),
            Page::Mods => match &mut self.library {
                Some(library) => {
                    let mut env = mods::ModsEnv {
                        config: &self.config,
                        steam: self.steam,
                        launcher_data: &self.paths.launcher_data,
                        local: &mut self.local_mods,
                        message: &mut self.message,
                        awaiting_items: &mut self.awaiting_items,
                        workshop: &self.workshop,
                        authors: &self.author_names,
                    };
                    self.mods_page
                        .show(ui, &mut self.gfx, page_rect, library, &mut env);
                }
                None => loading(ui, page_rect, "Initializing mods management..."),
            },
            Page::Parameters => {
                let env = parameters::ParamsEnv {
                    game_dir: &self.paths.game_dir,
                    profiles: &self.profiles,
                    allocators: &self.allocators,
                    total_memory: self.total_memory,
                };
                self.parameters_page.show(
                    ui,
                    &mut self.gfx,
                    page_rect,
                    &mut self.settings,
                    &mut self.favorites,
                    &env,
                );
            }
            Page::News => self.news_page.show(ui, &mut self.gfx, page_rect),
            Page::Servers => {
                let states = self.dlc_states.clone();
                let owned = move |id: u32| states.get(&id).is_some_and(|s| s.owned);
                let join = servers::JoinSettings {
                    file_patching: self.settings.flag("EnableFilePatching"),
                    headless_client: self.settings.flag("Client"),
                    battleye: self.settings.flag("UseBattleEye"),
                };
                if let Some(action) = self.servers_page.show(
                    ui,
                    &mut self.gfx,
                    page_rect,
                    self.steam,
                    self.library.as_ref(),
                    &owned,
                    self.units.as_ref(),
                    &join,
                ) {
                    self.server_action(&ctx, action);
                }
            }
            Page::Dlc => match &mut self.library {
                Some(library) => {
                    let action = self.dlc_page.show(
                        ui,
                        &mut self.gfx,
                        page_rect,
                        library,
                        &self.dlc_states,
                        &self.config,
                        self.steam,
                        &self.paths.game_dir,
                    );
                    let mut env = mods::ModsEnv {
                        config: &self.config,
                        steam: self.steam,
                        launcher_data: &self.paths.launcher_data,
                        local: &mut self.local_mods,
                        message: &mut self.message,
                        awaiting_items: &mut self.awaiting_items,
                        workshop: &self.workshop,
                        authors: &self.author_names,
                    };
                    self.mods_page
                        .preset_controls(ui, &mut self.gfx, page_rect, library, &mut env);
                    if let (Some(dlc::DlcAction::Install(id)), Some(steam)) = (action, self.steam) {
                        steam.install_dlc(id);
                    }
                }
                None => loading(ui, page_rect, "Initializing DLC management..."),
            },
            Page::Options => self.options_page.show(
                ui,
                &mut self.gfx,
                page_rect,
                &mut self.settings,
                &mut self.local_mods,
                &self.paths.launcher_data,
                &self.paths.game_dir,
            ),
            Page::BattlEye => battleye::show(
                ui,
                &mut self.gfx,
                page_rect,
                &mut self.settings,
                &self.config,
                &self.battleye_log,
            ),
        }

        widgets::status_bar(ui, win);
        self.status_line(ui, win);
        let play_style = self
            .library
            .as_ref()
            .map_or(widgets::PlayStyle::Play, |lib| {
                widgets::PlayStyle::new(
                    lib.dlcs
                        .iter()
                        .any(|id| crate::dlc::find(*id).is_some_and(|d| !d.mandatory)),
                    lib.loaded.iter().any(|id| lib.find(id).is_some()),
                )
            });
        let play = widgets::play_button(ui, &mut self.gfx, win, play_style);
        if play.play {
            self.play(&ctx, None, Vec::new());
        }
        let presets: Vec<String> = self
            .library
            .iter()
            .flat_map(|l| l.presets.iter().map(|(n, _)| n.clone()))
            .collect();
        let mut chosen: Option<Vec<String>> = None;
        // The preset list scrolls in the room above PLAY, below the two header rows and separator.
        let presets_height = (play.menu.rect.top() - win.top() - 77.0 - 8.0).max(120.0);
        egui::Popup::menu(&play.menu)
            .frame(menu::frame())
            .width(200.0)
            .show(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                if menu::item(ui, "Play without mods and optional DLC", true) {
                    chosen = Some(Vec::new());
                }
                menu::separator(ui);
                menu::item(ui, "Play with a preset:", false);
                egui::ScrollArea::vertical()
                    .id_salt("play-presets")
                    .max_height(presets_height)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        for name in &presets {
                            if menu::item(ui, &format!("      {name}"), true) {
                                chosen = Some(self.preset_mods(name));
                            }
                        }
                    });
            });
        if let Some(mods) = chosen {
            self.play(&ctx, Some(mods), Vec::new());
        }
        if self.owns_contact && widgets::play_contact_button(ui, win) {
            // PLAY CONTACT: vanilla game with only the Contact DLC loaded.
            let contact = crate::fs_case::resolve(&self.paths.game_dir, "contact");
            self.play(
                &ctx,
                Some(contact.iter().map(|c| game_path(c)).collect()),
                Vec::new(),
            );
        }
        chrome::resize_border(ui, win, &state);

        // Files dragged onto the window: preset HTML files and mod folders.
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            widgets::drop_pad(ui, win);
        }
        for file in ctx.input(|i| i.raw.dropped_files.clone()) {
            self.mods_page.queue_drop(file.path().to_path_buf());
            self.page = Page::Mods;
        }

        if let Some(message) = &self.message
            && let Some(button) = message.show(ui, win)
        {
            let confirm = message.confirm.filter(|_| button == 0);
            let close_after = self.close_after_report;
            self.close_after_report = false;
            self.message = None;
            if let Some(confirm) = confirm {
                self.handle_confirm(&ctx, confirm);
            }
            if close_after {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if let Some(eula) = &mut self.eula {
            eula.receive();
            if let Some(accepted) = battleye::eula(ui, win, &eula.licence) {
                self.answer_eula(&ctx, accepted);
            }
        }
    }
}

/// The BattlEye EULA dialog's state: BattlEye's licence text (`BattlEye/EULA.txt`), read off the
/// UI thread; empty while loading.
struct Eula {
    licence: String,
    rx: Receiver<String>,
}

impl Eula {
    fn open(game_dir: &std::path::Path, ctx: &egui::Context) -> Self {
        let (tx, rx) = channel();
        let (game_dir, ctx) = (game_dir.to_path_buf(), ctx.clone());
        std::thread::spawn(move || {
            let text = crate::fs_case::resolve(&game_dir, "BattlEye/EULA.txt")
                .and_then(|path| std::fs::read(path).ok())
                .map_or_else(
                    || "BattlEye/EULA.txt was not found in the game folder.".to_owned(),
                    // One paragraph per line, as in vanilla's copy of the licence.
                    |bytes| {
                        String::from_utf8_lossy(&bytes)
                            .lines()
                            .map(str::trim)
                            .filter(|l| !l.is_empty())
                            .collect::<Vec<_>>()
                            .join("\n")
                    },
                );
            let _ = tx.send(text);
            ctx.request_repaint();
        });
        Eula {
            licence: String::new(),
            rx,
        }
    }

    fn receive(&mut self) {
        if let Ok(text) = self.rx.try_recv() {
            self.licence = text;
        }
    }
}

/// `LoadingBox`: the busy text over the page while it loads.
fn loading(ui: &egui::Ui, rect: Rect, text: &str) {
    ui.painter().rect_filled(rect, 0.0, theme::STRONG_PANEL);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        theme::heading(18.0),
        theme::WHITE,
    );
}

/// Scans the mods off the UI thread and builds the library with the last session's state.
/// Also returns the workshop timestamps seen now, for the next start's "updated" tags; without
/// any from a previous start, the vanilla launcher's `Steam.json` provides them.
fn spawn_scan(
    paths: &GamePaths,
    local: &LocalMods,
    active_preset: Option<String>,
    seen: HashMap<u64, u64>,
    ctx: egui::Context,
) -> Receiver<(Library, HashMap<u64, u64>)> {
    let (tx, rx) = channel();
    let (steamapps, game_dir, launcher_data) = (
        paths.steamapps.clone(),
        paths.game_dir.clone(),
        paths.launcher_data.clone(),
    );
    let drives = paths.drive_map();
    let local = local.clone();
    std::thread::spawn(move || {
        let workshop_cache = crate::vanilla::workshop_cache(&launcher_data);
        let seen = if seen.is_empty() {
            workshop_cache
                .iter()
                .map(|(id, info)| (*id, info.publish_timestamp))
                .collect()
        } else {
            seen
        };
        let mods = crate::mods::scan(&ScanInput {
            steamapps: &steamapps,
            game_dir: &game_dir,
            drives: &drives,
            local: &local,
            workshop_cache: &workshop_cache,
            seen_timestamps: &seen,
        });
        let mut now = seen;
        for m in &mods {
            if let Some(id) = m.workshop_id()
                && let Some(ts) = crate::mods::read_cpp(&m.path.join("meta.cpp"))
                    .get("timestamp")
                    .and_then(|t| t.parse().ok())
            {
                now.insert(id, ts);
            }
        }
        let library = Library::new(mods, launcher_data.join("Presets"), active_preset);
        let _ = tx.send((library, now));
        ctx.request_repaint();
    });
    rx
}

/// Fetches the user's Arma 3 Units off the UI thread; without Steam they can't be looked up.
fn spawn_units(steam_id: Option<u64>, ctx: egui::Context) -> Receiver<crate::units::Units> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let units = steam_id.map_or(crate::units::Units::Failed, crate::units::fetch);
        let _ = tx.send(units);
        ctx.request_repaint();
    });
    rx
}

/// Registers the launcher's Roboto and Purista faces; Roboto Regular is the default
/// proportional font, like the vanilla window's `PuristaFamily`.
fn install_fonts(ctx: &egui::Context, assets: &Assets) {
    let mut fonts = FontDefinitions::default();
    for (family, file) in theme::FONT_FILES {
        let Some(bytes) = assets.get(&format!("resources/defaulttheme/fonts/{file}")) else {
            continue;
        };
        fonts.font_data.insert(
            family.to_owned(),
            Arc::new(FontData::from_owned(bytes.to_vec())),
        );
        let mut chain = vec![family.to_owned()];
        chain.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
        fonts
            .families
            .insert(FontFamily::Name(family.into()), chain);
    }
    if fonts.font_data.contains_key("roboto-regular") {
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "roboto-regular".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// egui's own widgets (scroll bars, text fields, popups) in the vanilla theme.
fn install_style(ctx: &egui::Context) {
    ctx.global_style_mut(|style| {
        let v = &mut style.visuals;
        v.dark_mode = true;
        v.selection.bg_fill = theme::accent();
        v.selection.stroke = egui::Stroke::new(1.0, theme::WHITE);
        v.text_cursor.stroke = egui::Stroke::new(1.0, theme::WHITE);
        v.extreme_bg_color = theme::BLACK;
        v.window_fill = theme::BLACK;
        v.panel_fill = egui::Color32::TRANSPARENT;
        // Scroll bar handle (`ScrollBarThumbColor` #EEFFFFFF, brighter on hover/drag).
        v.widgets.inactive.fg_stroke.color = theme::argb(0xEEFFFFFF);
        v.widgets.hovered.fg_stroke.color = theme::argb(0xF0FFFFFF);
        v.widgets.active.fg_stroke.color = theme::WHITE;
        v.widgets.inactive.bg_fill = theme::argb(0xEEFFFFFF);
        v.widgets.hovered.bg_fill = theme::argb(0xF0FFFFFF);
        v.widgets.active.bg_fill = theme::WHITE;
        v.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
        v.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
        v.widgets.active.corner_radius = egui::CornerRadius::ZERO;
        let scroll = &mut style.spacing.scroll;
        *scroll = egui::style::ScrollStyle::floating();
        scroll.bar_width = 6.0;
        scroll.floating_width = 6.0;
        scroll.floating_allocated_width = 0.0;
        scroll.bar_inner_margin = 3.0;
        scroll.bar_outer_margin = 0.0;
        scroll.dormant_handle_opacity = 0.4;
        scroll.active_handle_opacity = 1.0;
        scroll.interact_handle_opacity = 1.0;
        scroll.dormant_background_opacity = 0.0;
        scroll.active_background_opacity = 0.0;
        scroll.interact_background_opacity = 0.0;
        scroll.handle_min_length = 24.0;
    });
}
