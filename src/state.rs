//! The launcher's own state between runs (`state.json` in the state directory): things the
//! vanilla launcher keeps in files this launcher doesn't own. Linux uses `$XDG_STATE_HOME`;
//! Windows uses `%LOCALAPPDATA%\arma3-launcher`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct State {
    /// `meta.cpp` publish timestamps of workshop items at the last start (the "updated" tag).
    #[serde(default)]
    pub workshop_timestamps: HashMap<u64, u64>,
    /// Workshop items Steam reported as not mods (scenarios, compositions), kept out of the list.
    #[serde(default)]
    pub workshop_non_mods: Vec<u64>,
    /// The newest news post id at the last exit; newer posts are unread.
    #[serde(default)]
    pub news_seen_newest: Option<u64>,
    /// Posts still unread at the last exit.
    #[serde(default)]
    pub news_unread: Vec<u64>,
    /// The window's inner size at the last exit, shadow margin included.
    #[serde(default)]
    pub window_size: Option<[f32; 2]>,
}

fn path() -> Option<PathBuf> {
    crate::platform::state_dir().map(|dir| dir.join("state.json"))
}

impl State {
    pub fn load() -> Option<Self> {
        serde_json::from_slice(&std::fs::read(path()?).ok()?).ok()
    }

    pub fn save(&self) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
