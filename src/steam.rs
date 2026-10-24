//! The Steam API connection: callback pump and the workshop calls the launcher makes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use steamworks::{Client, ItemState, PublishedFileId};

pub const ARMA3_APP_ID: u32 = 107410;

/// A workshop file id from a bare number or a Steam URL (`?id=` / `&id=`).
pub fn published_file_id(text: &str) -> Option<u64> {
    let text = text.trim();
    if let Ok(id) = text.parse::<u64>() {
        return (id > 0).then_some(id);
    }
    let lower = text.to_ascii_lowercase();
    for key in ["?id=", "&id="] {
        let Some(rest) = lower.split(key).nth(1) else {
            continue;
        };
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(id) = digits.parse::<u64>()
            && id > 0
        {
            return Some(id);
        }
    }
    None
}
/// `kNumUGCResultsPerPage`: the most items one details query returns.
const DETAILS_BATCH: usize = 50;

/// State of a subscribed workshop item, read from Steam.
#[derive(Clone, Debug)]
pub struct ItemSnapshot {
    pub id: u64,
    pub folder: Option<PathBuf>,
    pub installed: bool,
    pub needs_update: bool,
    pub downloading: bool,
    pub download_pending: bool,
    /// Bytes downloaded and total, while downloading.
    pub progress: Option<(u64, u64)>,
}

/// Workshop details of an item.
#[derive(Clone, Debug)]
pub struct ItemDetails {
    pub id: u64,
    pub title: String,
    /// The author's SteamID64.
    pub owner: u64,
    pub time_updated: i64,
    pub children: Vec<u64>,
    /// Bytes.
    pub size: u64,
    /// Tagged "Mod" (vanilla lists only those as dependencies).
    pub is_mod: bool,
}

pub enum SteamEvent {
    Details(Vec<ItemDetails>),
    Unsubscribed(u64),
    /// The user subscribed to a workshop item, possibly in Steam itself.
    Subscribed(u64),
    /// Steam finished installing a workshop item.
    Installed(u64),
    /// The Steam persona name of a workshop author.
    Author {
        owner: u64,
        name: String,
    },
    /// A server answered for server-list request `request`.
    Server {
        request: u64,
        server: crate::servers::Server,
    },
    /// Server-list request `request` finished.
    ServersDone {
        request: u64,
    },
    /// The rules of the server at (address, query port); `None` when it didn't answer.
    Rules {
        addr: (std::net::Ipv4Addr, u16),
        rules: Option<crate::servers::rules::Rules>,
    },
}

/// The Steam server lists the browser tabs show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServerListKind {
    Internet,
    Favorites,
    History,
    Friends,
    /// The Internet list, asked for the user's unit servers only.
    Units,
}

pub struct Steam {
    client: Client,
    events_tx: Sender<SteamEvent>,
    events_rx: Receiver<SteamEvent>,
    repaint: Arc<Mutex<Option<eframe::egui::Context>>>,
    stop: Arc<AtomicBool>,
    /// Milliseconds between callback runs: 50 while the window is active, 1000 otherwise.
    pump_interval: Arc<AtomicU64>,
    pump: Option<JoinHandle<()>>,
    /// Registrations of the callbacks below; dropping them unregisters.
    _callbacks: Vec<steamworks::CallbackHandle>,
}

