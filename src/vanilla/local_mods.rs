//! `Local.json`: local mod folders and watched folders (`LocalExtensionStorageDataDto`, written
//! by .NET's DataContractJsonSerializer: compact, keys in alphabetical order, `/` escaped).

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMods {
    /// Watched folders, scanned for mods (Windows paths).
    #[serde(default)]
    pub autodetection_directories: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date_created: String,
    /// Every local mod known at the last save; mods outside it are "auto-included".
    #[serde(default)]
    pub known_local_mods: Vec<String>,
    /// Folders added as mods (Windows paths).
    #[serde(default)]
    pub user_directories: Vec<String>,
}

impl LocalMods {
    pub fn load(launcher_data: &Path) -> Self {
        std::fs::read(launcher_data.join("Local.json"))
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok()
            })
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        // Only strings contain '/', so escaping every one matches DataContractJsonSerializer.
        serde_json::to_string(self)
            .unwrap_or_default()
            .replace('/', "\\/")
    }

    pub fn save(&self, launcher_data: &Path) -> io::Result<()> {
        std::fs::write(launcher_data.join("Local.json"), self.to_json())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../../tests/fixtures/Local.json");

    #[test]
    fn round_trips_the_real_file() {
        let mods: LocalMods = serde_json::from_str(REAL).unwrap();
        assert_eq!(mods.user_directories.len(), 8);
        assert_eq!(
            mods.user_directories[0],
            r"Z:\home\Samuel\Games\Arma3\WogMods\@TFAR"
        );
        assert_eq!(mods.to_json(), REAL);
    }
}
