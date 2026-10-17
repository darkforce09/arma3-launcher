//! The server browser's data: servers from Steam's lists, their gametags and rules, the BI
//! server lists (official, monetized, banned), filtering and sorting.

pub mod rules;

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::path::Path;

use serde::Deserialize;

/// Session state (`s` gametag, `SessionState`).
pub const SESSION_STATES: [&str; 10] = [
    "None",
    "Selecting mission",
    "Editing mission",
    "Assigning roles",
    "Sending mission",
    "Loading game",
    "Briefing",
    "Playing",
    "Debriefing",
    "Mission aborted",
];

/// The `gametags` string (`GameTags`): comma-separated, first character the key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameTags {
    pub battleye: bool,
    pub equal_mods_required: bool,
    pub dedicated: bool,
    pub locked: bool,
    pub verify_signatures: bool,
    /// e.g. 220 for 2.20.
    pub version: u32,
    pub build: u32,
    pub game_type: String,
    pub session_state: u8,
    pub platform: Option<char>,
    /// `f`: 0 deny for everyone (also when missing), 1 headless clients only, 2 everyone.
    pub file_patching: u8,
}

impl GameTags {
    pub fn parse(tags: &str) -> Self {
        let mut t = GameTags::default();
        for tag in tags.split(',') {
            let mut chars = tag.chars();
            let Some(key) = chars.next() else { continue };
            let value = chars.as_str();
            let flag = value == "t";
            match key {
                'b' => t.battleye = flag,
                'm' => t.equal_mods_required = flag,
                'd' => t.dedicated = flag,
                'l' => t.locked = flag,
                'v' => t.verify_signatures = flag,
                'r' => t.version = value.parse().unwrap_or(0),
                'n' => t.build = value.parse().unwrap_or(0),
                't' => t.game_type = value.to_owned(),
                's' => t.session_state = value.parse().unwrap_or(0),
                'p' => t.platform = value.chars().next(),
                'f' => t.file_patching = value.parse().unwrap_or(0),
                _ => {}
            }
        }
        t
    }

    /// Whether joining needs file patching off (`JoinRequestHandlerAsync`): a dedicated server
    /// that denies it, or allows it for headless clients only and this isn't one; a player-hosted
    /// server that doesn't allow it for everyone.
    pub fn denies_file_patching(&self, headless_client: bool) -> bool {
        if self.dedicated {
            self.file_patching == 0 || (self.file_patching == 1 && !headless_client)
        } else {
            self.file_patching != 2
        }
    }
}

/// A server as Steam's list reports it.
#[derive(Clone, Debug)]
pub struct Server {
    pub addr: Ipv4Addr,
    pub query_port: u16,
    pub port: u16,
    pub name: String,
    pub map: String,
    /// The mission name (Steam's game description), URL-decoded when encoded.
    pub mission: String,
    pub players: i32,
    pub max_players: i32,
    /// Milliseconds.
    pub ping: u32,
    pub password: bool,
    pub tags: GameTags,
}

impl Server {
    pub fn key(&self) -> (Ipv4Addr, u16) {
        (self.addr, self.query_port)
    }

    /// The game type for the Type column: its first 7 characters (`GameTypesHelper`).
    pub fn game_type(&self) -> String {
        self.tags.game_type.trim().chars().take(7).collect()
    }

    /// Official names start with U+FFFD + " [ OFFICIAL ]" in Steam's list; vanilla cleans them.
    pub fn display_name(&self) -> &str {
        self.name
            .strip_prefix('\u{FFFD}')
            .map(str::trim_start)
            .unwrap_or(&self.name)
    }
}

