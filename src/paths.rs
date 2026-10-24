//! Where the game and the vanilla launcher's data live.

use std::path::{Path, PathBuf};

const ARMA3_APP_ID: &str = "107410";

pub struct GamePaths {
    /// The Arma 3 installation directory.
    pub game_dir: PathBuf,
    /// The vanilla launcher's data folder (`AppData/Local/Arma 3 Launcher`).
    pub launcher_data: PathBuf,
    /// The Steam library folder holding the game (`<library>/steamapps`).
    pub steamapps: PathBuf,
    /// The game's Wine prefix (`compatdata/107410/pfx`). Unused on Windows.
    #[cfg(unix)]
    prefix: PathBuf,
}

impl GamePaths {
    /// Paths for the install Steam launched (`game_dir`), or the default Steam library when the
    /// launcher was started by hand.
    pub fn new(game_dir: Option<&Path>) -> Self {
        let game_dir = game_dir
            .map(Path::to_path_buf)
            .unwrap_or_else(default_game_dir);
        let steamapps = game_dir
            .parent()
            .and_then(Path::parent)
            .unwrap_or(Path::new("/"))
            .to_path_buf();
        #[cfg(unix)]
        let (launcher_data, prefix) = proton_launcher_data(&steamapps);
        #[cfg(windows)]
        let launcher_data = crate::platform::local_app_data()
            .unwrap_or_else(|| PathBuf::from(r"C:\Users\Default\AppData\Local"))
            .join("Arma 3 Launcher");
        Self {
            game_dir,
            launcher_data,
            steamapps,
            #[cfg(unix)]
            prefix,
        }
    }

    /// `AppData/Local` of the prefix's user, or the real `%LOCALAPPDATA%` on Windows.
    pub fn local_app_data(&self) -> PathBuf {
        #[cfg(windows)]
        {
            self.launcher_data
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf()
        }
        #[cfg(unix)]
        {
            self.prefix.join("drive_c/users/steamuser/AppData/Local")
        }
    }

    /// Where the game writes profiles: the prefix's Documents folder, or the user's Documents.
    pub fn documents(&self) -> PathBuf {
        #[cfg(windows)]
        {
            crate::platform::documents_dir().unwrap_or_else(|| {
                self.local_app_data()
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("Documents")
            })
        }
        #[cfg(unix)]
        {
            self.prefix.join("drive_c/users/steamuser/Documents")
        }
    }

    /// The game's report folder (`AppData/Local/Arma 3`).
    pub fn arma_local(&self) -> PathBuf {
        self.local_app_data().join("Arma 3")
    }

    pub fn drive_map(&self) -> crate::vanilla::DriveMap {
        #[cfg(unix)]
        {
            crate::vanilla::DriveMap {
                dosdevices: self.prefix.join("dosdevices"),
                steam_root: self
                    .steamapps
                    .parent()
                    .unwrap_or(&self.steamapps)
                    .to_path_buf(),
            }
        }
        #[cfg(windows)]
        {
            let _ = self;
            crate::vanilla::DriveMap {
                dosdevices: PathBuf::new(),
                steam_root: PathBuf::new(),
            }
        }
    }

    /// The installed build id (Steam's manifest).
    pub fn build_id(&self) -> Option<String> {
        let manifest = std::fs::read_to_string(
            self.steamapps
                .join(format!("appmanifest_{ARMA3_APP_ID}.acf")),
        )
        .ok()?;
        crate::vdf::lookup(&manifest, &["AppState", "buildid"])
    }

    /// The Steam beta branch the installed build comes from; `None` on the public branch.
    pub fn steam_branch(&self) -> Option<String> {
        let manifest = std::fs::read_to_string(
            self.steamapps
                .join(format!("appmanifest_{ARMA3_APP_ID}.acf")),
        )
        .ok()?;
        crate::vdf::lookup(&manifest, &["AppState", "MountedConfig", "BetaKey"])
            .filter(|branch| !branch.is_empty() && branch != "public")
    }
}

#[cfg(unix)]
fn proton_launcher_data(steamapps: &Path) -> (PathBuf, PathBuf) {
    // Steam sets STEAM_COMPAT_DATA_PATH for Proton launches; otherwise the prefix sits in
    // the same library as the game: <library>/steamapps/{common/Arma 3, compatdata/107410}.
    let compat_data = std::env::var_os("STEAM_COMPAT_DATA_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| steamapps.join("compatdata").join(ARMA3_APP_ID));
    let prefix = compat_data.join("pfx");
    let launcher_data = prefix.join("drive_c/users/steamuser/AppData/Local/Arma 3 Launcher");
    (launcher_data, prefix)
}

fn default_game_dir() -> PathBuf {
    crate::platform::discover_game_dir().unwrap_or_else(|| {
        #[cfg(windows)]
        {
            PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\common\Arma 3")
        }
        #[cfg(not(windows))]
        {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            home.join(".local/share/Steam/steamapps/common/Arma 3")
        }
    })
}
