//! Files the vanilla launcher keeps in its data folder and the Proton prefix.

pub mod local_mods;
pub mod preset;
pub mod preset_html;
pub mod user_config;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Misc {
    #[serde(default)]
    subscribed_app_ids: Vec<u32>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok()
}

/// App ids the user owns (`Misc.json`, the vanilla launcher's cache of Steam ownership).
pub fn owned_app_ids(launcher_data: &Path) -> Vec<u32> {
    read_json::<Misc>(&launcher_data.join("Misc.json"))
        .map(|misc| misc.subscribed_app_ids)
        .unwrap_or_default()
}

/// Workshop metadata the vanilla launcher cached from Steam (`Steam.json`).
#[derive(Clone, Debug, Default)]
pub struct WorkshopInfo {
    pub title: String,
    pub author: String,
    pub dependencies: Vec<u64>,
    /// Last update on the workshop, Unix seconds.
    pub last_update: i64,
    /// The `meta.cpp` publish timestamp when vanilla last synced the item.
    pub publish_timestamp: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SteamCache {
    #[serde(default)]
    extensions: Vec<SteamCacheEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SteamCacheEntry {
    id: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    steam_dependencies: Vec<u64>,
    #[serde(default)]
    storage_info: Option<SteamCacheStorage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SteamCacheStorage {
    #[serde(default)]
    last_update: i64,
    #[serde(default)]
    publish_timestamp: u64,
}

pub fn workshop_cache(launcher_data: &Path) -> HashMap<u64, WorkshopInfo> {
    let Some(cache) = read_json::<SteamCache>(&launcher_data.join("Steam.json")) else {
        return HashMap::new();
    };
    cache
        .extensions
        .into_iter()
        .filter_map(|e| {
            let id = e.id.strip_prefix("steam:")?.parse().ok()?;
            Some((
                id,
                WorkshopInfo {
                    title: e.display_name,
                    author: e.author.unwrap_or_default(),
                    dependencies: e.steam_dependencies,
                    last_update: e
                        .storage_info
                        .as_ref()
                        .map(|s| s.last_update)
                        .unwrap_or_default(),
                    publish_timestamp: e
                        .storage_info
                        .as_ref()
                        .map(|s| s.publish_timestamp)
                        .unwrap_or_default(),
                },
            ))
        })
        .collect()
}

/// Maps Windows paths in the vanilla launcher's files back to Linux paths: `Z:` is `/`, `S:` is
/// the Steam root Proton maps for the game, other drives follow the prefix's `dosdevices`.
///
/// On Windows the host path is the path itself, so this map is only used by tests.
#[cfg_attr(windows, allow(dead_code))]
pub struct DriveMap {
    pub dosdevices: PathBuf,
    pub steam_root: PathBuf,
}

impl DriveMap {
    /// Linux path for a path stored by the vanilla launcher. Used on Linux, and by tests.
    #[cfg_attr(windows, allow(dead_code))]
    pub fn to_unix(&self, windows: &str) -> Option<PathBuf> {
        let (drive, rest) = windows.split_once(':')?;
        let rest = rest.replace('\\', "/");
        let rest = rest.trim_start_matches('/');
        let root = match drive.to_ascii_lowercase().as_str() {
            "z" => PathBuf::from("/"),
            "s" => self.steam_root.clone(),
            d => std::fs::read_link(self.dosdevices.join(format!("{d}:")))
                .ok()
                .map(|target| {
                    if target.is_absolute() {
                        target
                    } else {
                        self.dosdevices.join(target)
                    }
                })?,
        };
        Some(root.join(rest))
    }
}

/// A Linux path as the game sees it under Wine: on the `Z:` drive, with backslashes.
/// Used on Linux, and by tests.
#[cfg_attr(windows, allow(dead_code))]
pub fn wine_path(path: &Path) -> String {
    format!("Z:{}", path.to_string_lossy().replace('/', "\\"))
}

/// A host path as the game and the vanilla launcher store it.
///
/// Linux keeps the `Z:` form. Windows is already the path the game uses: backslashes, no `\\?`
/// prefix, and no extra trailing slash (`local_mod_id` adds the one vanilla expects).
pub fn game_path(path: &Path) -> String {
    #[cfg(windows)]
    {
        let mut text = path.to_string_lossy().replace('/', "\\");
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            text = rest.to_owned();
        }
        if text.len() > 3 && text.ends_with('\\') {
            text.pop();
        }
        text
    }
    #[cfg(not(windows))]
    {
        wine_path(path)
    }
}

/// A path stored by the vanilla launcher, as a host path.
///
/// Linux maps `Z:` to `/`, `S:` to the Steam root, and other letters through `dosdevices`.
/// Windows uses the drive or UNC path itself. A `Z:` path written on Linux is a `Z:` drive here,
/// which does not exist.
pub fn host_path(drives: &DriveMap, windows: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let _ = drives;
        let path = PathBuf::from(windows.trim());
        path.is_absolute().then_some(path)
    }
    #[cfg(not(windows))]
    {
        drives.to_unix(windows)
    }
}

/// The vanilla identifier of a local mod: `local:` + the upper-cased Windows path with a
/// trailing backslash (`LocalAddonIdentifier`).
pub fn local_mod_id(windows_path: &str) -> String {
    let mut id = format!("local:{}", windows_path.to_uppercase());
    if !id.ends_with('\\') {
        id.push('\\');
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_drives_and_builds_ids() {
        let map = DriveMap {
            dosdevices: PathBuf::from("/nonexistent"),
            steam_root: PathBuf::from("/home/u/.local/share/Steam"),
        };
        assert_eq!(
            map.to_unix(r"Z:\home\u\Mods\@TFAR"),
            Some(PathBuf::from("/home/u/Mods/@TFAR"))
        );
        assert_eq!(
            map.to_unix(r"S:\steamapps\workshop\content\107410\1"),
            Some(PathBuf::from(
                "/home/u/.local/share/Steam/steamapps/workshop/content/107410/1"
            ))
        );
        assert_eq!(map.to_unix(r"Q:\x"), None);
        assert_eq!(wine_path(Path::new("/home/u/@TFAR")), r"Z:\home\u\@TFAR");
        #[cfg(windows)]
        {
            assert_eq!(
                game_path(Path::new(r"C:\Users\A\@TFAR")),
                r"C:\Users\A\@TFAR"
            );
            assert_eq!(
                game_path(Path::new(r"C:\Users\A\@TFAR\")),
                r"C:\Users\A\@TFAR"
            );
            assert_eq!(
                host_path(&map, r"C:\Users\A\@TFAR"),
                Some(PathBuf::from(r"C:\Users\A\@TFAR"))
            );
            assert!(host_path(&map, r"\\server\share\@TFAR").is_some());
        }
        #[cfg(not(windows))]
        {
            assert_eq!(game_path(Path::new("/home/u/@TFAR")), r"Z:\home\u\@TFAR");
            assert_eq!(
                host_path(&map, r"Z:\home\u\Mods\@TFAR"),
                Some(PathBuf::from("/home/u/Mods/@TFAR"))
            );
        }
        assert_eq!(
            local_mod_id(r"Z:\home\Samuel\Games\Arma3\WogMods\@TFAR"),
            r"local:Z:\HOME\SAMUEL\GAMES\ARMA3\WOGMODS\@TFAR\"
        );
    }
}
