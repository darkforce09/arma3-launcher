//! DLC (`DlcFactory`): the list, what the user owns and has installed, and the folders loaded
//! optional DLC put first in `-mod`.

use std::path::Path;

pub struct Dlc {
    pub app_id: u32,
    pub name: &'static str,
    pub author: &'static str,
    /// The folder in the game directory (as the vanilla launcher spells it).
    pub folder: &'static str,
    /// Loaded by the game itself, never part of `-mod`.
    pub mandatory: bool,
    /// Image resources (relative to the launcher's images).
    pub preview: &'static str,
    pub icon: &'static str,
}

/// `DlcFactory`, in its order.
pub const DLCS: [Dlc; 17] = [
    Dlc {
        app_id: 2647830,
        name: "Expeditionary Forces",
        author: "Tiny Gecko Studios",
        folder: "EF",
        mandatory: false,
        preview: "dlc/ef-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-ef256.png",
    },
    Dlc {
        app_id: 2647760,
        name: "Reaction Forces",
        author: "Rotators Collective",
        folder: "RF",
        mandatory: false,
        preview: "dlc/rf-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-rf256.png",
    },
    Dlc {
        app_id: 1175380,
        name: "Spearhead 1944",
        author: "Heavy Ordnance Works",
        folder: "SPE",
        mandatory: false,
        preview: "dlc/spe-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-spe141.png",
    },
    Dlc {
        app_id: 1681170,
        name: "Western Sahara",
        author: "Rotators Collective",
        folder: "WS",
        mandatory: false,
        preview: "dlc/ws-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-ws256.png",
    },
    Dlc {
        app_id: 1227700,
        name: "S.O.G. Prairie Fire",
        author: "Savage Game Design",
        folder: "VN",
        mandatory: false,
        preview: "dlc/vn-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-vn300.png",
    },
    Dlc {
        app_id: 1294440,
        name: "CSLA Iron Curtain",
        author: "CSLA Studio",
        folder: "CSLA",
        mandatory: false,
        preview: "dlc/csla-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-csla256.png",
    },
    Dlc {
        app_id: 1325500,
        name: "Art of War",
        author: "Bohemia Interactive",
        folder: "AoW",
        mandatory: true,
        preview: "dlc/aow-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-aowicon256.png",
    },
    Dlc {
        app_id: 1042220,
        name: "Global Mobilization",
        author: "Vertexmacht",
        folder: "GM",
        mandatory: false,
        preview: "dlc/gm-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-gm128.png",
    },
    Dlc {
        app_id: 1021790,
        name: "Contact",
        author: "Bohemia Interactive",
        folder: "contact",
        mandatory: false,
        preview: "dlc/contact-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-contacticon32.png",
    },
    Dlc {
        app_id: 395180,
        name: "Apex",
        author: "Bohemia Interactive",
        folder: "Expansion",
        mandatory: true,
        preview: "dlc/apex-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-apexicon32.png",
    },
    Dlc {
        app_id: 304380,
        name: "Helicopters",
        author: "Bohemia Interactive",
        folder: "Heli",
        mandatory: true,
        preview: "dlc/helicopters-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-heliicon32.png",
    },
    Dlc {
        app_id: 332350,
        name: "Marksmen",
        author: "Bohemia Interactive",
        folder: "Mark",
        mandatory: true,
        preview: "dlc/marksmen-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-markicon32.png",
    },
    Dlc {
        app_id: 288520,
        name: "Karts",
        author: "Bohemia Interactive",
        folder: "Kart",
        mandatory: true,
        preview: "dlc/karts-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-kartsicon32.png",
    },
    Dlc {
        app_id: 798390,
        name: "Tanks",
        author: "Bohemia Interactive",
        folder: "Tank",
        mandatory: true,
        preview: "dlc/tanks-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-tanksicon32.png",
    },
    Dlc {
        app_id: 601670,
        name: "Jets",
        author: "Bohemia Interactive",
        folder: "Jets",
        mandatory: true,
        preview: "dlc/jets-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-jeticon32.png",
    },
    Dlc {
        app_id: 571710,
        name: "Laws of War",
        author: "Bohemia Interactive",
        folder: "Orange",
        mandatory: true,
        preview: "dlc/lawsofwar-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-orangeicon32.png",
    },
    Dlc {
        app_id: 744950,
        name: "Tac-Ops Mission Pack",
        author: "Bohemia Interactive",
        folder: "Tacops",
        mandatory: true,
        preview: "dlc/tacops-a3-dlc_launcherthumbnail_512x256.png",
        icon: "dlc-tacopsicon32.png",
    },
];

pub fn find(app_id: u32) -> Option<&'static Dlc> {
    DLCS.iter().find(|d| d.app_id == app_id)
}

/// Ownership and install state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DlcState {
    pub owned: bool,
    pub installed: bool,
}

/// DLC app ids installed according to Steam's manifest (`InstalledDepots` → `dlcappid`), for
/// when the Steam API isn't available.
pub fn installed_from_manifest(steamapps: &Path) -> Vec<u32> {
    let Ok(text) = std::fs::read_to_string(steamapps.join("appmanifest_107410.acf")) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    let lines = text.lines().map(str::trim);
    for line in lines {
        let mut parts = line.split('"').filter(|p| !p.trim().is_empty());
        if parts.next() == Some("dlcappid")
            && let Some(id) = parts.next().and_then(|v| v.parse().ok())
        {
            ids.push(id);
        }
    }
    ids
}

/// The `-mod` entries for the loaded optional DLC that are installed, in load order: the DLC
/// folders as the game sees them.
pub fn launch_paths(
    loaded: &[u32],
    game_dir: &Path,
    installed: impl Fn(u32) -> bool,
) -> Vec<String> {
    loaded
        .iter()
        .filter_map(|id| find(*id))
        .filter(|d| !d.mandatory && installed(d.app_id))
        .filter_map(|d| crate::fs_case::resolve(game_dir, d.folder))
        .map(|path| crate::vanilla::game_path(&path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_paths_skip_mandatory_and_missing_dlc() {
        let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp/dlc-game");
        let _ = std::fs::remove_dir_all(&game);
        std::fs::create_dir_all(game.join("GM")).unwrap();
        std::fs::create_dir_all(game.join("Contact")).unwrap();
        std::fs::create_dir_all(game.join("Expansion")).unwrap();
        let paths = launch_paths(&[1021790, 395180, 1042220, 2647830], &game, |_| true);
        let expected = [
            crate::vanilla::game_path(&game.join("Contact")),
            crate::vanilla::game_path(&game.join("GM")),
        ];
        // Windows `exists` is case-insensitive, so the resolved folder may keep the requested case.
        assert_eq!(paths.len(), expected.len());
        for (path, expected) in paths.iter().zip(expected) {
            assert!(path.eq_ignore_ascii_case(&expected), "{path} != {expected}");
        }
    }
}
