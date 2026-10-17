# arma3-launcher

A native Linux launcher for Arma 3 that looks and works like the vanilla launcher, without running the vanilla launcher under Proton. Rust + egui. It starts in about a tenth of a second and uses the desktop's own file dialogs (through the XDG portal).

## Install
```bash
cargo build --release
```
In Steam: Arma 3 → Properties → Launch Options. Put the launcher in front of `%command%`, after any variables you already have:
```
/path/to/arma_3_launcher/target/release/arma3-launcher %command%
```
Press Play in Steam and the launcher opens. Its PLAY button starts the game through the Proton setup Steam would use. Launch options written after `%command%` are passed to the game.

`libsteam_api.so` must stay next to the binary.

## What it does
- **Dashboard:** latest report, Arma 3 Units, social links, DLC banner.
- **News:** posts from dev.arma3.com, drawn in the launcher.
- **DLC:** ownership and install state, optional DLC loading, installing.
- **Parameters:** all startup parameters, with favourites.
- **Mods:** workshop and local mods, load order by drag and drop, presets (also HTML import/export), subscribe/unsubscribe/repair.
- **BattlEye:** the EULA before the first BattlEye launch; with the launcher left open, BattlEye's status messages in its log.
- **Servers:** Favorites, Recent, Official, Friends, Internet and Units lists; search, basic and advanced filters; server details and mod lists, matched by workshop id, mod hash or name; joining with the server's mods (subscribing to missing ones and waiting for Steam to install them); Direct Connect.
- **Options:** action after starting the game, default page, theme, watched mod folders.

It reads and writes the vanilla launcher's own files in the Proton prefix: presets, the loaded mods, parameter values in `user.config`, favourites, local mods, and the news and server-list caches. You can switch between the two launchers at any time.

Its own state (workshop update tracking, unread news, window size) lives in `$XDG_STATE_HOME/arma3-launcher/state.json`; downloaded images are cached in `$XDG_CACHE_HOME/arma3-launcher/`.

## Boundaries
- Arma 3 must be the Windows build run through Proton.
- Images and fonts are read from your Arma 3 install at runtime; none ship with this launcher.
- English only.
