#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod battleye;
mod config;
mod dlc;
mod fs_case;
mod library;
mod mods;
mod news;
mod params;
mod paths;
mod platform;
mod rpt;
mod servers;
mod state;
mod steam;
mod steam_cmd;
mod steam_env;
mod ui;
mod units;
mod vanilla;
mod vdf;

use std::ffi::OsString;
use std::process::ExitCode;
use std::sync::Arc;

use eframe::egui;
use steam_cmd::SteamCommand;

/// The vanilla window size, plus the shadow margin on every side where that margin is painted.
const WINDOW_SIZE: [f32; 2] = [
    970.0 + ui::theme::SHADOW_DEPTH * 2.0,
    690.0 + ui::theme::SHADOW_DEPTH * 2.0,
];

fn main() -> ExitCode {
    // Before any Steam call. Windows delay-loads steam_api64.dll from our own copy; the game
    // directory's DLL is an older Steamworks and does not export the flat API.
    #[cfg(windows)]
    platform::prepare_steam_api();
    if let Err(e) = steam_env::reexec_without_steam_libraries() {
        platform::log_error(&format!("re-exec without Steam's libraries failed: {e}"));
    }
    let steam_command = SteamCommand::parse(std::env::args_os().skip(1));
    let paths = paths::GamePaths::new(steam_command.as_ref().map(SteamCommand::game_dir));
    let assets = match assets::Assets::load(&paths.game_dir) {
        Ok(assets) => Some(Arc::new(assets)),
        Err(e) => {
            platform::log_error(&format!("launcher resources unavailable: {e}"));
            None
        }
    };
    let startup = ui::Startup {
        steam_command,
        config: config::LauncherConfig::load(&paths.game_dir),
        news: news::cached_posts(&paths.launcher_data),
        owned_app_ids: vanilla::owned_app_ids(&paths.launcher_data),
        branch: paths.steam_branch(),
        assets,
        paths,
    };
    let steam = match steam::Steam::connect() {
        Ok(steam) => Some(steam),
        Err(e) => {
            platform::log_error(&format!("Steam API unavailable: {e}"));
            None
        }
    };

    let mut launch: Option<Vec<OsString>> = None;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Arma 3 Launcher")
        .with_app_id("arma3-launcher")
        .with_decorations(false)
        .with_transparent(true)
        .with_inner_size(
            state::State::load()
                .and_then(|s| s.window_size)
                .map_or(WINDOW_SIZE, |[w, h]| {
                    [w.max(WINDOW_SIZE[0]), h.max(WINDOW_SIZE[1])]
                }),
        )
        .with_min_inner_size(WINDOW_SIZE);
    if let Some(icon) = startup
        .assets
        .as_ref()
        .and_then(|assets| window_icon(assets))
    {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let result = eframe::run_native(
        "arma3-launcher",
        options,
        Box::new(|cc| {
            Ok(Box::new(ui::LauncherApp::new(
                cc,
                startup,
                steam.as_ref(),
                &mut launch,
            )))
        }),
    );
    if let Err(e) = result {
        platform::log_error(&e.to_string());
        return ExitCode::FAILURE;
    }
    // The game initialises the Steam API itself; ours must be shut down before handing over.
    if let Some(steam) = steam {
        steam.shutdown();
    }
    match launch {
        Some(command) => start_game(&command),
        None => ExitCode::SUCCESS,
    }
}

/// Close-after-Play. Linux replaces this process with Proton so Steam keeps tracking it.
/// Windows has no `exec`: the process Steam launched waits for the game, without a window, and
/// stays in Steam's job so Stop still ends the game.
fn start_game(command: &[OsString]) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut child = steam_env::command_with_steam_env(command);
        let e = child.exec();
        platform::log_error(&format!("starting the game failed: {e}"));
        ExitCode::FAILURE
    }
    #[cfg(windows)]
    {
        let mut child = steam_env::command_with_steam_env(command);
        match child.spawn() {
            Ok(mut game) => match game.wait() {
                Ok(status) if status.success() => ExitCode::SUCCESS,
                Ok(status) => ExitCode::from(status.code().unwrap_or(1) as u8),
                Err(e) => {
                    platform::log_error(&format!("waiting for the game failed: {e}"));
                    ExitCode::FAILURE
                }
            },
            Err(e) => {
                platform::log_error(&format!("starting the game failed: {e}"));
                ExitCode::FAILURE
            }
        }
    }
}

/// The taskbar icon, from the launcher logo already in the game's resources.
fn window_icon(assets: &assets::Assets) -> Option<egui::IconData> {
    let bytes = assets.get("themes/defaultstyle/images/arma_logo.png")?;
    let image = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}
