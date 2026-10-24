# arma3-launcher

A native launcher for Arma 3 that looks and works like the vanilla launcher. Rust + egui. On Linux it replaces the vanilla launcher under Proton. On Windows it is a native program next to the Windows game. It starts in about a tenth of a second and uses the desktop's own file dialogs.

## Install
```bash
cargo build --release
```
In Steam: Arma 3 → Properties → Launch Options. Put the launcher in front of `%command%`.

Linux, after any variables you already have:
```
/path/to/arma3-launcher/target/release/arma3-launcher %command%
```
`libsteam_api.so` must stay next to the binary. Press Play in Steam and the launcher opens. Its PLAY button starts the game through the Proton setup Steam would use.

Windows:
```
"C:\path\to\arma3-launcher.exe" %command%
```
The exe can live in its own folder or replace `arma3launcher.exe` in the Arma 3 folder. It does not use the `steam_api64.dll` that ships with the game; that file is an older Steamworks, and loading it makes Windows report a missing `SteamAPI_SteamFriends_v018` entry point. The process Steam launched stays running until the game exits, so playtime and Steam's Stop button keep working. An unsigned build will show a SmartScreen warning.

Launch options written after `%command%` are passed to the game.

## What it does
- **Dashboard:** latest report, Arma 3 Units, social links, DLC banner.
- **News:** posts from dev.arma3.com, drawn in the launcher.
- **DLC:** ownership and install state, optional DLC loading, installing.
- **Parameters:** all startup parameters, with favourites.
- **Mods:** workshop and local mods, load order by drag and drop, presets (also HTML import/export), subscribe/unsubscribe/repair, subscribe to a workshop collection.
- **BattlEye:** the EULA before the first BattlEye launch; with the launcher left open, BattlEye's status messages in its log.
- **Servers:** Favorites, Recent, Official, Friends, Internet and Units lists; search, basic and advanced filters; server details and mod lists, matched by workshop id, mod hash or name; joining with the server's mods (subscribing to missing ones and waiting for Steam to install them); Direct Connect.
- **Options:** action after starting the game, default page, theme, watched mod folders.

It reads and writes the vanilla launcher's own files: presets, the loaded mods, parameter values in `user.config`, favourites, local mods, and the news and server-list caches. On Linux those files are in the Proton prefix. On Windows they are in `%LOCALAPPDATA%\Arma 3 Launcher`. You can switch between the two launchers at any time.

Its own state (workshop update tracking, unread news, window size) lives in `$XDG_STATE_HOME/arma3-launcher/state.json` on Linux and in `%LOCALAPPDATA%\arma3-launcher\` on Windows. Downloaded images are cached beside that.

After a game that stays attached to the launcher, a report in the Arma 3 folder that records an error can be opened from the dialog or from the OPTIONS menu.

## Boundaries
- On Linux, Arma 3 must be the Windows build run through Proton.
- Images and fonts are read from your Arma 3 install at runtime; none ship with this launcher.
- English only.
