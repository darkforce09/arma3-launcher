# arma3-launcher

Native Linux replacement for the vanilla Arma 3 launcher: Rust + egui, same look as vanilla, started by Steam through `%command%`, reads and writes the vanilla launcher's own files in the Proton prefix.

## Laws
- Behaviour and look follow the decompiled vanilla launcher. When unsure, check the decompiled source; don't guess.
- No Bohemia assets in git. Images and fonts are read from the user's Arma 3 install at runtime.
- Files the vanilla launcher owns (presets, `user.config`, JSON) stay readable by vanilla after we write them.
- File lookups in the game folder ignore case.
- All disk, Steam and network work runs off the UI thread.

## Agent environment
- Builds run in the `agents` distrobox (Debian, same glibc as the Bazzite host); the binary runs on the host.
- Arma 3 (Windows build under Proton): `~/.local/share/Steam/steamapps/common/Arma 3`; prefix: `~/.local/share/Steam/steamapps/compatdata/107410/pfx`; vanilla launcher data: `<prefix>/drive_c/users/steamuser/AppData/Local/Arma 3 Launcher/`.
- `.workstation/` (gitignored): `decompiled/` vanilla launcher source (ilspycmd output), `findings/` notes on it with source paths, `tools/` (.NET SDK, ilspycmd), `logs/`.

## Repository map
- `build.rs`: on Linux, copies `libsteam_api.so` next to the binary (`$ORIGIN` rpath). On Windows, embeds that crate's `steam_api64.dll` and delay-loads it from `%LOCALAPPDATA%\arma3-launcher`, so the older DLL in the game folder is not used.
- `src/main.rs`: entry, Steam connection, window, hand-over to the game. `src/steam_env.rs` keeps Steam's `LD_PRELOAD`/`LD_LIBRARY_PATH` away from the UI; `src/steam_cmd.rs` rewrites Steam's `%command%` into the game command.
- `src/assets.rs`: images and fonts read from the install's resource DLLs. `src/paths.rs`, `src/fs_case.rs`, `src/vdf.rs`: where things are.
- `src/vanilla/`: the vanilla launcher's files (presets, preset HTML, `user.config`, `Local.json`, caches).
- `src/mods.rs` + `src/library.rs`: mod scan, load order, presets, `-mod`, mod short hashes. `src/dlc.rs`, `src/params/` (parameter table and command line), `src/news/` (feed, HTML → blocks), `src/servers/` (lists, gametags, A2S rules decoder), `src/units.rs`, `src/battleye.rs` (BattlEye status reports and their log), `src/steam.rs` (Steamworks), `src/state.rs`, `src/config.rs`.
- `src/ui/`: one module per page plus `chrome` (window frame), `nav`, `widgets`, `menu`, `rich` (news text), `gfx` (textures, icons), `theme` (vanilla colours and metrics).

## Commands
- Build: `cargo build --release`
- Test: `cargo test`
- Lint: `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`
