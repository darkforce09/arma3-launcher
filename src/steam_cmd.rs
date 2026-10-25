//! The command Steam hands the launcher through `%command%` in the game's launch options, and its
//! rewrite into the command that starts the game.
//!
//! Steam expands `%command%` to its whole launch chain (reaper, runtime entry point, Proton)
//! ending in the Windows executable it would start, normally `arma3launcher.exe`, followed by any
//! launch options written after `%command%`. Swapping that executable for the game keeps the chain
//! intact, so Proton, the overlay and playtime tracking behave as for a normal launch.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Executables Steam may start for Arma 3; the last argument naming one of them is the game slot.
const GAME_EXES: [&str; 4] = [
    "arma3launcher.exe",
    "arma3_x64.exe",
    "arma3.exe",
    "arma3battleye.exe",
];

const BATTLEYE_EXE: &str = "arma3battleye.exe";

/// Launch options that only mean something to the vanilla launcher.
const VANILLA_ONLY_ARGS: [&str; 1] = ["-nolauncher"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SteamCommand {
    /// Everything before the executable: the launch chain.
    chain: Vec<OsString>,
    exe: PathBuf,
    /// Launch options written after `%command%`.
    user_args: Vec<OsString>,
}

impl SteamCommand {
    /// Parses the launcher's arguments (without argv\[0\]); `None` when they hold no Arma 3
    /// executable, i.e. the launcher was not started through `%command%`.
    ///
    /// On Windows, Steam's own command is just `arma3launcher.exe`. Replacing that file makes
    /// this process the executable, so it is not in the argument list; `current_exe` then is
    /// the game slot and the chain is empty.
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Option<Self> {
        let current = std::env::current_exe().ok();
        Self::from_invocation(args, current.as_deref())
    }

    fn from_invocation(
        args: impl IntoIterator<Item = OsString>,
        current_exe: Option<&Path>,
    ) -> Option<Self> {
        let args: Vec<OsString> = args.into_iter().collect();
        if let Some(index) = args.iter().rposition(|a| is_game_exe(a)) {
            let mut chain = args;
            let mut rest = chain.split_off(index);
            let exe = PathBuf::from(rest.remove(0));
            let user_args = rest
                .into_iter()
                .filter(|a| !VANILLA_ONLY_ARGS.iter().any(|v| a.eq_ignore_ascii_case(v)))
                .collect();
            return Some(Self {
                chain,
                exe,
                user_args,
            });
        }
        let exe = current_exe.filter(|exe| is_launcher_exe(exe.as_os_str()))?;
        let user_args = args
            .into_iter()
            .filter(|a| !VANILLA_ONLY_ARGS.iter().any(|v| a.eq_ignore_ascii_case(v)))
            .collect();
        Some(Self {
            chain: Vec::new(),
            exe: exe.to_path_buf(),
            user_args,
        })
    }

    /// The Arma 3 installation directory.
    pub fn game_dir(&self) -> &Path {
        self.exe.parent().unwrap_or(Path::new("."))
    }

    /// The full command that starts the game executable `exe` (a file name in the game folder)
    /// with `game_args`, then the user's launch options. With `battleye`, the BattlEye launcher
    /// starts `exe`; the vanilla launcher passes it `2 1`, its log mode (`1`: no console window)
    /// and the executable.
    pub fn game_command(&self, exe: &str, battleye: bool, game_args: &[String]) -> Vec<OsString> {
        let mut command = self.chain.clone();
        if battleye {
            command.push(self.game_dir().join(BATTLEYE_EXE).into_os_string());
            command.extend(["2", "1", "1", "-exe", exe].map(OsString::from));
        } else {
            command.push(self.game_dir().join(exe).into_os_string());
        }
        command.extend(game_args.iter().map(OsString::from));
        command.extend(self.user_args.iter().cloned());
        command
    }
}

fn is_game_exe(arg: &OsStr) -> bool {
    Path::new(arg)
        .file_name()
        .is_some_and(|name| GAME_EXES.iter().any(|exe| name.eq_ignore_ascii_case(exe)))
}

/// The file Steam launches for Arma 3 on Windows. Only this name, not the game executable: a
/// process that is itself `arma3_x64.exe` must not be treated as the launcher.
fn is_launcher_exe(arg: &OsStr) -> bool {
    Path::new(arg)
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("arma3launcher.exe"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = "/home/u/.local/share/Steam/steamapps/common/Arma 3";

    fn steam_args(exe: &str, extra: &[&str]) -> Vec<OsString> {
        let mut args: Vec<OsString> = [
            "/home/u/.local/share/Steam/ubuntu12_32/reaper",
            "SteamLaunch",
            "AppId=107410",
            "--",
            "/home/u/.local/share/Steam/ubuntu12_32/steam-launch-wrapper",
            "--",
            "/home/u/.local/share/Steam/steamapps/common/SteamLinuxRuntime_sniper/_v2-entry-point",
            "--verb=waitforexitandrun",
            "--",
            "/home/u/.local/share/Steam/compatibilitytools.d/GE-Proton11-7/proton",
            "waitforexitandrun",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        args.push(Path::new(GAME).join(exe).into_os_string());
        args.extend(extra.iter().map(OsString::from));
        args
    }

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn swaps_the_vanilla_launcher_for_the_game_and_keeps_the_chain() {
        let original = steam_args("arma3launcher.exe", &[]);
        let command = SteamCommand::parse(original.clone()).unwrap();
        assert_eq!(command.game_dir(), Path::new(GAME));

        let rewritten = command.game_command(
            "arma3_x64.exe",
            false,
            &["-skipIntro".into(), "-mod=a;b".into()],
        );
        let expected = strings(&steam_args("arma3_x64.exe", &["-skipIntro", "-mod=a;b"]));
        assert_eq!(strings(&rewritten), expected);
    }

    #[test]
    fn keeps_user_launch_options_after_the_game_args_and_drops_nolauncher() {
        let command = SteamCommand::parse(steam_args(
            "ARMA3LAUNCHER.EXE",
            &["-noLauncher", "-world=empty"],
        ))
        .unwrap();
        let rewritten = command.game_command("arma3_x64.exe", false, &["-skipIntro".into()]);
        assert_eq!(
            strings(&rewritten),
            strings(&steam_args(
                "arma3_x64.exe",
                &["-skipIntro", "-world=empty"]
            ))
        );
    }

    #[test]
    fn battleye_runs_its_launcher_with_the_game_executable() {
        let command = SteamCommand::parse(steam_args("arma3launcher.exe", &[])).unwrap();
        let rewritten =
            command.game_command("arma3profiling_x64.exe", true, &["-skipIntro".into()]);
        let expected = steam_args(
            "arma3battleye.exe",
            &[
                "2",
                "1",
                "1",
                "-exe",
                "arma3profiling_x64.exe",
                "-skipIntro",
            ],
        );
        assert_eq!(strings(&rewritten), strings(&expected));
    }

    #[test]
    fn an_empty_chain_starts_the_game_directly() {
        let game = Path::new("D:/Steam/steamapps/common/Arma 3");
        let command = SteamCommand::parse([
            game.join("arma3launcher.exe").into_os_string(),
            OsString::from("-world=empty"),
        ])
        .unwrap();
        assert_eq!(command.game_dir(), game);
        let rewritten = command.game_command("arma3_x64.exe", false, &["-skipIntro".into()]);
        assert_eq!(
            rewritten,
            vec![
                game.join("arma3_x64.exe").into_os_string(),
                OsString::from("-skipIntro"),
                OsString::from("-world=empty"),
            ]
        );
    }

    #[test]
    fn rejects_arguments_without_an_arma_executable() {
        assert_eq!(
            SteamCommand::from_invocation(Vec::new(), Some(Path::new("arma3-launcher.exe"))),
            None
        );
        assert_eq!(SteamCommand::from_invocation(["--help".into()], None), None);
    }

    #[test]
    fn a_replaced_launcher_is_the_steam_executable() {
        let game = Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\Arma 3");
        let command = SteamCommand::from_invocation(
            ["-world=empty".into()],
            Some(&game.join("arma3launcher.exe")),
        )
        .unwrap();
        assert_eq!(command.game_dir(), game);
        assert_eq!(
            command.game_command("arma3_x64.exe", false, &["-skipIntro".into()]),
            vec![
                game.join("arma3_x64.exe").into_os_string(),
                OsString::from("-skipIntro"),
                OsString::from("-world=empty"),
            ]
        );
    }
}
