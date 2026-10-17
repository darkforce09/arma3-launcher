//! Puts Steam's `libsteam_api.so` next to the binary, which finds it through an `$ORIGIN` rpath.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lib = steamworks_sys_dir().join("lib/steam/redistributable_bin/linux64/libsteam_api.so");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // OUT_DIR is target/<profile>/build/<crate>-<hash>/out; the binary lives in target/<profile>.
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("OUT_DIR is inside target/<profile>");
    std::fs::copy(&lib, profile_dir.join("libsteam_api.so"))
        .unwrap_or_else(|e| panic!("copy {}: {e}", lib.display()));
}

/// The source directory of the `steamworks-sys` crate, which ships the Steamworks redistributable.
fn steamworks_sys_dir() -> PathBuf {
    let out = Command::new(std::env::var("CARGO").unwrap())
        .args(["metadata", "--format-version", "1", "--offline"])
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
