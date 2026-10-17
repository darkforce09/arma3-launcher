//! Preset HTML files (`HtmlModListBuilder` / `ExtensionListSharer.LoadPresetFromFile`): the
//! shareable preset format, written as XML (BOM, CRLF, 2-space indent) and read as strict XML.

use quick_xml::Reader;
use quick_xml::events::Event;

const STEAM_ITEM_URL: &str = "https://steamcommunity.com/sharedfiles/filedetails/?id=";
const STEAM_APP_URL: &str = "https://store.steampowered.com/app/";

pub enum ExportMod<'a> {
    Workshop {
        name: &'a str,
        id: u64,
    },
    /// `meta_name|folder|url` per `LocalModDescriptor`.
    Local {
        name: &'a str,
        folder: &'a str,
        url: &'a str,
    },
}

pub struct ExportDlc<'a> {
    pub name: &'a str,
    pub app_id: u32,
}

/// XML escaping as `XDocument` writes text and double-quoted attributes (apostrophes stay).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `HtmlModListBuilder.GetHtmlStyle`, line breaks as the launcher writes them.
const STYLE: &str = "\r\nbody {\r\n\tmargin: 0;\r\n\tpadding: 0;\r\n\tcolor: #fff;\r\n\tbackground: #000;\t\r\n}\r\n\r\nbody, th, td {\r\n\tfont: 95%/1.3 Roboto, Segoe UI, Tahoma, Arial, Helvetica, sans-serif;\r\n}\r\n\r\ntd {\r\n    padding: 3px 30px 3px 0;\r\n}\r\n\r\nh1 {\r\n    padding: 20px 20px 0 20px;\r\n    color: white;\r\n    font-weight: 200;\r\n    font-family: segoe ui;\r\n    font-size: 3em;\r\n    margin: 0;\r\n}\r\n\r\nem {\r\n    font-variant: italic;\r\n    color:silver;\r\n}\r\n\r\n.before-list {\r\n    padding: 5px 20px 10px 20px;\r\n}\r\n\r\n.mod-list {\r\n    background: #222222;\r\n    padding: 20px;\r\n}\r\n\r\n.dlc-list {\r\n    background: #222222;\r\n    padding: 20px;\r\n}\r\n\r\n.footer {\r\n    padding: 20px;\r\n    color:gray;\r\n}\r\n\r\n.whups {\r\n    color:gray;\r\n}\r\n\r\na {\r\n    color: #D18F21;\r\n    text-decoration: underline;\r\n}\r\n\r\na:hover {\r\n    color:#F1AF41;\r\n    text-decoration: none;\r\n}\r\n\r\n.from-steam {\r\n    color: #449EBD;\r\n}\r\n.from-local {\r\n    color: gray;\r\n}\r\n\r\n";

