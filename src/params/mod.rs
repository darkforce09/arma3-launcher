//! Startup parameters: definitions, values (the vanilla `user.config`), favourites
//! (`Parameters.json`) and the command line they produce.

mod defs;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use defs::DEFS;

use crate::vanilla::user_config::{SETTINGS, UserConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `-name` when enabled.
    Basic,
    /// `-name=value`, the bare value without a name, or raw text.
    TextValue,
    IntegerValue,
    /// `-exThreads=N`, N a bit mask edited as three checkboxes.
    ExtraThreads,
    DropDown,
    /// A dropdown that also accepts typed values.
    Combo,
    File,
    Folder,
    /// Folders separated by `;`.
    FolderList,
}

#[derive(Debug)]
pub struct Def {
    pub kind: Kind,
    pub group: &'static str,
    /// The `user.config` setting that turns the parameter on.
    pub enabled: &'static str,
    /// The `user.config` setting holding its value.
    pub value: &'static str,
    /// The command-line name, without the dash.
    pub name: &'static str,
    /// The favourites key when it differs from the name (`SurrogateName`).
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub min: i64,
    pub max: i64,
    pub step: i64,
    /// The value goes on the command line verbatim.
    pub raw: bool,
    /// Always present, without a checkbox.
    pub always: bool,
    /// Not emitted (the executable choice).
    pub ignore: bool,
    /// File dialog filter patterns.
    pub filter: &'static str,
}

impl Def {
    /// The favourites key: `SurrogateName ?? CommandLineParameter`.
    pub fn key(&self) -> &'static str {
        if self.id.is_empty() {
            self.name
        } else {
            self.id
        }
    }

    /// Parameters this launcher doesn't offer: the unit (Arma 3 Units) and the launcher's own
    /// command line, which this launcher passes to the game directly.
    pub fn hidden(&self) -> bool {
        self.name == "unit" || self.id == "[commandLine]"
    }

    /// `-malloc` and `-maxmem` exist only for the 64-bit client.
    pub fn for_platform(&self, platform: &str) -> bool {
        !matches!(self.name, "malloc" | "maxmem") || platform.is_empty() || platform == "win64"
    }
}

pub const GROUPS: [(&str, &str); 6] = [
    ("basic", "Basic"),
    ("advanced", "Advanced"),
    ("client", "Client"),
    ("host", "Host"),
    ("author", "Author"),
    ("compatibility", "Compatibility"),
];

/// Parameter values: `user.config`, falling back to the defaults in `arma3launcher.exe.config`.
pub struct Settings {
    config: Option<UserConfig>,
    defaults: HashMap<String, String>,
    pending: HashMap<String, String>,
}

impl Settings {
    pub fn new(config: Option<UserConfig>, game_dir: &Path) -> Self {
        let defaults = crate::fs_case::resolve(game_dir, "arma3launcher.exe.config")
            .and_then(|path| UserConfig::load(&path).ok())
            .map(|c| {
                c.settings()
                    .into_iter()
                    .filter(|(s, _, _)| s == SETTINGS)
                    .map(|(_, n, v)| (n, v))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            config,
            defaults,
            pending: HashMap::new(),
        }
    }

    pub fn get(&self, name: &str) -> String {
        self.pending
            .get(name)
            .cloned()
            .or_else(|| self.config.as_ref().and_then(|c| c.get(SETTINGS, name)))
            .or_else(|| self.defaults.get(name).cloned())
            .unwrap_or_default()
    }

    pub fn flag(&self, name: &str) -> bool {
        self.get(name).eq_ignore_ascii_case("true")
    }

    pub fn set(&mut self, name: &str, value: impl Into<String>) {
        self.pending.insert(name.to_owned(), value.into());
    }

    pub fn set_flag(&mut self, name: &str, on: bool) {
        self.set(name, if on { "True" } else { "False" });
    }

    pub fn config_mut(&mut self) -> Option<&mut UserConfig> {
        self.config.as_mut()
    }

    /// Writes changed values to `user.config`.
    pub fn save(&mut self) -> std::io::Result<()> {
        let Some(config) = &mut self.config else {
            return Ok(());
        };
        for (name, value) in &self.pending {
            config.set(SETTINGS, name, value);
        }
        config.save()
    }

    pub fn enabled(&self, def: &Def) -> bool {
        def.always || (!def.enabled.is_empty() && self.flag(def.enabled))
    }

    /// The executable setting: `""` (64-bit client), `server`, `profiling` or `diagnostic`.
    pub fn platform(&self) -> String {
        match self.get("Platform").to_ascii_lowercase().as_str() {
            p @ ("server" | "profiling" | "diagnostic") => p.to_owned(),
            _ => String::new(),
        }
    }
}

/// The game executable for a platform setting.
pub fn platform_exe(platform: &str) -> &'static str {
    match platform {
        "server" => "arma3server_x64.exe",
        "profiling" => "arma3profiling_x64.exe",
        "diagnostic" => "arma3diag_x64.exe",
        _ => "arma3_x64.exe",
    }
}

