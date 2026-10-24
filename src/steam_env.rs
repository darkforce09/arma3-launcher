//! Keeps Steam's injected libraries out of the launcher's own process and hands them back to the
//! game.
//!
//! On Linux, Steam starts the launch chain with `LD_PRELOAD` (the overlay) and `LD_LIBRARY_PATH`
//! (its runtime libraries) set. Both are meant for the game: the overlay would hook the launcher's
//! window and the runtime's old libraries can shadow the system ones the UI links against. The
//! launcher re-executes itself with them moved aside and restores them on the game's command.
//!
//! Windows injects the overlay into the process Steam launched; there is nothing to stash.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::Command;

/// Executables that are the game itself. When the launch chain is empty, the command starts one
/// of these and should run in the game directory.
const GAME_EXES: [&str; 6] = [
    "arma3_x64.exe",
    "arma3.exe",
    "arma3battleye.exe",
    "arma3profiling_x64.exe",
    "arma3diag_x64.exe",
    "arma3server_x64.exe",
];

#[cfg(unix)]
const STASHED: [&str; 2] = ["LD_PRELOAD", "LD_LIBRARY_PATH"];
#[cfg(unix)]
const ORIGINAL_PREFIX: &str = "ARMA3_LAUNCHER_ORIG_";
/// Set once the variables are stashed, so the re-executed process doesn't stash again.
#[cfg(unix)]
const MARKER: &str = "ARMA3_LAUNCHER_STASHED";

/// Re-executes the launcher without the stashed variables; returns only when nothing needs
/// stashing or the re-exec failed. On Windows there is nothing to stash.
pub fn reexec_without_steam_libraries() -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        if std::env::var_os(MARKER).is_some()
            || STASHED.iter().all(|v| std::env::var_os(v).is_none())
        {
            return Ok(());
        }
        let mut args = std::env::args_os();
        let arg0 = args.next().unwrap_or_default();
        let mut command = Command::new("/proc/self/exe");
        command.arg0(arg0).args(args).env(MARKER, "1");
        for var in STASHED {
            if let Some(value) = std::env::var_os(var) {
                command
                    .env(format!("{ORIGINAL_PREFIX}{var}"), value)
                    .env_remove(var);
            }
        }
        Err(command.exec())
    }
    #[cfg(not(unix))]
    {
        Ok(())
    }
}

/// A command for `program_and_args` with the environment Steam gave the launcher.
pub fn command_with_steam_env(program_and_args: &[OsString]) -> Command {
    let mut command = Command::new(&program_and_args[0]);
    command.args(&program_and_args[1..]);
    #[cfg(unix)]
    {
        command.env_remove(MARKER);
        for var in STASHED {
            let original = format!("{ORIGINAL_PREFIX}{var}");
            match std::env::var_os(&original) {
                Some(value) => command.env(var, value),
                None => command.env_remove(var),
            };
            command.env_remove(original);
        }
    }
    // An empty chain (Windows `%command%`) starts the game directly. Proton's chain keeps the
    // working directory Steam set.
    if is_game_executable(&program_and_args[0])
        && let Some(dir) = Path::new(&program_and_args[0]).parent()
    {
        command.current_dir(dir);
    }
    command
}

fn is_game_executable(program: &OsStr) -> bool {
    Path::new(program)
        .file_name()
        .is_some_and(|name| GAME_EXES.iter().any(|exe| name.eq_ignore_ascii_case(exe)))
}
