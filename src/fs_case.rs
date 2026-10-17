//! Case-insensitive path lookup. The vanilla launcher runs on a case-insensitive file system and
//! names files with a different case than they have on disk (`Arma3_x64.exe` vs `arma3_x64.exe`).

use std::path::{Path, PathBuf};

/// The existing path under `base` whose components match `relative` (`/`-separated) ignoring
/// ASCII case; an exact match wins.
pub fn resolve(base: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = base.to_path_buf();
    for component in relative.split('/').filter(|c| !c.is_empty()) {
        let exact = path.join(component);
        if exact.exists() {
            path = exact;
            continue;
        }
        let entry = std::fs::read_dir(&path)
            .ok()?
            .flatten()
            .find(|e| e.file_name().eq_ignore_ascii_case(component))?;
        path = entry.path();
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_files_whatever_their_case() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp/fs-case");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Launcher/En-us")).unwrap();
        std::fs::write(
            dir.join("Launcher/En-us/SharedResources.resources.dll"),
            b"",
        )
        .unwrap();

        assert_eq!(
            resolve(&dir, "launcher/en-US/sharedresources.RESOURCES.dll"),
            Some(dir.join("Launcher/En-us/SharedResources.resources.dll"))
        );
        assert_eq!(resolve(&dir, "Launcher/missing.dll"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
