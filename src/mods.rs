//! The mod library: workshop items and local mod folders, their health, and load order.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::vanilla::local_mods::LocalMods;
use crate::vanilla::{DriveMap, WorkshopInfo, local_mod_id};

pub const ARMA3_APP_ID: &str = "107410";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Workshop(u64),
    /// A local folder, with its path as the vanilla launcher stores it (`Z:\…`).
    Local {
        windows_path: String,
    },
}

/// `ModStatus` in the vanilla launcher, reduced to what this launcher can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    /// Downloading or installing.
    Busy,
    /// Missing, or without an `addons` folder holding PBOs.
    Corrupted,
    UpdateAvailable,
    InstallationQueued,
    DependenciesMissing,
}

impl Status {
    /// The 16 × 16 status icon (`ExtensionState_*.png`); none for a healthy mod.
    pub fn icon(self) -> Option<&'static str> {
        Some(match self {
            Status::Ok => return None,
            Status::Busy => "extensionstate_busy.png",
            Status::Corrupted => "extensionstate_corrupted.png",
            Status::UpdateAvailable => "extensionstate_updateavailable.png",
            Status::InstallationQueued => "extensionstate_queued.png",
            Status::DependenciesMissing => "extensionstate_dependenciesmissing.png",
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Busy => "Downloading",
            Status::Corrupted => "Corrupted",
            Status::UpdateAvailable => "Update available",
            Status::InstallationQueued => "Queued for download",
            Status::DependenciesMissing => "Dependencies missing",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Mod {
    /// The vanilla identifier: `steam:<id>` or `local:<WINDOWS PATH>\`.
    pub id: String,
    pub source: Source,
    pub name: String,
    pub author: String,
    pub path: PathBuf,
    /// Workshop dependencies, as identifiers.
    pub dependencies: Vec<String>,
    pub status: Status,
    /// Updated since the launcher last looked (the "updated" tag).
    pub updated: bool,
    /// A local mod found in a watched folder that the vanilla launcher didn't know yet.
    pub auto_included: bool,
    /// Last workshop update, Unix seconds.
    pub last_update: Option<i64>,
}

impl Mod {
    pub fn is_workshop(&self) -> bool {
        matches!(self.source, Source::Workshop(_))
    }

    pub fn workshop_id(&self) -> Option<u64> {
        match self.source {
            Source::Workshop(id) => Some(id),
            Source::Local { .. } => None,
        }
    }

    pub fn url(&self) -> Option<String> {
        self.workshop_id()
            .map(|id| format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}"))
    }

    pub fn launchable(&self) -> bool {
        self.status == Status::Ok || self.status == Status::UpdateAvailable
    }
}

pub fn steam_id(id: u64) -> String {
    format!("steam:{id}")
}

/// The workshop id in a `steam:<id>` identifier.
pub fn workshop_number(id: &str) -> Option<u64> {
    id.strip_prefix("steam:")?.parse().ok()
}

/// Everything a scan needs.
pub struct ScanInput<'a> {
    pub steamapps: &'a Path,
    pub game_dir: &'a Path,
    pub drives: &'a DriveMap,
    pub local: &'a LocalMods,
    pub workshop_cache: &'a HashMap<u64, WorkshopInfo>,
    /// Publish timestamps (`meta.cpp`) seen at the previous start, for the "updated" tag.
    pub seen_timestamps: &'a HashMap<u64, u64>,
}

/// Scans installed workshop items and local mods, sorted by name.
pub fn scan(input: &ScanInput) -> Vec<Mod> {
    let mut mods = Vec::new();
    let workshop_root = input.steamapps.join("workshop/content").join(ARMA3_APP_ID);
    for entry in std::fs::read_dir(&workshop_root)
        .into_iter()
        .flatten()
        .flatten()
    {
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u64>().ok())
        else {
            continue;
        };
        mods.push(workshop_mod(id, entry.path(), input));
    }

    let mut seen_paths = HashSet::new();
    let mut local = |windows_path: &str, auto: bool| {
        let Some(path) = crate::vanilla::host_path(input.drives, windows_path) else {
            return;
        };
        if !seen_paths.insert(path.clone()) || !path.is_dir() {
            return;
        }
        let known = input
            .local
            .known_local_mods
            .iter()
            .any(|k| k.eq_ignore_ascii_case(windows_path));
        mods.push(local_mod(windows_path, path, auto && !known));
    };
    for dir in &input.local.user_directories {
        local(dir, false);
    }
    for watched in &input.local.autodetection_directories {
        let Some(root) = crate::vanilla::host_path(input.drives, watched) else {
            continue;
        };
        for folder in detect_mods(&root, input.game_dir) {
            local(&crate::vanilla::game_path(&folder), true);
        }
    }

    mods.sort_by_key(|m| m.name.to_lowercase());
    mods
}

fn workshop_mod(id: u64, path: PathBuf, input: &ScanInput) -> Mod {
    let cached = input.workshop_cache.get(&id);
    let meta = read_cpp(&path.join("meta.cpp"));
    let name = cached
        .map(|c| c.title.clone())
        .filter(|t| !t.is_empty())
        .or_else(|| meta.get("name").cloned())
        .or_else(|| read_cpp(&path.join("mod.cpp")).get("name").cloned())
        .unwrap_or_else(|| id.to_string());
    let timestamp = meta.get("timestamp").and_then(|t| t.parse::<u64>().ok());
    let updated = match (timestamp, input.seen_timestamps.get(&id)) {
        (Some(now), Some(before)) => now != *before,
        _ => false,
    };
    Mod {
        id: steam_id(id),
        source: Source::Workshop(id),
        name,
        author: cached.map(|c| c.author.clone()).unwrap_or_default(),
        dependencies: cached
            .map(|c| c.dependencies.iter().map(|d| steam_id(*d)).collect())
            .unwrap_or_default(),
        status: if is_intact(&path) {
            Status::Ok
        } else {
            Status::Corrupted
        },
        path,
        updated,
        auto_included: false,
        last_update: cached.map(|c| c.last_update).filter(|t| *t > 0),
    }
}

pub fn local_mod(windows_path: &str, path: PathBuf, auto_included: bool) -> Mod {
    let cpp = read_cpp(&path.join("mod.cpp"));
    let name = cpp
        .get("name")
        .cloned()
        .unwrap_or_else(|| folder_display_name(&path));
    Mod {
        id: local_mod_id(windows_path),
        source: Source::Local {
            windows_path: windows_path.to_owned(),
        },
        name,
        author: cpp.get("author").cloned().unwrap_or_default(),
        dependencies: Vec::new(),
        status: if is_intact(&path) {
            Status::Ok
        } else {
            Status::Corrupted
        },
        path,
        updated: false,
        auto_included,
        last_update: None,
    }
}

/// The folder name as the vanilla launcher shows a mod without `mod.cpp` name: `.pbo` removed,
/// `_` as spaces, leading `@` trimmed.
fn folder_display_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.replace(".pbo", "")
        .replace('_', " ")
        .trim_start_matches('@')
        .to_owned()
}

