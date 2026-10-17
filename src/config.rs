//! The vanilla launcher's `Launcher/Config.json` (`appSettings`): web links and service URLs.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct LauncherConfig {
    #[serde(rename = "appSettings", default)]
    app_settings: HashMap<String, serde_json::Value>,
}

impl LauncherConfig {
    pub fn load(game_dir: &Path) -> Self {
        crate::fs_case::resolve(game_dir, "Launcher/Config.json")
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| {
                serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok()
            })
            .unwrap_or_default()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.app_settings
            .get(key)
            .and_then(serde_json::Value::as_str)
    }

    /// Opens the URL setting `key` in the default browser.
    pub fn open_url(&self, key: &str) {
        if let Some(url) = self.get(key) {
            let _ = open::that_detached(url);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_app_settings() {
        let config: LauncherConfig = serde_json::from_str(
            r#"{"appSettings":{"DevArma3Url":"https://dev.arma3.com/","miniDumpDirName":"x"}}"#,
        )
        .unwrap();
        assert_eq!(config.get("DevArma3Url"), Some("https://dev.arma3.com/"));
        assert_eq!(config.get("Missing"), None);
    }
}