impl Steam {
    /// Connects as Arma 3 and starts pumping callbacks.
    pub fn connect() -> Result<Self, String> {
        let client = Client::init_app(ARMA3_APP_ID).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let pump_interval = Arc::new(AtomicU64::new(50));
        let pump = {
            let (client, stop, interval) = (client.clone(), stop.clone(), pump_interval.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    client.run_callbacks();
                    std::thread::sleep(Duration::from_millis(interval.load(Ordering::Relaxed)));
                }
            })
        };
        let (events_tx, events_rx) = channel();
        let repaint: Arc<Mutex<Option<eframe::egui::Context>>> = Default::default();
        let send = {
            let (tx, repaint) = (events_tx.clone(), repaint.clone());
            move |event: SteamEvent| {
                let _ = tx.send(event);
                if let Some(ctx) = repaint.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
            }
        };
        let callbacks = vec![
            client.register_callback({
                let send = send.clone();
                move |c: WorkshopSubscribed| {
                    if c.app == ARMA3_APP_ID {
                        send(SteamEvent::Subscribed(c.id));
                    }
                }
            }),
            client.register_callback({
                let send = send.clone();
                move |c: WorkshopUnsubscribed| {
                    if c.app == ARMA3_APP_ID {
                        send(SteamEvent::Unsubscribed(c.id));
                    }
                }
            }),
            client.register_callback({
                let send = send.clone();
                move |c: WorkshopInstalled| {
                    if c.app == ARMA3_APP_ID {
                        send(SteamEvent::Installed(c.id));
                    }
                }
            }),
            // Author names asked for in `query_details` arrive here.
            client.register_callback({
                let client = client.clone();
                move |c: steamworks::PersonaStateChange| {
                    if c.flags.contains(steamworks::PersonaChange::NAME) {
                        send(SteamEvent::Author {
                            owner: c.steam_id.raw(),
                            name: client.friends().get_friend(c.steam_id).name(),
                        });
                    }
                }
            }),
        ];
        Ok(Self {
            client,
            events_tx,
            events_rx,
            repaint,
            stop,
            pump_interval,
            pump: Some(pump),
            _callbacks: callbacks,
        })
    }

    /// Runs callbacks every 50 ms while the window is active (or a server list is loading), every
    /// second otherwise, like the vanilla launcher's slow mode.
    pub fn set_active(&self, active: bool) {
        self.pump_interval
            .store(if active { 50 } else { 1000 }, Ordering::Relaxed);
    }

    /// Lets callbacks wake the UI.
    pub fn set_repaint(&self, ctx: &eframe::egui::Context) {
        *self.repaint.lock().unwrap() = Some(ctx.clone());
    }

    pub fn try_event(&self) -> Option<SteamEvent> {
        self.events_rx.try_recv().ok()
    }

    pub fn subscribed_items(&self) -> Vec<ItemSnapshot> {
        let ugc = self.client.ugc();
        ugc.subscribed_items(false)
            .into_iter()
            .map(|id| {
                let state = ugc.item_state(id);
                let folder = ugc
                    .item_install_info(id)
                    .map(|info| PathBuf::from(info.folder));
                let downloading = state.contains(ItemState::DOWNLOADING);
                ItemSnapshot {
                    id: id.0,
                    folder,
                    installed: state.contains(ItemState::INSTALLED),
                    needs_update: state.contains(ItemState::NEEDS_UPDATE),
                    downloading,
                    download_pending: state.contains(ItemState::DOWNLOAD_PENDING),
                    progress: if downloading {
                        ugc.item_download_info(id)
                    } else {
                        None
                    },
                }
            })
            .collect()
    }

    /// Queries workshop details; results arrive as [`SteamEvent::Details`].
    pub fn query_details(&self, ids: &[u64]) {
        for batch in ids.chunks(DETAILS_BATCH) {
            let items: Vec<PublishedFileId> = batch.iter().map(|&id| PublishedFileId(id)).collect();
            let Ok(query) = self.client.ugc().query_items(items) else {
                continue;
            };
            let (tx, repaint) = (self.events_tx.clone(), self.repaint.clone());
            let client = self.client.clone();
            query.include_children(true).fetch(move |result| {
                let Ok(results) = result else { return };
                let details: Vec<ItemDetails> = (0..results.returned_results())
                    .filter_map(|i| {
                        let item = results.get(i)?;
                        Some(ItemDetails {
                            id: item.published_file_id.0,
                            owner: item.owner.raw(),
                            title: item.title,
                            time_updated: i64::from(item.time_updated),
                            size: u64::from(item.file_size),
                            is_mod: item.tags.iter().any(|t| t.eq_ignore_ascii_case("Mod")),
                            children: results
                                .get_children(i)
                                .unwrap_or_default()
                                .into_iter()
                                .map(|c| c.0)
                                .collect(),
                        })
                    })
                    .collect();
                // Author names (`AuthorNameService`): known ones now, the rest through
                // `PersonaStateChange`.
                let mut owners: Vec<u64> = details.iter().map(|d| d.owner).collect();
                owners.sort_unstable();
                owners.dedup();
                let friends = client.friends();
                for owner in owners {
                    let id = steamworks::SteamId::from_raw(owner);
                    if !friends.request_user_information(id, true) {
                        let name = friends.get_friend(id).name();
                        let _ = tx.send(SteamEvent::Author { owner, name });
                    }
                }
                let _ = tx.send(SteamEvent::Details(details));
                if let Some(ctx) = repaint.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
            });
        }
    }

    pub fn unsubscribe(&self, id: u64) {
        let (tx, repaint) = (self.events_tx.clone(), self.repaint.clone());
        self.client
            .ugc()
            .unsubscribe_item(PublishedFileId(id), move |result| {
                if result.is_ok() {
                    let _ = tx.send(SteamEvent::Unsubscribed(id));
                    if let Some(ctx) = repaint.lock().unwrap().as_ref() {
                        ctx.request_repaint();
                    }
                }
            });
    }

    /// Asks Steam to (re)download an item: update and repair.
    pub fn download(&self, id: u64) -> bool {
        self.client.ugc().download_item(PublishedFileId(id), true)
    }

    /// Stops the pump and shuts the API down, so the game can initialise its own.
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(pump) = self.pump.take() {
            let _ = pump.join();
        }
    }
}