/// `ModIntegrityChecker`: the folder holds an `addons` folder (any case) with a `.pbo` or `.ebo`.
pub fn is_intact(path: &Path) -> bool {
    let Some(addons) = crate::fs_case::resolve(path, "addons") else {
        return false;
    };
    std::fs::read_dir(addons)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.ends_with(".pbo") || name.ends_with(".ebo")
        })
}

/// The mod's short hash, what servers send for each mod (`ModHashCalculator`): the trailing
/// 20-byte SHA-1 of every `*.pbo` in `dta`, `addons` and `bin`, sorted, hex-encoded and
/// concatenated, then SHA-1 again; the short hash is the RS hash of the hex digest, prefixed with
/// its length. `None` when a folder can't be read or a PBO is shorter than its trailer.
pub fn short_hash(path: &Path) -> Option<u32> {
    use std::io::{Read, Seek, SeekFrom};
    let mut trailers: Vec<[u8; 20]> = Vec::new();
    for dir in ["dta", "addons", "bin"] {
        let Some(dir) = crate::fs_case::resolve(path, dir).filter(|d| d.is_dir()) else {
            continue;
        };
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let name = entry.file_name();
            let is_pbo = Path::new(&name)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pbo"));
            if !is_pbo || !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let mut file = std::fs::File::open(entry.path()).ok()?;
            file.seek(SeekFrom::End(-20)).ok()?;
            let mut trailer = [0; 20];
            file.read_exact(&mut trailer).ok()?;
            trailers.push(trailer);
        }
    }
    trailers.sort_unstable();
    let full: Vec<u8> = trailers.iter().flat_map(|t| hex(t)).collect();
    Some(rs_hash(&hex(&sha1_digest(&full))))
}

fn sha1_digest(data: &[u8]) -> [u8; 20] {
    use sha1::Digest;
    sha1::Sha1::digest(data).into()
}

/// Lowercase hex digits as bytes.
fn hex(bytes: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|b| [DIGITS[(b >> 4) as usize], DIGITS[(b & 0xF) as usize]])
        .collect()
}

/// `RsHashAlgorithm` over the length byte and then `data`.
fn rs_hash(data: &[u8]) -> u32 {
    let (mut hash, mut a) = (0u32, 378551u32);
    for &byte in std::iter::once(&(data.len() as u8)).chain(data) {
        hash = hash.wrapping_mul(a).wrapping_add(u32::from(byte));
        a = a.wrapping_mul(63689);
    }
    hash
}

/// Mod folders under a watched folder: parents of `addons` folders holding PBOs, excluding the
/// game itself (`DefaultModDiscoveryStrategy`).
fn detect_mods(root: &Path, game_dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > 6 {
            continue;
        }
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name == "!workshop" {
                continue;
            }
            if name == "addons" {
                if dir != game_dir && is_intact(&dir) {
                    found.push(dir.clone());
                }
            } else {
                stack.push((path, depth + 1));
            }
        }
    }
    found
}

