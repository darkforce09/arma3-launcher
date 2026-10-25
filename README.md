# arma3-launcher

A native launcher for Arma 3 that looks and works like the vanilla launcher. Rust + egui. On Linux it replaces the vanilla launcher under Proton. On Windows it is a native program next to the Windows game. It starts in about a tenth of a second and uses the desktop's own file dialogs.

## Install
Download `arma3-launcher-<version>-linux-x86_64.tar.gz` from [Releases](https://github.com/darkforce09/arma3-launcher/releases) and extract it into the Arma 3 folder:
```bash
tar -xzf arma3-launcher-*-linux-x86_64.tar.gz -C ~/.local/share/Steam/steamapps/common/"Arma 3"
```
That gives `Arma 3/arma3-launcher/` with the launcher and `libsteam_api.so`, which must stay next to it. Needs glibc 2.39 or newer.

In Steam: Arma 3 → Properties → Launch Options. Put the launcher in front of `%command%`, after any variables you already have, with the full path in quotes (replace `<you>` with your user name):
```
"/home/<you>/.local/share/Steam/steamapps/common/Arma 3/arma3-launcher/arma3-launcher" %command%
```
Press Play in Steam and the launcher opens. Its PLAY button starts the game through the Proton setup Steam would use. Launch options written after `%command%` are passed to the game.

Steam's "Verify integrity" leaves the folder alone; uninstalling Arma 3 deletes it.

Windows:
```
"C:\path\to\arma3-launcher.exe" %command%
```
The exe can live in its own folder or replace `arma3launcher.exe` in the Arma 3 folder. It does not use the `steam_api64.dll` that ships with the game; that file is an older Steamworks, and loading it makes Windows report a missing `SteamAPI_SteamFriends_v018` entry point. The process Steam launched stays running until the game exits, so playtime and Steam's Stop button keep working. An unsigned build will show a SmartScreen warning.

### Build from source
```bash
cargo build --release
```
The launcher is then `target/release/arma3-launcher`, with `libsteam_api.so` next to it. On Windows the Steam DLL is embedded in the exe and written to `%LOCALAPPDATA%\arma3-launcher\a3l_steam_api.dll` on first Steam use; do not copy `steam_api64.dll` next to the exe.

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