/// The parameters as command-line tokens, `-mod` excluded (the caller merges it with the loaded
/// mods): each enabled parameter in definition order (`ParametersViewModel.GetCommandLine`).
/// Returns the tokens and the user's own `-mod` folders.
pub fn command_line(settings: &Settings) -> (Vec<String>, Vec<String>) {
    let platform = settings.platform();
    let mut tokens = Vec::new();
    let mut user_mods = Vec::new();
    for def in DEFS
        .iter()
        .filter(|d| !d.ignore && !d.hidden() && d.for_platform(&platform))
    {
        if !settings.enabled(def) {
            continue;
        }
        let value = if def.value.is_empty() {
            String::new()
        } else {
            settings.get(def.value)
        };
        match def.kind {
            Kind::Basic => tokens.push(format!("-{}", def.name)),
            _ if def.raw => tokens.extend(split_arguments(&value)),
            _ if def.name == "mod" => user_mods.extend(
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(String::from),
            ),
            _ if def.name.is_empty() => tokens.push(value),
            _ => tokens.push(format!("-{}={}", def.name, value)),
        }
    }
    (tokens, user_mods)
}

/// Splits raw text into arguments on whitespace outside double quotes, dropping the quotes that
/// wrap a whole argument or its value (`ArmaStyleCommandLineToArgsParser`, `CommandLineValue`).
pub fn split_arguments(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args.into_iter()
        .map(|a| {
            let a = a
                .strip_prefix('"')
                .and_then(|a| a.strip_suffix('"'))
                .map(str::to_owned)
                .unwrap_or(a);
            match a.split_once('=') {
                Some((name, value)) => {
                    let value = value
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .unwrap_or(value);
                    format!("{name}={value}")
                }
                None => a,
            }
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
struct FavoritesFile {
    favorites: Option<Vec<String>>,
}

/// The default favourites when `Parameters.json` has none.
const DEFAULT_FAVORITES: [&str; 6] = [
    "[Platform]",
    "skipIntro",
    "noSplash",
    "window",
    "name",
    "unit",
];

pub fn load_favorites(launcher_data: &Path) -> Vec<String> {
    std::fs::read(launcher_data.join("Parameters.json"))
        .ok()
        .and_then(|b| {
            serde_json::from_slice::<FavoritesFile>(b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&b))
                .ok()
        })
        .and_then(|f| f.favorites)
        .unwrap_or_else(|| DEFAULT_FAVORITES.map(String::from).to_vec())
}

pub fn save_favorites(launcher_data: &Path, favorites: &[String]) -> std::io::Result<()> {
    let json = serde_json::to_string(&FavoritesFile {
        favorites: Some(favorites.to_vec()),
    })?;
    std::fs::write(
        launcher_data.join("Parameters.json"),
        json.replace('/', "\\/"),
    )
}

/// Profile names (`ProfileLoader`): `Documents/Arma 3/*.Arma3Profile` and
/// `Documents/Arma 3 - Other Profiles/<dir>/*.Arma3Profile`, the stem up to the first `.`,
/// URL-decoded.
pub fn profiles(documents: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut add_from = |dir: &Path| {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if file.ends_with(".Arma3Profile")
                && let Some(stem) = file.split('.').next()
            {
                let name = url_decode(stem);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    };
    add_from(&documents.join("Arma 3"));
    for entry in std::fs::read_dir(documents.join("Arma 3 - Other Profiles"))
        .into_iter()
        .flatten()
        .flatten()
    {
        add_from(&entry.path());
    }
    names
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The `-world` suggestions (`ReloadWorlds`).
pub const WORLDS: [&str; 7] = [
    "stratis", "altis", "tanoa", "enoch", "malden", "VR", "empty",
];

/// Memory allocators: `system` plus the DLLs in the game's `Dll` folder.
pub fn allocators(game_dir: &Path) -> Vec<String> {
    let mut list = vec!["system".to_owned()];
    if let Some(dll) = crate::fs_case::resolve(game_dir, "dll") {
        let mut found: Vec<String> = std::fs::read_dir(dll)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let stem = name.strip_suffix(".dll")?;
                (stem.contains("x64") || !stem.ends_with("_bi")).then(|| stem.to_owned())
            })
            .collect();
        found.sort();
        list.extend(found);
    }
    list
}

/// Executable choices (`ParametersViewModel`): the setting value and its label; profiling and
/// diagnostic only when their executable exists.
pub fn platforms(game_dir: &Path) -> Vec<(&'static str, &'static str)> {
    let mut list = vec![("", "Client (64-Bit)"), ("server", "Server")];
    if crate::fs_case::resolve(game_dir, "arma3profiling_x64.exe").is_some() {
        list.push(("profiling", "Profiling"));
    }
    if crate::fs_case::resolve(game_dir, "arma3diag_x64.exe").is_some() {
        list.push(("diagnostic", "Diagnostic"));
    }
    list
}

/// Total installed memory in MB, the `maxmem` maximum.
pub fn total_memory_mb() -> i64 {
    crate::platform::total_memory_mb().unwrap_or(65536)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = include_str!("../../tests/fixtures/user.config");

    fn settings(test: &str, text: &str) -> Settings {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("target/test-tmp/params-{test}.config"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        Settings {
            config: UserConfig::load(&path).ok(),
            defaults: HashMap::new(),
            pending: HashMap::new(),
        }
    }

    #[test]
    fn real_settings_give_the_logged_command_line() {
        // Launcher.log: -skipIntro -noSplash TFAR_HRTF=1 -noCB -mod=…, where TFAR_HRTF=1 is the
        // launcher's own command line, which this launcher leaves to Steam.
        let s = settings("logged", REAL);
        let (tokens, user_mods) = command_line(&s);
        assert_eq!(tokens, ["-skipIntro", "-noSplash", "-noCB"]);
        assert!(user_mods.is_empty());
    }

    #[test]
    fn values_and_raw_text_render_like_vanilla() {
        let mut s = settings("values", REAL);
        s.set_flag("CPUCount", true);
        s.set("CPUCountValue", "8");
        s.set_flag("Name", true);
        s.set("NameValue", "PLAYER 1");
        s.set_flag("Mod", true);
        s.set("ModValue", "@a; @b;");
        s.set_flag("Cmd", true);
        s.set("CmdValue", r#"-world=empty "-init=hint x" -foo="a b""#);
        let (tokens, user_mods) = command_line(&s);
        assert_eq!(tokens[..3], ["-skipIntro", "-noSplash", "-name=PLAYER 1"]);
        assert!(tokens.contains(&"-cpuCount=8".to_owned()));
        assert!(tokens.ends_with(&[
            "-world=empty".into(),
            "-init=hint x".into(),
            "-foo=a b".into(),
            "-noCB".into()
        ]));
        assert_eq!(user_mods, ["@a", "@b"]);
    }

    #[test]
    fn decodes_profile_names() {
        assert_eq!(url_decode("PLAYER%201"), "PLAYER 1");
        assert_eq!(url_decode("[MOR]Sam"), "[MOR]Sam");
    }

    #[test]
    fn every_parameter_has_a_label_and_known_group() {
        for def in &DEFS {
            assert!(!def.label.is_empty(), "{def:?}");
            assert!(GROUPS.iter().any(|(g, _)| *g == def.group), "{def:?}");
        }
    }
}