/// `key = "value";` and `key = value;` pairs of a `mod.cpp` / `meta.cpp`, keys lower-cased.
pub fn read_cpp(path: &Path) -> HashMap<String, String> {
    let Ok(text) = std::fs::read(path) else {
        return HashMap::new();
    };
    String::from_utf8_lossy(&text)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = key.trim().to_ascii_lowercase();
            let value = value
                .trim()
                .trim_end_matches(';')
                .trim()
                .trim_matches('"')
                .to_owned();
            (!key.is_empty() && !key.contains(char::is_whitespace)).then_some((key, value))
        })
        .collect()
}

/// `loaded` reordered so every mod comes after the loaded mods it depends on; otherwise the
/// user's order is kept (`DependencyManager`). Cycles are broken by the original order.
pub fn dependency_order(loaded: &[String], mods: &HashMap<&str, &Mod>) -> Vec<String> {
    let set: HashSet<&str> = loaded.iter().map(String::as_str).collect();
    let mut placed = HashSet::new();
    let mut visiting = HashSet::new();
    let mut out = Vec::new();
    fn visit<'a>(
        id: &'a str,
        set: &HashSet<&str>,
        mods: &HashMap<&str, &'a Mod>,
        placed: &mut HashSet<&'a str>,
        visiting: &mut HashSet<&'a str>,
        out: &mut Vec<String>,
    ) {
        if placed.contains(id) || !visiting.insert(id) {
            return;
        }
        if let Some(m) = mods.get(id) {
            for dep in &m.dependencies {
                if set.contains(dep.as_str()) {
                    visit(dep, set, mods, placed, visiting, out);
                }
            }
        }
        visiting.remove(id);
        if placed.insert(id) {
            out.push(id.to_owned());
        }
    }
    for id in loaded {
        visit(id, &set, mods, &mut placed, &mut visiting, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, deps: &[&str]) -> Mod {
        Mod {
            id: id.into(),
            source: Source::Workshop(0),
            name: id.into(),
            author: String::new(),
            path: PathBuf::new(),
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
            status: Status::Ok,
            updated: false,
            auto_included: false,
            last_update: None,
        }
    }

    #[test]
    fn dependencies_come_first_and_user_order_is_kept() {
        let all = [
            m("a", &["c"]),
            m("b", &[]),
            m("c", &["d"]),
            m("d", &[]),
            m("e", &["a"]),
        ];
        let index: HashMap<&str, &Mod> = all.iter().map(|m| (m.id.as_str(), m)).collect();
        let loaded: Vec<String> = ["b", "a", "c", "d"].map(String::from).to_vec();
        assert_eq!(dependency_order(&loaded, &index), ["b", "d", "c", "a"]);
    }

    #[test]
    fn cycles_do_not_loop() {
        let all = [m("a", &["b"]), m("b", &["a"])];
        let index: HashMap<&str, &Mod> = all.iter().map(|m| (m.id.as_str(), m)).collect();
        assert_eq!(
            dependency_order(&["a".into(), "b".into()], &index),
            ["b", "a"]
        );
    }

    #[test]
    fn reads_cpp_pairs_and_folder_names() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp/mod-cpp/@My_Mod");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Addons")).unwrap();
        std::fs::write(
            dir.join("mod.cpp"),
            "name = \"My Mod\";\r\nauthor=\"Me\";\nactionName = \"Website\";\n",
        )
        .unwrap();
        let cpp = read_cpp(&dir.join("mod.cpp"));
        assert_eq!(cpp.get("name").map(String::as_str), Some("My Mod"));
        assert_eq!(cpp.get("author").map(String::as_str), Some("Me"));
        assert_eq!(folder_display_name(&dir), "My Mod");
        assert!(!is_intact(&dir));
        std::fs::write(dir.join("Addons/x.pbo"), b"").unwrap();
        assert!(is_intact(&dir));
    }

    #[test]
    fn short_hash_covers_pbo_trailers_in_any_order_and_case() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp/mod-hash");
        let _ = std::fs::remove_dir_all(&root);
        let pbo = |n: u8| [vec![0xAB; 7], vec![n; 20]].concat();
        for (mod_dir, a, b) in [("one", "Addons", "bin"), ("two", "addons", "BIN")] {
            let dir = root.join(mod_dir);
            std::fs::create_dir_all(dir.join(a)).unwrap();
            std::fs::create_dir_all(dir.join(b)).unwrap();
            std::fs::write(dir.join(a).join("a.pbo"), pbo(1)).unwrap();
            std::fs::write(dir.join(a).join("a.pbo.bisign"), pbo(9)).unwrap();
            std::fs::write(dir.join(b).join("b.PBO"), pbo(2)).unwrap();
        }
        // The same trailers, swapped between folders.
        std::fs::write(root.join("two/addons/a.pbo"), pbo(2)).unwrap();
        std::fs::write(root.join("two/BIN/b.PBO"), pbo(1)).unwrap();
        let one = short_hash(&root.join("one"));
        assert_eq!(one, Some(0x6AD9_95CC));
        assert_eq!(short_hash(&root.join("two")), one);
        std::fs::write(root.join("two/addons/c.pbo"), b"short").unwrap();
        assert_eq!(short_hash(&root.join("two")), None);
        std::fs::remove_dir_all(root).unwrap();
    }
}