/// `%`-decoding for mission names (`ServerRowViewModel`).
pub fn decode_mission(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = text
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The game version as `(major, minor, build)` from `arma3_x64.exe`'s `ProductVersion`.
pub fn exe_version(game_dir: &Path) -> Option<(u32, u32, u32)> {
    let exe = crate::fs_case::resolve(game_dir, "arma3_x64.exe")?;
    let data = std::fs::read(exe).ok()?;
    let key: Vec<u8> = "ProductVersion\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let start = data.windows(key.len()).rposition(|w| w == key.as_slice())? + key.len();
    // The value follows, aligned to 4 bytes, as UTF-16 up to a NUL.
    let mut pos = start;
    while !pos.is_multiple_of(4) {
        pos += 1;
    }
    let value: String = char::decode_utf16(
        data[pos..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .take(32),
    )
    .filter_map(Result::ok)
    .collect();
    let mut numbers = value
        .split(['.', ','])
        .map(|p| p.trim().parse::<u32>().ok());
    Some((
        numbers.next()??,
        numbers.next()??,
        numbers.next().flatten().unwrap_or(0),
    ))
}

/// The version gametag Steam filters on: `r` + major + two-digit minor (2.20 → `r220`).
pub fn version_tag(version: Option<(u32, u32, u32)>) -> String {
    version.map_or_else(
        || "r000".to_owned(),
        |(major, minor, _)| format!("r{major}{minor:02}"),
    )
}

/// Whether a server runs a different game version: another major.minor, or the same with a
/// newer build.
pub fn version_mismatch(server: &GameTags, ours: Option<(u32, u32, u32)>) -> bool {
    let Some((major, minor, build)) = ours else {
        return false;
    };
    server.version != major * 100 + minor || server.build > build
}

/// The five filter stages vanilla queries in turn, de-duplicating the results (`GetList`).
pub fn stages(version_tag: &str) -> Vec<Vec<(&'static str, String)>> {
    vec![
        vec![
            ("gametagsand", version_tag.to_owned()),
            ("hasplayers", "0".into()),
            ("notfull", "1".into()),
        ],
        vec![
            ("gametagsand", version_tag.to_owned()),
            ("notfull", "1".into()),
        ],
        vec![("gametagsand", version_tag.to_owned())],
        vec![("gametagsnor", version_tag.to_owned())],
        vec![],
    ]
}

/// The Units tab's queries: one `gameaddr` stage per unit server (address and connection port).
/// Vanilla sends them in one request (`or=N` and N `gameaddr` filters); Steam's filter map in the
/// steamworks crate can't hold a key twice.
pub fn unit_stages(servers: &[(Ipv4Addr, u16)]) -> Vec<Vec<(&'static str, String)>> {
    servers
        .iter()
        .map(|(addr, port)| vec![("gameaddr", format!("{addr}:{port}"))])
        .collect()
}

/// Ping colour thresholds (`PingColorConverter`).
pub fn ping_color(ping: u32) -> u32 {
    match ping {
        0..100 => 0xFF00FF97,
        100..200 => 0xFF96FF32,
        200..350 => 0xFFFFFF00,
        350..500 => 0xFFFFA500,
        _ => 0xFFFF0000,
    }
}

/// BI's server lists (`BiServersList`): official and monetized servers by IP, banned servers by
/// IP and port (`*` = every port).
#[derive(Clone, Debug, Default)]
pub struct BiLists {
    pub official: HashSet<Ipv4Addr>,
    pub monetized: HashSet<Ipv4Addr>,
    pub banned: Vec<(Ipv4Addr, Option<u16>)>,
}

#[derive(Deserialize)]
struct ListEntry {
    ip_address: String,
    #[serde(default, deserialize_with = "port_text")]
    port: Option<u16>,
}

fn port_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Number(n) => n.as_u64().and_then(|n| u16::try_from(n).ok()),
        _ => None,
    })
}

const LISTS: [(&str, &str); 3] = [
    ("official", "Servers.Official.json"),
    ("monetization", "Servers.Monetized.json"),
    ("banned", "Servers.Banned.json"),
];

impl BiLists {
    pub fn is_banned(&self, addr: Ipv4Addr, port: u16) -> bool {
        self.banned
            .iter()
            .any(|(ip, p)| *ip == addr && p.is_none_or(|p| p == port))
    }

    /// Fetches the lists (5 s timeout each), refreshing the vanilla cache files, and falls back
    /// to those files for any list that fails (the official list currently answers 404).
    pub fn load(launcher_data: &Path) -> Self {
        let mut parsed: Vec<Vec<ListEntry>> = Vec::new();
        for (name, file) in LISTS {
            let path = launcher_data.join(file);
            let fetched = ureq::get(&format!("https://servers.bistudio.com/a3/{name}/"))
                .config()
                .timeout_global(Some(std::time::Duration::from_secs(5)))
                .build()
                .call()
                .ok()
                .and_then(|mut r| r.body_mut().read_to_vec().ok())
                .filter(|bytes| serde_json::from_slice::<Vec<ListEntry>>(bytes).is_ok());
            if let Some(bytes) = &fetched {
                let _ = std::fs::write(&path, bytes);
            }
            let bytes = fetched
                .or_else(|| std::fs::read(&path).ok())
                .unwrap_or_default();
            parsed.push(
                serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes))
                    .unwrap_or_default(),
            );
        }
        let ips = |entries: &[ListEntry]| {
            entries
                .iter()
                .filter_map(|e| e.ip_address.parse().ok())
                .collect::<HashSet<Ipv4Addr>>()
        };
        BiLists {
            official: ips(&parsed[0]),
            monetized: ips(&parsed[1]),
            banned: parsed[2]
                .iter()
                .filter_map(|e| Some((e.ip_address.parse().ok()?, e.port)))
                .collect(),
        }
    }
}