impl Steam {
    /// Whether the user owns DLC `app_id` and has it installed (`BIsSubscribedApp`,
    /// `BIsDlcInstalled`).
    pub fn dlc_state(&self, app_id: u32) -> crate::dlc::DlcState {
        let apps = self.client.apps();
        let id = steamworks::AppId(app_id);
        crate::dlc::DlcState {
            owned: apps.is_subscribed_app(id),
            installed: apps.is_dlc_installed(id),
        }
    }

    /// Asks Steam to install an owned DLC (`ISteamApps::InstallDLC`, missing from the crate).
    pub fn install_dlc(&self, app_id: u32) {
        // SAFETY: the API is initialised for as long as `self` lives; InstallDLC only queues work.
        unsafe {
            steamworks_sys::SteamAPI_ISteamApps_InstallDLC(
                steamworks_sys::SteamAPI_SteamApps_v009(),
                app_id,
            )
        }
    }
}

impl Steam {
    /// The logged-in user's SteamID64.
    pub fn steam_id(&self) -> u64 {
        self.client.user().steam_id().raw()
    }
}

/// Rule key/value pairs as Steam returns them.
type RawRules = Vec<(Vec<u8>, Vec<u8>)>;

/// A running server-list request; dropping it releases the request.
pub struct ServerListHandle(std::sync::Arc<Mutex<steamworks::ServerListRequest>>);

impl Drop for ServerListHandle {
    fn drop(&mut self) {
        if let Ok(mut request) = self.0.lock() {
            let _ = request.release();
        }
    }
}

impl Steam {
    fn notify(&self) {
        if let Some(ctx) = self.repaint.lock().unwrap().as_ref() {
            ctx.request_repaint();
        }
    }

    /// Requests a Steam server list with `filters`; each server arrives as
    /// [`SteamEvent::Server`] tagged with `request`, then [`SteamEvent::ServersDone`].
    pub fn request_servers(
        &self,
        kind: ServerListKind,
        filters: &[(&str, String)],
        request: u64,
    ) -> Option<ServerListHandle> {
        let filters: std::collections::HashMap<&str, &str> =
            filters.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let (tx, repaint) = (self.events_tx.clone(), self.repaint.clone());
        let (tx_done, repaint_done) = (tx.clone(), repaint.clone());
        let callbacks = steamworks::ServerListCallbacks::new(
            Box::new(move |list, index| {
                let Ok(item) = list.lock().unwrap().get_server_details(index) else {
                    return;
                };
                let server = crate::servers::Server {
                    addr: item.addr,
                    query_port: item.query_port,
                    port: item.connection_port,
                    name: item.server_name,
                    map: item.map,
                    mission: crate::servers::decode_mission(&item.game_description),
                    players: item.players,
                    max_players: item.max_players,
                    ping: item.ping.as_millis() as u32,
                    password: item.have_password,
                    tags: crate::servers::GameTags::parse(&item.tags),
                };
                let _ = tx.send(SteamEvent::Server { request, server });
                if let Some(ctx) = repaint.lock().unwrap().as_ref() {
                    ctx.request_repaint_after(Duration::from_millis(500));
                }
            }),
            Box::new(|_, _| {}),
            Box::new(move |_, _| {
                let _ = tx_done.send(SteamEvent::ServersDone { request });
                if let Some(ctx) = repaint_done.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
            }),
        );
        let servers = self.client.matchmaking_servers();
        let result = match kind {
            ServerListKind::Internet | ServerListKind::Units => {
                servers.internet_server_list(ARMA3_APP_ID, &filters, callbacks)
            }
            ServerListKind::Favorites => {
                servers.favorites_server_list(ARMA3_APP_ID, &filters, callbacks)
            }
            ServerListKind::History => {
                servers.history_server_list(ARMA3_APP_ID, &filters, callbacks)
            }
            ServerListKind::Friends => {
                servers.friends_server_list(ARMA3_APP_ID, &filters, callbacks)
            }
        };
        result.ok().map(ServerListHandle)
    }

