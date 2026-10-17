//! Mod presets (`Presets/<name>.preset2`), byte-compatible with the vanilla launcher's
//! `FileBasedPresetManager`: XML, UTF-8 with BOM, CRLF, 2-space indent.

use std::io;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

/// The preset holding the loaded mods and DLC, written when the launcher exits.
pub const DEFAULT_PRESET_FILE: &str = "arma3.defaultpreset2";
const EXTENSION: &str = "preset2";
const BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preset {
    /// `last-update`, kept verbatim (XmlConvert round-trip format, local offset).
    pub last_update: String,
    /// Addon identifiers in load order: `steam:<id>` or `local:<WINDOWS PATH>\`.
    pub mods: Vec<String>,
    /// DLC app ids in load order.
    pub dlcs: Vec<u32>,
}

impl Preset {
    pub fn new(mods: Vec<String>, dlcs: Vec<u32>) -> Self {
        Self {
            last_update: now(),
            mods,
            dlcs,
        }
    }

    /// `None` when the document isn't a preset or has no `last-update` (vanilla skips those).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes.strip_prefix(BOM).unwrap_or(bytes)).ok()?;
        let mut reader = Reader::from_str(text);
        let mut path: Vec<String> = Vec::new();
        let mut preset = Preset::default();
        let mut has_last_update = false;
        loop {
            match reader.read_event().ok()? {
                Event::Start(e) => path.push(e.name().as_ref().to_owned()),
                Event::End(_) => {
                    path.pop();
                }
                Event::Text(t) => {
                    let value = t.xml_content(XmlVersion::Implicit1_0).trim().to_owned();
                    match path
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .as_slice()
                    {
                        ["addons-presets", "last-update"] => {
                            has_last_update = true;
                            preset.last_update = value;
                        }
                        ["addons-presets", "published-ids", "id"] if !value.is_empty() => {
                            preset.mods.push(value)
                        }
                        ["addons-presets", "dlcs-appids", "id"] => {
                            preset.dlcs.extend(value.parse::<u32>().ok())
                        }
                        _ => {}
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        has_last_update.then_some(preset)
    }

    /// The file contents as `XDocument.Save` writes them.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut xml =
            String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<addons-presets>\r\n");
        xml += &format!(
            "  <last-update>{}</last-update>\r\n",
            escape(&self.last_update)
        );
        list(
            &mut xml,
            "published-ids",
            self.mods.iter().map(|m| escape(m)),
        );
        list(
            &mut xml,
            "dlcs-appids",
            self.dlcs.iter().map(u32::to_string),
        );
        xml += "</addons-presets>";
        [BOM, xml.as_bytes()].concat()
    }

    pub fn load(path: &Path) -> Option<Self> {
        Self::parse(&std::fs::read(path).ok()?)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }
}

fn list(xml: &mut String, element: &str, items: impl Iterator<Item = String>) {
    let items: Vec<String> = items.collect();
    if items.is_empty() {
        *xml += &format!("  <{element} />\r\n");
        return;
    }
    *xml += &format!("  <{element}>\r\n");
    for item in items {
        *xml += &format!("    <id>{item}</id>\r\n");
    }
    *xml += &format!("  </{element}>\r\n");
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// `DateTime.Now` in XmlConvert's round-trip format: 7 fractional digits and the local offset.
fn now() -> String {
    use chrono::Timelike;
    let now = chrono::Local::now();
    let ticks = now.nanosecond() % 1_000_000_000 / 100;
    format!(
        "{}.{ticks:07}{}",
        now.format("%Y-%m-%dT%H:%M:%S"),
        now.format("%:z")
    )
}

/// Named presets in `dir`, sorted by name (the default preset is not one of them).
pub fn named_presets(dir: &Path) -> Vec<(String, Preset)> {
    let mut presets: Vec<(String, Preset)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_stem()?.to_str()?.to_owned();
            (path.extension()? == EXTENSION).then_some(())?;
            Some((name, Preset::load(&path)?))
        })
        .collect();
    presets.sort_by_key(|(name, _)| name.to_lowercase());
    presets
}

pub fn preset_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.{EXTENSION}"))
}

/// Vanilla's preset name rule (`PresetNameErrorHandling`): letters, digits and spaces, at most 30.
pub fn is_valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= 30
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    const WOG: &[u8] = include_bytes!("../../tests/fixtures/WOG.preset2");

    #[test]
    fn round_trips_a_real_vanilla_preset_byte_for_byte() {
        let preset = Preset::parse(WOG).unwrap();
        assert_eq!(preset.mods.len(), 14);
        assert_eq!(preset.mods[0], "steam:843577117");
        assert_eq!(
            preset.mods[13],
            r"local:Z:\HOME\SAMUEL\GAMES\ARMA3\WOGMODS\@TFAR\"
        );
        assert!(preset.dlcs.is_empty());
        assert_eq!(preset.to_bytes(), WOG);
    }

    #[test]
    fn writes_dlc_lists_and_requires_last_update() {
        let preset = Preset {
            last_update: "2026-10-17T14:53:26.2061751+02:00".into(),
            mods: vec![],
            dlcs: vec![1042220],
        };
        let text = String::from_utf8(preset.to_bytes()[3..].to_vec()).unwrap();
        assert!(text.contains(
            "  <published-ids />\r\n  <dlcs-appids>\r\n    <id>1042220</id>\r\n  </dlcs-appids>\r\n"
        ));
        assert_eq!(Preset::parse(&preset.to_bytes()), Some(preset));
        assert_eq!(
            Preset::parse(b"<addons-presets><published-ids /></addons-presets>"),
            None
        );
    }

    #[test]
    fn new_presets_stamp_the_vanilla_date_format() {
        let stamp = Preset::new(vec![], vec![]).last_update;
        // e.g. 2026-10-17T14:53:26.2061751+02:00
        assert_eq!(stamp.len(), 33, "{stamp}");
        assert_eq!(&stamp[19..20], ".");
    }

    #[test]
    fn validates_names_like_vanilla() {
        assert!(is_valid_name("TBD Event2"));
        assert!(!is_valid_name("bad/name"));
        assert!(!is_valid_name(&"x".repeat(31)));
    }
}
