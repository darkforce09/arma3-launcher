//! Keeps Steam's injected libraries out of the launcher's own process and hands them back to the
//! game.
//!
//! Steam starts the launch chain with `LD_PRELOAD` (the overlay) and `LD_LIBRARY_PATH` (its
//! runtime libraries) set. Both are meant for the game: the overlay would hook the launcher's
//! window and the runtime's old libraries can shadow the system ones the UI links against. The
//! launcher re-executes itself with them moved aside and restores them on the game's command.

use std::ffi::OsString;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

const STASHED: [&str; 2] = ["LD_PRELOAD", "LD_LIBRARY_PATH"];
const ORIGINAL_PREFIX: &str = "ARMA3_LAUNCHER_ORIG_";
/// Set once the variables are stashed, so the re-executed process doesn't stash again.
const MARKER: &str = "ARMA3_LAUNCHER_STASHED";

/// Re-executes the launcher without the stashed variables; returns only when nothing needs
/// stashing or the re-exec failed.
pub fn reexec_without_steam_libraries() -> io::Result<()> {
    if std::env::var_os(MARKER).is_some() || STASHED.iter().all(|v| std::env::var_os(v).is_none()) {
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

/// A command for `program_and_args` with the environment Steam gave the launcher.
pub fn command_with_steam_env(program_and_args: &[OsString]) -> Command {
    let mut command = Command::new(&program_and_args[0]);
    command.args(&program_and_args[1..]).env_remove(MARKER);
    for var in STASHED {
        let original = format!("{ORIGINAL_PREFIX}{var}");
        match std::env::var_os(&original) {
            Some(value) => command.env(var, value),
            None => command.env_remove(var),
        };
        command.env_remove(original);
    }
    command
}
