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
use std::os::unix::process::CommandExt;
use std::process::ExitCode;
use std::sync::Arc;

use eframe::egui;
use steam_cmd::SteamCommand;

/// The vanilla window size plus the shadow margin on every side.
const WINDOW_SIZE: [f32; 2] = [970.0 + 32.0, 690.0 + 32.0];

fn main() -> ExitCode {
    if let Err(e) = steam_env::reexec_without_steam_libraries() {
        eprintln!("arma3-launcher: re-exec without Steam's libraries failed: {e}");
    }
    let steam_command = SteamCommand::parse(std::env::args_os().skip(1));
    let paths = paths::GamePaths::new(steam_command.as_ref().map(SteamCommand::game_dir));
    let assets = match assets::Assets::load(&paths.game_dir) {
        Ok(assets) => Some(Arc::new(assets)),
        Err(e) => {
            eprintln!("arma3-launcher: launcher resources unavailable: {e}");
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
            eprintln!("arma3-launcher: Steam API unavailable: {e}");
            None
        }
    };

    let mut launch: Option<Vec<OsString>> = None;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
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
            .with_min_inner_size(WINDOW_SIZE),
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
        eprintln!("arma3-launcher: {e}");
        return ExitCode::FAILURE;
    }
    // The game initialises the Steam API itself; ours must be shut down before handing over.
    if let Some(steam) = steam {
        steam.shutdown();
    }
    match launch {
        Some(command) => {
            let e = steam_env::command_with_steam_env(&command).exec();
            eprintln!("arma3-launcher: starting the game failed: {e}");
            ExitCode::FAILURE
        }
        None => ExitCode::SUCCESS,
    }
}
