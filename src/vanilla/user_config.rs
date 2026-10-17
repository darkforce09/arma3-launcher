//! The vanilla launcher's .NET `user.config`: parameter values, options and the last preset.
//!
//! It lives at `AppData/Local/Bohemia_Interactive/arma3launcher.exe_Url_<hash>/<version>/`, where
//! the hash depends on the path the launcher was started from, so several can exist. Edits
//! rewrite only the touched `<value>`s and keep the rest of the file byte for byte.

use std::io;
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion};

pub const SETTINGS: &str = "Launcher.Properties.Settings";
pub const PRESET_SETTINGS: &str = "Launcher.Properties.PresetSettings";

pub struct UserConfig {
    path: PathBuf,
    text: String,
}

impl UserConfig {
    /// The `user.config` the vanilla launcher uses: the newest one that holds real settings
    /// (a launcher started from an unusual path leaves one with only `IsUpgradeNeeded`).
    pub fn find(local_app_data: &Path) -> Option<Self> {
        let root = local_app_data.join("Bohemia_Interactive");
        let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(root)
            .ok()?
            .flatten()
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("arma3launcher.exe_")
            })
            .flat_map(|e| std::fs::read_dir(e.path()).into_iter().flatten().flatten())
            .map(|version| version.path().join("user.config"))
            .filter_map(|path| Some((path.metadata().ok()?.modified().ok()?, path)))
            .collect();
        candidates.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        candidates.into_iter().find_map(|(_, path)| {
            let config = Self::load(&path).ok()?;
            config.has_real_settings().then_some(config)
        })
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            text: std::fs::read_to_string(path)?,
        })
    }

    fn has_real_settings(&self) -> bool {
        self.settings()
            .iter()
            .any(|(_, name, _)| name != "IsUpgradeNeeded")
    }

    /// The value of setting `name` in `section`.
    pub fn get(&self, section: &str, name: &str) -> Option<String> {
        self.settings()
            .into_iter()
            .find(|(s, n, _)| s == section && n == name)
            .map(|(_, _, v)| v)
    }

    /// All (section, name, value) triples.
    pub fn settings(&self) -> Vec<(String, String, String)> {
        let mut reader = Reader::from_str(&self.text);
        let mut out = Vec::new();
        let mut section = String::new();
        let mut setting: Option<String> = None;
        let mut in_value = false;
        let mut value = String::new();
        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    let name = e.name().as_ref().to_owned();
                    if name == "setting" {
                        setting = attribute(&e, "name");
                    } else if name == "value" && setting.is_some() {
                        in_value = true;
                        value.clear();
                    } else if name.starts_with("Launcher.") {
                        section = name;
                    }
                }
                Ok(Event::Empty(e)) if e.name().as_ref() == "value" => {
                    if let Some(name) = &setting {
                        out.push((section.clone(), name.clone(), String::new()));
                    }
                }
                Ok(Event::Text(t)) if in_value => value += &t.xml_content(XmlVersion::Implicit1_0),
                Ok(Event::GeneralRef(r)) if in_value => {
                    value += &r
                        .resolve_char_ref()
                        .ok()
                        .flatten()
                        .map(String::from)
                        .unwrap_or_else(|| {
                            match r.as_ref() {
                                "amp" => "&",
                                "lt" => "<",
                                "gt" => ">",
                                "quot" => "\"",
                                "apos" => "'",
                                _ => "",
                            }
                            .to_owned()
                        })
                }
                Ok(Event::End(e)) => match e.name().as_ref() {
                    "value" if in_value => {
                        in_value = false;
                        if let Some(name) = &setting {
                            out.push((section.clone(), name.clone(), value.clone()));
                        }
                    }
                    "setting" => setting = None,
                    _ => {}
                },
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        out
    }

    /// Sets `name` in `section` to `value`, adding the setting at the end of the section when
    /// missing.
    pub fn set(&mut self, section: &str, name: &str, value: &str) {
        if self.get(section, name).as_deref() == Some(value) {
            return;
        }
        if let Some(text) = rewrite(&self.text, section, name, value) {
            self.text = text;
        }
    }

    pub fn save(&self) -> io::Result<()> {
        std::fs::write(&self.path, &self.text)
    }
}