    /// Queries a server's rules (A2S_RULES through Steam); the result arrives as
    /// [`SteamEvent::Rules`].
    pub fn query_rules(&self, addr: std::net::Ipv4Addr, query_port: u16) {
        let collected: std::sync::Arc<Mutex<RawRules>> = Default::default();
        let (add, done) = (collected.clone(), collected);
        let (tx, repaint) = (self.events_tx.clone(), self.repaint.clone());
        let (tx_failed, repaint_failed) = (tx.clone(), repaint.clone());
        let callbacks = steamworks::ServerRulesCallbacks::new(
            Box::new(move |key, value| {
                add.lock()
                    .unwrap()
                    .push((key.to_bytes().to_vec(), value.to_bytes().to_vec()))
            }),
            Box::new(move || {
                let _ = tx_failed.send(SteamEvent::Rules {
                    addr: (addr, query_port),
                    rules: None,
                });
                if let Some(ctx) = repaint_failed.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
            }),
            Box::new(move || {
                let rules = crate::servers::rules::parse(&done.lock().unwrap());
                let _ = tx.send(SteamEvent::Rules {
                    addr: (addr, query_port),
                    rules,
                });
                if let Some(ctx) = repaint.lock().unwrap().as_ref() {
                    ctx.request_repaint();
                }
            }),
        );
        self.client
            .matchmaking_servers()
            .server_rules(addr, query_port, callbacks);
        self.notify();
    }

    /// Adds or removes a Steam favourite server (`k_unFavoriteFlagFavorite`, query port).
    pub fn set_favorite(
        &self,
        addr: std::net::Ipv4Addr,
        port: u16,
        query_port: u16,
        favorite: bool,
    ) {
        let ip = u32::from(addr);
        // SAFETY: the API is initialised for as long as `self` lives.
        unsafe {
            let mm = steamworks_sys::SteamAPI_SteamMatchmaking_v009();
            if favorite {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as u32);
                steamworks_sys::SteamAPI_ISteamMatchmaking_AddFavoriteGame(
                    mm,
                    ARMA3_APP_ID,
                    ip,
                    port,
                    query_port,
                    1,
                    now,
                );
            } else {
                steamworks_sys::SteamAPI_ISteamMatchmaking_RemoveFavoriteGame(
                    mm,
                    ARMA3_APP_ID,
                    ip,
                    port,
                    query_port,
                    1,
                );
            }
        }
    }

    /// Subscribes to a workshop item (the join flow's "Setup mods").
    pub fn subscribe(&self, id: u64) {
        self.client
            .ugc()
            .subscribe_item(PublishedFileId(id), |_| {});
    }
}

/// `RemoteStoragePublishedFileSubscribed_t`: subscribed to a workshop item, also in Steam.
struct WorkshopSubscribed {
    app: u32,
    id: u64,
}

// SAFETY: ID and layout are steamworks-sys's `RemoteStoragePublishedFileSubscribed_t`.
unsafe impl steamworks::Callback for WorkshopSubscribed {
    const ID: i32 = 1321;
    unsafe fn from_raw(raw: *mut std::ffi::c_void) -> Self {
        // SAFETY: Steam passes a pointer to this callback's struct.
        let c = unsafe {
            std::ptr::read_unaligned(
                raw as *const steamworks_sys::RemoteStoragePublishedFileSubscribed_t,
            )
        };
        Self {
            app: c.m_nAppID,
            id: c.m_nPublishedFileId,
        }
    }
}

/// `RemoteStoragePublishedFileUnsubscribed_t`.
struct WorkshopUnsubscribed {
    app: u32,
    id: u64,
}

// SAFETY: ID and layout are steamworks-sys's `RemoteStoragePublishedFileUnsubscribed_t`.
unsafe impl steamworks::Callback for WorkshopUnsubscribed {
    const ID: i32 = 1322;
    unsafe fn from_raw(raw: *mut std::ffi::c_void) -> Self {
        // SAFETY: Steam passes a pointer to this callback's struct.
        let c = unsafe {
            std::ptr::read_unaligned(
                raw as *const steamworks_sys::RemoteStoragePublishedFileUnsubscribed_t,
            )
        };
        Self {
            app: c.m_nAppID,
            id: c.m_nPublishedFileId,
        }
    }
}

/// `ItemInstalled_t`: Steam finished installing or updating a workshop item.
struct WorkshopInstalled {
    app: u32,
    id: u64,
}

// SAFETY: ID and layout are steamworks-sys's `ItemInstalled_t`.
unsafe impl steamworks::Callback for WorkshopInstalled {
    const ID: i32 = 3405;
    unsafe fn from_raw(raw: *mut std::ffi::c_void) -> Self {
        // SAFETY: Steam passes a pointer to this callback's struct.
        let c = unsafe { std::ptr::read_unaligned(raw as *const steamworks_sys::ItemInstalled_t) };
        Self {
            app: c.m_unAppID,
            id: c.m_nPublishedFileId,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_workshop_file_id_from_a_url_or_a_number() {
        assert_eq!(published_file_id("450814997"), Some(450814997));
        assert_eq!(
            published_file_id(
                "https://steamcommunity.com/sharedfiles/filedetails/?id=450814997&searchtext="
            ),
            Some(450814997)
        );
        assert_eq!(published_file_id("https://example.com"), None);
        assert_eq!(published_file_id("0"), None);
    }
}