/// The Filters popup and search box (`ServersTabViewModel`), applied client-side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filters {
    /// Free text: whitespace-separated terms (quotes group), `-term` excludes; each term must
    /// appear in the name, map or mission.
    pub text: String,
    pub server_name: String,
    pub map: String,
    pub mission: String,
    pub game_type: String,
    /// `Some(true)`: BattlEye only, `Some(false)`: without.
    pub battleye: Option<bool>,
    pub max_ping: Option<u32>,
    pub min_players: Option<i32>,
    pub max_players: Option<i32>,
    pub official_only: bool,
    pub no_password: bool,
    pub hide_full: bool,
    pub hide_empty: bool,
    pub hide_monetized: bool,
    /// Advanced filters, from the server's rules: 0 Recruit, 1 Regular, 2 Veteran, 3 Custom.
    pub difficulty: Option<u8>,
    /// 0 Novice, 1 Normal, 2 Expert, 3 Custom.
    pub ai_level: Option<u8>,
    pub advanced_flight_model: Option<bool>,
    pub third_person: Option<bool>,
    pub crosshair: Option<bool>,
}

impl Filters {
    /// Whether an Advanced filter is set; those need the server's rules.
    pub fn needs_rules(&self) -> bool {
        self.difficulty.is_some()
            || self.ai_level.is_some()
            || self.advanced_flight_model.is_some()
            || self.third_person.is_some()
            || self.crosshair.is_some()
    }

    /// The Advanced filters against a server's rules. Levels outside the named ones count as
    /// Custom (`ServerRowViewModel`).
    pub fn matches_rules(&self, rules: &rules::Rules) -> bool {
        self.difficulty.is_none_or(|d| d == rules.difficulty.min(3))
            && self.ai_level.is_none_or(|a| a == rules.ai_level.min(3))
            && self
                .advanced_flight_model
                .is_none_or(|f| f == rules.advanced_flight_model)
            && self.third_person.is_none_or(|t| t == rules.third_person)
            && self.crosshair.is_none_or(|c| c == rules.crosshair)
    }

    pub fn is_active(&self) -> bool {
        *self
            != Filters {
                text: self.text.clone(),
                ..Default::default()
            }
            || !self.text.is_empty()
    }

    pub fn matches(&self, s: &Server, lists: &BiLists) -> bool {
        let contains = |hay: &str, needle: &str| {
            needle.is_empty() || hay.to_lowercase().contains(&needle.to_lowercase())
        };
        let name_ok = self.server_name.is_empty()
            || contains(&s.name, &self.server_name)
            || self.server_name.trim() == s.addr.to_string()
            || self.server_name.trim() == format!("{}:{}", s.addr, s.port);
        let text_ok = crate::params::split_arguments(&self.text)
            .iter()
            .all(|term| {
                let (exclude, term) = match term.strip_prefix('-') {
                    Some(t) => (true, t),
                    None => (false, term.as_str()),
                };
                let found =
                    contains(&s.name, term) || contains(&s.map, term) || contains(&s.mission, term);
                found != exclude
            });
        name_ok
            && text_ok
            && contains(&s.map, &self.map)
            && contains(&s.mission, &self.mission)
            && contains(&s.tags.game_type, &self.game_type)
            && self.battleye.is_none_or(|b| s.tags.battleye == b)
            && self.max_ping.is_none_or(|p| s.ping <= p)
            && self.min_players.is_none_or(|p| s.players >= p)
            && self.max_players.is_none_or(|p| s.players <= p)
            && (!self.official_only || lists.official.contains(&s.addr))
            && (!self.no_password || !s.password)
            && (!self.hide_full || s.players < s.max_players)
            && (!self.hide_empty || s.players > 0)
            && (!self.hide_monetized || !lists.monetized.contains(&s.addr))
            && !lists.is_banned(s.addr, s.port)
    }
}

/// Sort modes (`PingSorterMode`); persisted by name in `ServerBrowser.Settings.json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    None,
    Ping,
    ServerName,
    ServerState,
    GameType,
    MissionName,
    CurrentPlayers,
    MaxPlayers,
}

impl Sort {
    pub const ALL: [(Sort, &str); 8] = [
        (Sort::None, "None"),
        (Sort::Ping, "Ping"),
        (Sort::ServerName, "Server name"),
        (Sort::ServerState, "Server state"),
        (Sort::GameType, "Game type"),
        (Sort::MissionName, "Mission name"),
        (Sort::CurrentPlayers, "Current players"),
        (Sort::MaxPlayers, "Max players"),
    ];

