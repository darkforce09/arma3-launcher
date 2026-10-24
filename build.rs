//! Ships the Steam redistributable the binary was linked against.
//!
//! Linux finds `libsteam_api.so` through an `$ORIGIN` rpath. Windows cannot put `steam_api64.dll`
//! next to an executable that lives in the Arma 3 folder: that folder already has an older
//! `steam_api64.dll`, and the loader would bind that one. The DLL is embedded instead and
//! delay-loaded from a private path.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let redistributable = steamworks_sys_dir().join("lib/steam/redistributable_bin");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    if target_os == "windows" {
        // Resolved on the first Steam call, after the hook in `platform` has written our copy.
        println!("cargo:rustc-link-arg=/DELAYLOAD:steam_api64.dll");
        println!("cargo:rustc-link-arg=delayimp.lib");
        let source = redistributable.join("win64/steam_api64.dll");
        std::fs::copy(&source, out_dir.join("steam_api64.dll"))
            .unwrap_or_else(|e| panic!("copy {}: {e}", source.display()));
    } else {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
        let source = redistributable.join("linux64/libsteam_api.so");
        // OUT_DIR is target/<profile>/build/<crate>-<hash>/out; the binary lives in target/<profile>.
        let profile_dir = out_dir
            .ancestors()
            .nth(3)
            .expect("OUT_DIR is inside target/<profile>");
        std::fs::copy(&source, profile_dir.join("libsteam_api.so"))
            .unwrap_or_else(|e| panic!("copy {}: {e}", source.display()));
    }
}

/// The source directory of the `steamworks-sys` crate, which ships the Steamworks redistributable.
///
/// `cargo metadata --filter-platform` downloads only this build's target. The directory is the
/// version this package links: another version exports a different set of functions, and the
/// loader then refuses to start the binary.
fn steamworks_sys_dir() -> PathBuf {
    let out = Command::new(std::env::var("CARGO").unwrap())
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
        ])
        .arg(std::env::var("TARGET").unwrap())
        .output()
        .expect("cargo metadata runs");
    assert!(
        out.status.success(),
        "cargo metadata: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "steamworks-sys")
        .expect("steamworks-sys in cargo metadata");
    Path::new(package["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_path_buf()
}

⚠ 1 unresolved conflict detected
- ours = HEAD
- theirs = origin/main
NOTICE: Inspect a block by reading `conflict://<N>` (add `/ours` / `/theirs` / `/base` to render a single side). Resolve with `write({ path: "conflict://<N>", content })`, or bulk-resolve every registered conflict with `write({ path: "conflict://*", content })`. Writes replace ONLY the marker block (markers + all sides) — never repeat the lines before/after it; they stay in place.
`content` shorthand: a line that is exactly `@ours` / `@theirs` / `@base` / `@both` expands to that recorded section. `@both` is ours-then-theirs with no separator — only for additive conflicts where each side adds something different; NEVER for competing edits of the same lines (pick a side or write the combined text). Lines that are not a token pass through verbatim, so `"// keep both\n@ours\n@theirs"` literally writes the comment, then ours, then theirs.
Per-id bulk: `write({ path: "conflict://*", content: "1: @ours\n2: @theirs\n…" })` resolves each listed id with that side in ONE call — the cheapest way through many pick-one conflicts; unlisted ids stay registered.
Resolve each block faithfully: keep one side (`@ours`/`@theirs`), or combine them when both intents apply — never invent content beyond the recorded sides, and never stack both sides of competing edits. Resolve several conflicts in a single turn by issuing multiple `write` calls at once; ids stay valid as earlier blocks are resolved.

──── #2  L41-113 ────
<<< ours
    let folder = format!("steamworks-sys-{}", steamworks_sys_version());
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
            Some(PathBuf::from(home).join(".cargo"))
… (37 more lines)
>>> theirs
    let out = Command::new(std::env::var("CARGO").unwrap())
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
… (21 more lines)
⚠ 1 unresolved conflict detected
- ours = HEAD
- theirs = origin/main
NOTICE: Inspect a block by reading `conflict://<N>` (add `/ours` / `/theirs` / `/base` to render a single side). Resolve with `write({ path: "conflict://<N>", content })`, or bulk-resolve every registered conflict with `write({ path: "conflict://*", content })`. Writes replace ONLY the marker block (markers + all sides) — never repeat the lines before/after it; they stay in place.
`content` shorthand: a line that is exactly `@ours` / `@theirs` / `@base` / `@both` expands to that recorded section. `@both` is ours-then-theirs with no separator — only for additive conflicts where each side adds something different; NEVER for competing edits of the same lines (pick a side or write the combined text). Lines that are not a token pass through verbatim, so `"// keep both\n@ours\n@theirs"` literally writes the comment, then ours, then theirs.
Per-id bulk: `write({ path: "conflict://*", content: "1: @ours\n2: @theirs\n…" })` resolves each listed id with that side in ONE call — the cheapest way through many pick-one conflicts; unlisted ids stay registered.
Resolve each block faithfully: keep one side (`@ours`/`@theirs`), or combine them when both intents apply — never invent content beyond the recorded sides, and never stack both sides of competing edits. Resolve several conflicts in a single turn by issuing multiple `write` calls at once; ids stay valid as earlier blocks are resolved.

──── #3  L42-114 ────
<<< ours
    let folder = format!("steamworks-sys-{}", steamworks_sys_version());
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
            Some(PathBuf::from(home).join(".cargo"))
… (37 more lines)
>>> theirs
    let out = Command::new(std::env::var("CARGO").unwrap())
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
… (21 more lines)