/// The preset as an HTML file.
pub fn export(preset_name: &str, mods: &[ExportMod], dlcs: &[ExportDlc]) -> Vec<u8> {
    let mut l: Vec<String> = Vec::new();
    l.push(r#"<?xml version="1.0" encoding="utf-8"?>"#.into());
    l.push("<html>".into());
    l.push("  <!--Created by Arma 3 Launcher: https://arma3.com-->".into());
    l.push("  <head>".into());
    l.push(r#"    <meta name="arma:Type" content="preset" />"#.into());
    l.push(format!(
        r#"    <meta name="arma:PresetName" content="{}" />"#,
        esc(preset_name)
    ));
    l.push(r#"    <meta name="generator" content="Arma 3 Launcher - https://arma3.com" />"#.into());
    l.push("    <title>Arma 3</title>".into());
    l.push(r#"    <link href="https://fonts.googleapis.com/css?family=Roboto" rel="stylesheet" type="text/css" />"#.into());
    l.push(format!("    <style>{STYLE}</style>"));
    l.push("  </head>".into());
    l.push("  <body>".into());
    l.push(format!(
        "    <h1>Arma 3  - Preset <strong>{}</strong></h1>",
        esc(preset_name)
    ));
    l.push(r#"    <p class="before-list">"#.into());
    l.push("      <em>To import this preset, drag this file onto the Launcher window. Or click the MODS tab, then PRESET in the top right, then IMPORT at the bottom, and finally select this file.</em>".into());
    l.push("    </p>".into());
    l.push(r#"    <div class="mod-list">"#.into());
    l.push("      <table>".into());
    for m in mods {
        l.push(r#"        <tr data-type="ModContainer">"#.into());
        match m {
            ExportMod::Workshop { name, id } => {
                let url = format!("{STEAM_ITEM_URL}{id}");
                l.push(format!(
                    r#"          <td data-type="DisplayName">{}</td>"#,
                    esc(name)
                ));
                l.push("          <td>".into());
                l.push(r#"            <span class="from-steam">Steam</span>"#.into());
                l.push("          </td>".into());
                l.push("          <td>".into());
                l.push(format!(
                    r#"            <a href="{url}" data-type="Link">{url}</a>"#
                ));
                l.push("          </td>".into());
            }
            ExportMod::Local { name, folder, url } => {
                l.push(format!(
                    r#"          <td data-type="DisplayName">{}</td>"#,
                    esc(name)
                ));
                l.push("          <td>".into());
                l.push(r#"            <span class="from-local">Local</span>"#.into());
                l.push("          </td>".into());
                l.push("          <td>".into());
                let meta = esc(&format!("local:{name}|{folder}|{url}"));
                if url.is_empty() {
                    l.push(format!(
                        r#"            <span class="whups" data-type="Link" data-meta="{meta}" />"#
                    ));
                } else {
                    l.push(format!(
                        r#"            <span class="whups" data-type="Link" data-meta="{meta}">"#
                    ));
                    l.push(format!(r#"              <a href="{0}">{0}</a>"#, esc(url)));
                    l.push("            </span>".into());
                }
                l.push("          </td>".into());
            }
        }
        l.push("        </tr>".into());
    }
    l.push("      </table>".into());
    l.push("    </div>".into());
    l.push(r#"    <div class="dlc-list">"#.into());
    l.push("      <table>".into());
    for d in dlcs {
        let url = format!("{STEAM_APP_URL}{}", d.app_id);
        l.push(r#"        <tr data-type="DlcContainer">"#.into());
        l.push(format!(
            r#"          <td data-type="DisplayName">{}</td>"#,
            esc(d.name)
        ));
        l.push("          <td>".into());
        l.push(format!(
            r#"            <a href="{url}" data-type="Link">{url}</a>"#
        ));
        l.push("          </td>".into());
        l.push("        </tr>".into());
    }
    l.push("      </table>".into());
    l.push("    </div>".into());
    l.push(r#"    <div class="footer">"#.into());
    l.push("      <span>Created by Arma 3 Launcher by Bohemia Interactive.</span>".into());
    l.push("    </div>".into());
    l.push("  </body>".into());
    l.push("</html>".into());
    [b"\xEF\xBB\xBF".as_slice(), l.join("\r\n").as_bytes()].concat()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Imported {
    pub name: String,
    pub workshop_ids: Vec<u64>,
    /// The `DisplayName` of each workshop mod, in the same order (empty when missing).
    pub workshop_names: Vec<String>,
    /// `(meta name, folder name)` of local mods, to match against the library.
    pub local: Vec<(String, String)>,
    pub dlc_app_ids: Vec<u32>,
}

/// Reads a preset HTML file. `None` when it isn't well-formed XML (vanilla refuses those too).
pub fn import(bytes: &[u8]) -> Option<Imported> {
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).ok()?;
    let mut reader = Reader::from_str(text);
    let mut out = Imported::default();
    // The data-type of each open element, to know whether we're inside a container.
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut container_done = false;
    // The current mod's DisplayName text, and whether we're inside that element.
    let mut display_name = String::new();
    let mut in_display_name = false;
    loop {
        let event = reader.read_event().ok()?;
        let (start, empty) = match &event {
            Event::Start(e) => (Some(e.clone()), false),
            Event::Empty(e) => (Some(e.clone()), true),
            Event::Text(t) if in_display_name => {
                display_name.push_str(&t.xml10_content());
                continue;
            }
            Event::GeneralRef(r) if in_display_name => {
                let name = r.xml10_content();
                if let Some(c) = r.resolve_char_ref().ok().flatten() {
                    display_name.push(c);
                } else if let Some(s) = quick_xml::escape::resolve_predefined_entity(&name) {
                    display_name.push_str(s);
                }
                continue;
            }
            Event::End(_) => {
                if stack.pop().flatten().as_deref() == Some("DisplayName") {
                    in_display_name = false;
                }
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let Some(e) = start else { continue };
        let attr = |key: &str| {
            e.attributes()
                .flatten()
                .find(|a| a.key.as_ref() == key)
                .and_then(|a| {
                    a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .ok()
                        .map(|v| v.into_owned())
                })
        };
        let data_type = attr("data-type");
        let tag = e.name().as_ref().to_owned();
        if tag == "meta" && attr("name").as_deref() == Some("arma:PresetName") {
            out.name = attr("content").unwrap_or_default();
        }
        if matches!(data_type.as_deref(), Some("ModContainer" | "DlcContainer")) {
            container_done = false;
            display_name.clear();
        }
        if data_type.as_deref() == Some("DisplayName") && !empty {
            in_display_name = true;
        }
        let container = stack
            .iter()
            .rev()
            .flatten()
            .find(|t| t.as_str() == "ModContainer" || t.as_str() == "DlcContainer")
            .cloned();
        if data_type.as_deref() == Some("Link") && !container_done {
            match container.as_deref() {
                Some("ModContainer") => {
                    if let Some(id) = attr("href").as_deref().and_then(workshop_id) {
                        out.workshop_ids.push(id);
                        out.workshop_names.push(display_name.trim().to_owned());
                    } else if let Some(meta) = attr("data-meta") {
                        let meta = meta.strip_prefix("local:").unwrap_or(&meta);
                        let mut parts = meta.split('|');
                        let name = parts.next().unwrap_or_default().to_owned();
                        let folder = parts.next().unwrap_or_default().to_owned();
                        out.local.push((name, folder));
                    }
                    container_done = true;
                }
                Some("DlcContainer") => {
                    if let Some(id) = attr("href")
                        .and_then(|h| h.strip_prefix(STEAM_APP_URL).and_then(|r| r.parse().ok()))
                    {
                        out.dlc_app_ids.push(id);
                    }
                    container_done = true;
                }
                _ => {}
            }
        }
        if !empty {
            stack.push(data_type);
        }
    }
    if out.name.is_empty() {
        out.name = "Imported preset".into();
    }
    Some(out)
}

/// The workshop id in a Steam item URL or a `steam:<id>` reference.
fn workshop_id(href: &str) -> Option<u64> {
    let digits = href
        .split_once("filedetails/?id=")
        .map(|(_, rest)| rest)
        .or_else(|| href.strip_prefix("steam:"))?;
    digits
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_presets_import_back() {
        let html = export(
            "TBD Event",
            &[
                ExportMod::Workshop {
                    name: "CBA & A3",
                    id: 450814997,
                },
                ExportMod::Local {
                    name: "TFAR & co",
                    folder: "@TFAR",
                    url: "",
                },
            ],
            &[ExportDlc {
                name: "Global Mobilization",
                app_id: 1042220,
            }],
        );
        let text = String::from_utf8(html[3..].to_vec()).unwrap();
        assert!(text.contains("<h1>Arma 3  - Preset <strong>TBD Event</strong></h1>\r\n"));
        let imported = import(&html).unwrap();
        assert_eq!(
            imported,
            Imported {
                name: "TBD Event".into(),
                workshop_ids: vec![450814997],
                workshop_names: vec!["CBA & A3".into()],
                local: vec![("TFAR & co".into(), "@TFAR".into())],
                dlc_app_ids: vec![1042220],
            }
        );
    }

    #[test]
    fn rejects_html_that_is_not_xml() {
        assert_eq!(import(b"<html><body><p>unclosed</body></html>"), None);
    }
}