fn attribute(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(|v| v.into_owned())
        })
}

/// Re-emits `text`, replacing the setting's value or inserting the setting before the section's
/// end tag.
fn rewrite(text: &str, section: &str, name: &str, value: &str) -> Option<String> {
    let mut reader = Reader::from_str(text);
    let mut writer = Writer::new(Vec::new());
    let mut current_section = String::new();
    let mut in_target = false;
    let mut in_value = false;
    let mut done = false;
    loop {
        let event = reader.read_event().ok()?;
        match &event {
            Event::Start(e) => {
                let tag = e.name().as_ref().to_owned();
                if tag.starts_with("Launcher.") {
                    current_section = tag;
                } else if tag == "setting" {
                    in_target =
                        current_section == section && attribute(e, "name").as_deref() == Some(name);
                } else if tag == "value" && in_target {
                    in_value = true;
                    writer.write_event(event.clone()).ok()?;
                    writer
                        .write_event(Event::Text(BytesText::new(value)))
                        .ok()?;
                    done = true;
                    continue;
                }
            }
            Event::Empty(e) if in_target && e.name().as_ref() == "value" => {
                writer
                    .write_event(Event::Start(BytesStart::new("value")))
                    .ok()?;
                writer
                    .write_event(Event::Text(BytesText::new(value)))
                    .ok()?;
                writer
                    .write_event(Event::End(BytesEnd::new("value")))
                    .ok()?;
                done = true;
                continue;
            }
            Event::Text(_) | Event::GeneralRef(_) if in_value => continue,
            Event::End(e) => {
                let tag = e.name().as_ref().to_owned();
                if tag == "value" {
                    in_value = false;
                } else if tag == "setting" {
                    in_target = false;
                } else if tag == section && !done {
                    // Same layout as .NET writes: a 2-space-indented setting block.
                    let block = format!(
                        "  <setting name=\"{name}\" serializeAs=\"String\">\r\n    <value>{}</value>\r\n  </setting>\r\n",
                        quick_xml::escape::escape(value)
                    );
                    writer.get_mut().extend_from_slice(block.as_bytes());
                    done = true;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        writer.write_event(event).ok()?;
    }
    String::from_utf8(writer.into_inner()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../../tests/fixtures/user.config");

    fn config(text: &str) -> UserConfig {
        UserConfig {
            path: PathBuf::new(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn reads_settings_from_the_real_file() {
        let c = config(REAL);
        assert_eq!(
            c.get(PRESET_SETTINGS, "LastActivePreset").as_deref(),
            Some("ScudHunters")
        );
        assert!(c.has_real_settings());
    }

    #[test]
    fn unchanged_values_leave_the_file_untouched() {
        let mut c = config(REAL);
        c.set(PRESET_SETTINGS, "LastActivePreset", "ScudHunters");
        assert_eq!(c.text, REAL);
        // A full rewrite with the same value is byte-identical too.
        assert_eq!(
            rewrite(REAL, PRESET_SETTINGS, "LastActivePreset", "ScudHunters").unwrap(),
            REAL
        );
    }

    #[test]
    fn replaces_and_inserts_values() {
        let mut c = config(REAL);
        c.set(PRESET_SETTINGS, "LastActivePreset", "WOG & co");
        assert_eq!(
            c.get(PRESET_SETTINGS, "LastActivePreset").as_deref(),
            Some("WOG & co")
        );
        c.set(SETTINGS, "BrandNewSetting", "True");
        assert_eq!(c.get(SETTINGS, "BrandNewSetting").as_deref(), Some("True"));
        // Everything else is unchanged.
        let before = config(REAL).settings();
        let after = c.settings();
        assert_eq!(after.len(), before.len() + 1);
        assert!(
            before
                .iter()
                .filter(|(_, n, _)| n != "LastActivePreset")
                .all(|s| after.contains(s))
        );
    }
}