    /// Sorts servers (`PingSorter`): official first, then the mode, ties broken by name.
    pub fn sort(self, servers: &mut [&Server], reversed: bool, lists: &BiLists) {
        servers.sort_by(|a, b| {
            let official = lists
                .official
                .contains(&b.addr)
                .cmp(&lists.official.contains(&a.addr));
            let by_mode = match self {
                Sort::None | Sort::Ping => a.ping.cmp(&b.ping),
                Sort::ServerName => a
                    .display_name()
                    .to_lowercase()
                    .cmp(&b.display_name().to_lowercase()),
                Sort::ServerState => b.tags.session_state.cmp(&a.tags.session_state),
                Sort::GameType => a.tags.game_type.cmp(&b.tags.game_type),
                Sort::MissionName => a.mission.to_lowercase().cmp(&b.mission.to_lowercase()),
                Sort::CurrentPlayers => b.players.cmp(&a.players),
                Sort::MaxPlayers => b.max_players.cmp(&a.max_players),
            };
            let by_mode = if reversed { by_mode.reverse() } else { by_mode };
            official
                .then(by_mode)
                .then_with(|| a.display_name().cmp(b.display_name()))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gametags() {
        let tags = GameTags::parse(
            "bt,r220,n152984,s7,i2,mf,lf,vt,dt,tcoop,g65545,h4a5ab52e,f0,c0-52,pw,",
        );
        assert!(
            tags.battleye
                && tags.dedicated
                && tags.verify_signatures
                && !tags.locked
                && !tags.equal_mods_required
        );
        assert_eq!(
            (tags.version, tags.build, tags.session_state, tags.platform),
            (220, 152984, 7, Some('w'))
        );
        assert_eq!(tags.game_type, "coop");
    }

    #[test]
    fn versions_and_tags() {
        assert_eq!(version_tag(Some((2, 20, 152984))), "r220");
        assert_eq!(version_tag(Some((2, 5, 1))), "r205");
        assert_eq!(version_tag(None), "r000");
        let server = GameTags {
            version: 220,
            build: 152985,
            ..Default::default()
        };
        assert!(version_mismatch(&server, Some((2, 20, 152984))));
        assert!(!version_mismatch(&server, Some((2, 20, 152990))));
        assert!(version_mismatch(&server, Some((2, 18, 0))));
        assert_eq!(decode_mission("Liberation%20Altis"), "Liberation Altis");
    }

    #[test]
    fn filters_and_bans() {
        let server = Server {
            addr: Ipv4Addr::new(1, 2, 3, 4),
            query_port: 2303,
            port: 2302,
            name: "[TBD] Operations".into(),
            map: "Altis".into(),
            mission: "Liberation".into(),
            players: 0,
            max_players: 40,
            ping: 30,
            password: true,
            tags: GameTags::default(),
        };
        let mut lists = BiLists::default();
        let mut f = Filters {
            text: "tbd -koth".into(),
            ..Default::default()
        };
        assert!(f.matches(&server, &lists));
        f.hide_empty = true;
        assert!(!f.matches(&server, &lists));
        f = Filters {
            text: "altis koth".into(),
            ..Default::default()
        };
        assert!(!f.matches(&server, &lists));
        lists.banned.push((server.addr, None));
        assert!(!Filters::default().matches(&server, &lists));
    }

    #[test]
    fn file_patching_rules_follow_the_server_type() {
        let tags = |s: &str| GameTags::parse(s);
        // Dedicated: deny (also when the tag is missing), headless clients only, everyone.
        assert!(tags("dt,f0").denies_file_patching(false));
        assert!(tags("dt").denies_file_patching(true));
        assert!(tags("dt,f1").denies_file_patching(false));
        assert!(!tags("dt,f1").denies_file_patching(true));
        assert!(!tags("dt,f2").denies_file_patching(false));
        // Player hosted: only "everyone" lets it through.
        assert!(tags("df,f1").denies_file_patching(true));
        assert!(!tags("df,f2").denies_file_patching(false));
    }

    #[test]
    fn advanced_filters_read_the_rules() {
        let rules = rules::Rules {
            difficulty: 2,
            ai_level: 5,
            third_person: true,
            ..Default::default()
        };
        let mut f = Filters::default();
        assert!(!f.needs_rules() && f.matches_rules(&rules));
        f.difficulty = Some(2);
        f.third_person = Some(true);
        assert!(f.needs_rules() && f.matches_rules(&rules));
        // AI levels past Expert count as Custom.
        f.ai_level = Some(3);
        assert!(f.matches_rules(&rules));
        f.crosshair = Some(true);
        assert!(!f.matches_rules(&rules));
    }

    #[test]
    fn unit_servers_get_one_gameaddr_stage_each() {
        let stages = unit_stages(&[
            (Ipv4Addr::new(1, 2, 3, 4), 2302),
            (Ipv4Addr::new(5, 6, 7, 8), 2402),
        ]);
        assert_eq!(
            stages,
            [
                vec![("gameaddr", "1.2.3.4:2302".to_owned())],
                vec![("gameaddr", "5.6.7.8:2402".to_owned())]
            ]
        );
        assert!(unit_stages(&[]).is_empty());
    }
}
