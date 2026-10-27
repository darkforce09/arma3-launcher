//! The game's latest report (`*.rpt` in the Arma 3 local folder).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const SCAN_BYTES: u64 = 2 * 1024 * 1024;

/// The newest `.rpt` in `dir`.
pub fn newest(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.to_ascii_lowercase().ends_with(".rpt") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        if best.as_ref().is_none_or(|(time, _)| modified > *time) {
            best = Some((modified, entry.path()));
        }
    }
    best.map(|(_, path)| path)
}

/// Whether the report records an engine or script error. Only the start and the end are read.
pub fn has_error(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let Ok(len) = file.metadata().map(|m| m.len()) else {
        return false;
    };
    contains_error(&read_range(&mut file, 0, SCAN_BYTES.min(len)))
        || (len > SCAN_BYTES
            && contains_error(&read_range(&mut file, len - SCAN_BYTES, SCAN_BYTES)))
}

fn read_range(file: &mut File, start: u64, len: u64) -> Vec<u8> {
    let mut buf = vec![0u8; len as usize];
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    match file.read(&mut buf) {
        Ok(n) => buf.truncate(n),
        Err(_) => return Vec::new(),
    }
    buf
}

fn contains_error(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("Error") || trimmed.contains(" Error")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_engine_line_is_an_error_and_a_clean_log_is_not() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp/rpt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let clean = dir.join("clean.rpt");
        let bad = dir.join("Arma3_x64_2026-10-24.rpt");
        std::fs::write(&clean, "Starting game.\n").unwrap();
        std::fs::write(&bad, "12:00:00 Error in expression <_x>\n").unwrap();
        // Both writes can land on the same timestamp, so make the clean report clearly older.
        File::options()
            .write(true)
            .open(&clean)
            .unwrap()
            .set_modified(SystemTime::now() - std::time::Duration::from_secs(60))
            .unwrap();
        assert!(!has_error(&clean));
        assert!(has_error(&bad));
        assert_eq!(newest(&dir), Some(bad));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
