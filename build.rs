//! Ships the Steam redistributable the binary was linked against.
//!
//! Linux finds `libsteam_api.so` through an `$ORIGIN` rpath. Windows cannot put `steam_api64.dll`
//! next to an executable that lives in the Arma 3 folder: that folder already has an older
//! `steam_api64.dll`, and the loader would bind that one. The DLL is embedded instead and
//! delay-loaded from a private path.

use std::path::PathBuf;

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
/// Looked up in the cargo registry rather than `cargo metadata`, which needs every platform's
/// crates cached. The directory is the version this package links: another version in the registry
/// exports a different set of functions, and the loader then refuses to start the binary.
fn steamworks_sys_dir() -> PathBuf {
    let folder = format!("steamworks-sys-{}", steamworks_sys_version());
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
            Some(PathBuf::from(home).join(".cargo"))
        })
        .expect("cargo home");
    let src = cargo_home.join("registry/src");
    std::fs::read_dir(&src)
        .unwrap_or_else(|e| panic!("read {}: {e}", src.display()))
        .flatten()
        .find_map(|index| {
            let candidate = index.path().join(&folder);
            candidate.is_dir().then_some(candidate)
        })
        .unwrap_or_else(|| panic!("{folder} not in the cargo registry"))
}

fn steamworks_sys_version() -> String {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let lock = std::fs::read_to_string(manifest_dir.join("Cargo.lock")).expect("Cargo.lock");
    let mut in_package = false;
    let mut name = String::new();
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_package = true;
            name.clear();
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(value) = line.strip_prefix("name = \"") {
            name = value.trim_end_matches('"').to_owned();
        } else if let Some(value) = line.strip_prefix("version = \"")
            && name == "steamworks-sys"
        {
            return value.trim_end_matches('"').to_owned();
        }
    }
    panic!("steamworks-sys not in Cargo.lock");
}
