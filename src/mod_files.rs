//! What the vanilla launcher reads from a mod's files for its details: whether the PBOs are signed
//! (`ModPropertyUpdater.ExtensionSetBisignFlag`) and which game extensions the mod ships
//! (`ExtensionLibrariesChecker`).

use std::path::{Path, PathBuf};

/// Both checks for one mod folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModFiles {
    pub signing: Signing,
    pub extensions: Extensions,
}

pub fn inspect(dir: &Path) -> ModFiles {
    ModFiles {
        signing: signing(dir),
        extensions: extensions(dir),
    }
}

/// `ExtensionSignatureStatus`: how many PBOs have a `.bisign` next to them. Only the files'
/// presence counts; vanilla doesn't check the signatures themselves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signing {
    None,
    Partial,
    Full,
}

impl Signing {
    /// `ModSignatureStatus_*`.
    pub fn label(self) -> &'static str {
        match self {
            Signing::None => "Not signed",
            Signing::Partial => "Partially signed",
            Signing::Full => "Signed",
        }
    }

    /// `ModSignatureStatus_*Desc` above `ModSignatureToolTip_Description`.
    pub fn tooltip(self) -> String {
        let status = match self {
            Signing::None => "There are no signature files in this mod.",
            Signing::Partial => "Some of the PBO files have signature files, but not all of them.",
            Signing::Full => "All PBO files have a signature file associated with them.",
        };
        format!(
            "{status}\n\nThe presence of signature files indicates that mod files (*.PBO) are not \
             modified by anyone but the author, which might be required by some game servers."
        )
    }
}

/// Every `*.pbo`/`*.ebo` in `addons` with an `<name>.*.bisign` beside it is signed; a mod without
/// PBOs is not signed.
pub fn signing(dir: &Path) -> Signing {
    let Some(addons) = crate::fs_case::resolve(dir, "addons") else {
        return Signing::None;
    };
    let Ok(entries) = std::fs::read_dir(addons) else {
        return Signing::None;
    };
    let names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
        .collect();
    let (mut all, mut any, mut pbos) = (true, false, 0);
    for name in &names {
        let Some(stem) = name
            .strip_suffix(".pbo")
            .or_else(|| name.strip_suffix(".ebo"))
        else {
            continue;
        };
        pbos += 1;
        let prefix = format!("{stem}.");
        let signed = names.iter().any(|n| {
            n.len() >= prefix.len() + ".bisign".len()
                && n.starts_with(&prefix)
                && n.ends_with(".bisign")
        });
        all &= signed;
        any |= signed;
    }
    match (pbos, all, any) {
        (0, _, _) => Signing::None,
        (_, true, _) => Signing::Full,
        (_, _, true) => Signing::Partial,
        _ => Signing::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bitness {
    Bit32,
    Bit64,
}

/// The game extensions in a mod: libraries named after their DLLs (`name.dll` for 32-bit,
/// `name_x64.dll` for 64-bit) that export `RVExtension`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extensions {
    /// A library has a 32-bit file, working or not.
    pub supports32: bool,
    /// A library has a working 32-bit file.
    pub ok32: bool,
    /// `PlatformViewModel.Is64BitSupported`: a library has a 64-bit file, working or not.
    pub supports64: bool,
    /// `PlatformViewModel.Is64BitOk`: a library has a working 64-bit file.
    pub ok64: bool,
    /// `ExtensionInfo.ContainsExtensions64`: there are libraries and each has a working 64-bit
    /// file.
    pub all64: bool,
    /// File names of library files that won't load: not a PE file, no `RVExtension` export or the
    /// wrong bitness for their name.
    pub corrupted: Vec<String>,
}

impl Extensions {
    /// `ExtensionInfo_32bit`, `ExtensionInfo_64bit` or both, else `ExtensionInfo_None`. Vanilla
    /// only shows the 64-bit one now; we show whichever files the mod has.
    pub fn label(&self) -> &'static str {
        match (self.supports32, self.supports64) {
            (true, true) => "32-bit, 64-bit",
            (true, false) => "32-bit",
            (false, true) => "64-bit",
            (false, false) => "(none)",
        }
    }

    /// `ExtensionViewModel.UpdateBitnessInfo`, with a line for 32-bit files as well.
    pub fn tooltip(&self) -> String {
        if !self.supports32 && !self.supports64 {
            return "This mod does not contain any game extensions.".to_owned();
        }
        let mut text = String::new();
        for (supported, ok, bits) in [
            (self.supports32, self.ok32, 32),
            (self.supports64, self.ok64, 64),
        ] {
            if !supported {
                continue;
            }
            text.push_str(&format!("This mod contains {bits}-bit extensions"));
            text.push_str(if ok {
                ".\n"
            } else {
                ", but some of them are corrupted.\n"
            });
        }
        if !self.corrupted.is_empty() {
            text.push_str("\nCorrupted game extensions:");
            for name in &self.corrupted {
                text.push_str(&format!("\n- {name}"));
            }
        }
        text.trim().to_owned()
    }
}

/// `ExtensionLibrariesChecker.Check` over the DLLs directly in the mod folder.
pub fn extensions(dir: &Path) -> Extensions {
    let dlls: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(name, _)| name.to_ascii_lowercase().ends_with(".dll"))
        .collect();
    let infos: Vec<Option<PeInfo>> = dlls.iter().map(|(_, path)| pe_info(path)).collect();

    // A library is any extension DLL's name without `.dll` and `_x64`.
    let mut libraries: Vec<&str> = Vec::new();
    for ((name, _), info) in dlls.iter().zip(&infos) {
        if info.as_ref().is_some_and(PeInfo::is_extension) {
            let stem = &name[..name.len() - ".dll".len()];
            let library = strip_suffix_ignore_case(stem, "_x64");
            if !libraries.iter().any(|l| l.eq_ignore_ascii_case(library)) {
                libraries.push(library);
            }
        }
    }

    let mut out = Extensions {
        all64: !libraries.is_empty(),
        ..Extensions::default()
    };
    for library in libraries {
        let mut has64 = false;
        for (bitness, suffix) in [(Bitness::Bit32, ""), (Bitness::Bit64, "_x64")] {
            let file = format!("{library}{suffix}.dll");
            let Some(i) = dlls.iter().position(|(n, _)| n.eq_ignore_ascii_case(&file)) else {
                continue;
            };
            let correct = infos[i]
                .as_ref()
                .is_some_and(|pe| pe.is_extension() && pe.bitness == Some(bitness));
            match bitness {
                Bitness::Bit32 => {
                    out.supports32 = true;
                    out.ok32 |= correct;
                }
                Bitness::Bit64 => {
                    out.supports64 = true;
                    out.ok64 |= correct;
                    has64 |= correct;
                }
            }
            if !correct {
                out.corrupted.push(dlls[i].0.clone());
            }
        }
        out.all64 &= has64;
    }
    out
}

fn strip_suffix_ignore_case<'a>(text: &'a str, suffix: &str) -> &'a str {
    let split = text.len().saturating_sub(suffix.len());
    match text.get(split..) {
        Some(end) if end.eq_ignore_ascii_case(suffix) => &text[..split],
        _ => text,
    }
}

/// What `PortableExecutableParser` reads from a DLL.
struct PeInfo {
    /// `None` for an MZ file without a PE header.
    bitness: Option<Bitness>,
    exports: Vec<String>,
}

impl PeInfo {
    /// `ContainsRealVirtualityExtension`: the entry points the game calls, decorated in 32-bit
    /// DLLs.
    fn is_extension(&self) -> bool {
        let names: &[&str] = match self.bitness {
            Some(Bitness::Bit32) => &["_RVExtension@12", "_RVExtensionArgs@20"],
            Some(Bitness::Bit64) => &["RVExtension", "RVExtensionArgs"],
            None => &[],
        };
        self.exports
            .iter()
            .any(|e| names.iter().any(|n| e.eq_ignore_ascii_case(n)))
    }
}

/// `None` when the file can't be read or isn't a PE file.
fn pe_info(path: &Path) -> Option<PeInfo> {
    parse_pe(&std::fs::read(path).ok()?)
}

fn parse_pe(data: &[u8]) -> Option<PeInfo> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?));
    if data.get(..2)? != b"MZ" {
        return None;
    }
    let pe = u32_at(0x3C)? as usize;
    if data.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Some(PeInfo {
            bitness: None,
            exports: Vec::new(),
        });
    }
    let file_header = pe + 4;
    // Like vanilla: anything that isn't i386 counts as 64-bit.
    let bitness = if u16_at(file_header)? == 0x14C {
        Bitness::Bit32
    } else {
        Bitness::Bit64
    };
    let section_count = u16_at(file_header + 2)? as usize;
    let optional = file_header + 20;
    let sections = optional + u16_at(file_header + 16)? as usize;
    // The data directories follow the optional header's fixed part, longer in PE32+.
    let directories = optional + if u16_at(optional)? == 0x20B { 112 } else { 96 };
    let directory_count = u32_at(directories - 4)?;
    let mut exports = Vec::new();
    if directory_count > 0 && u32_at(directories + 4)? != 0 {
        // An RVA is in the last section whose raw data covers it.
        let offset = |rva: u32| -> Option<usize> {
            let section = (0..section_count)
                .map(|i| sections + i * 40)
                .filter_map(|s| Some((u32_at(s + 12)?, u32_at(s + 16)?, u32_at(s + 20)?)))
                .rfind(|&(start, size, _)| rva >= start && rva - start <= size)?;
            Some((rva - section.0) as usize + section.2 as usize)
        };
        let directory = offset(u32_at(directories)?)?;
        let names = offset(u32_at(directory + 32)?)?;
        for i in 0..u32_at(directory + 24)? as usize {
            let start = offset(u32_at(names + i * 4)?)?;
            let len = data.get(start..)?.iter().position(|&b| b == 0)?;
            exports.push(String::from_utf8_lossy(&data[start..start + len]).into_owned());
        }
    }
    Some(PeInfo {
        bitness: Some(bitness),
        exports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-tmp")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal DLL: headers, then one section holding the export directory and `names`.
    fn dll(bitness: Bitness, names: &[&str]) -> Vec<u8> {
        let (machine, magic, fixed) = match bitness {
            Bitness::Bit32 => (0x14Cu16, 0x10Bu16, 96usize),
            Bitness::Bit64 => (0x8664, 0x20B, 112),
        };
        let mut data = vec![0u8; 0x200];
        let put16 =
            |d: &mut Vec<u8>, at: usize, v: u16| d[at..at + 2].copy_from_slice(&v.to_le_bytes());
        let put32 =
            |d: &mut Vec<u8>, at: usize, v: u32| d[at..at + 4].copy_from_slice(&v.to_le_bytes());
        data[..2].copy_from_slice(b"MZ");
        put32(&mut data, 0x3C, 0x40);
        data[0x40..0x44].copy_from_slice(b"PE\0\0");
        put16(&mut data, 0x44, machine);
        put16(&mut data, 0x46, 1);
        let optional_size = fixed + 16 * 8;
        put16(&mut data, 0x54, optional_size as u16);
        put16(&mut data, 0x58, magic);
        put32(&mut data, 0x58 + fixed - 4, 16);
        put32(&mut data, 0x58 + fixed, 0x1000);
        put32(&mut data, 0x58 + fixed + 4, 40);
        let section = 0x58 + optional_size;
        put32(&mut data, section + 12, 0x1000);
        put32(&mut data, section + 16, 0x200);
        put32(&mut data, section + 20, 0x200);

        // Section data at file offset 0x200, RVA 0x1000.
        let mut raw = vec![0u8; 40];
        put32(&mut raw, 24, names.len() as u32);
        put32(&mut raw, 32, 0x1000 + 40);
        let strings = 40 + names.len() * 4;
        raw.resize(strings, 0);
        for (i, name) in names.iter().enumerate() {
            let rva = 0x1000 + raw.len() as u32;
            put32(&mut raw, 40 + i * 4, rva);
            raw.extend_from_slice(name.as_bytes());
            raw.push(0);
        }
        raw.resize(0x200, 0);
        data.extend(raw);
        data
    }

    #[test]
    fn pbos_with_a_bisign_each_are_signed() {
        let dir = tmp("signing");
        let addons = dir.join("Addons");
        fs::create_dir_all(&addons).unwrap();
        // No PBOs yet: not signed.
        assert_eq!(signing(&dir), Signing::None);

        fs::write(addons.join("A.PBO"), b"").unwrap();
        fs::write(addons.join("b.ebo"), b"").unwrap();
        fs::write(addons.join("a.pbo.tag_1.0.BISIGN"), b"").unwrap();
        assert_eq!(signing(&dir), Signing::Partial);

        fs::write(addons.join("b.ebo.tag_1.0.bisign"), b"").unwrap();
        assert_eq!(signing(&dir), Signing::Full);

        // A bisign of another PBO with the same prefix doesn't count.
        fs::write(addons.join("ab.pbo"), b"").unwrap();
        assert_eq!(signing(&dir), Signing::Partial);

        assert_eq!(signing(&dir.join("missing")), Signing::None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_bitness_and_exports_of_dlls() {
        let pe = parse_pe(&dll(Bitness::Bit32, &["_RVExtension@12", "Other"])).unwrap();
        assert_eq!(pe.bitness, Some(Bitness::Bit32));
        assert_eq!(pe.exports, ["_RVExtension@12", "Other"]);
        assert!(pe.is_extension());

        let pe = parse_pe(&dll(Bitness::Bit64, &["RVExtensionArgs"])).unwrap();
        assert_eq!(pe.bitness, Some(Bitness::Bit64));
        assert!(pe.is_extension());

        // Undecorated names don't load in 32-bit.
        assert!(
            !parse_pe(&dll(Bitness::Bit32, &["RVExtension"]))
                .unwrap()
                .is_extension()
        );
        assert!(parse_pe(b"not a dll").is_none());
    }

    #[test]
    fn extensions_are_listed_per_bitness() {
        let dir = tmp("extensions");
        // No DLLs.
        assert_eq!(extensions(&dir).label(), "(none)");

        // A DLL without RVExtension is no game extension.
        fs::write(dir.join("helper_x64.dll"), dll(Bitness::Bit64, &["Init"])).unwrap();
        assert_eq!(extensions(&dir), Extensions::default());

        fs::write(
            dir.join("Radio_x64.DLL"),
            dll(Bitness::Bit64, &["RVExtension"]),
        )
        .unwrap();
        let found = extensions(&dir);
        assert_eq!(found.label(), "64-bit");
        assert!(found.all64 && found.ok64 && !found.supports32);

        fs::write(
            dir.join("radio.dll"),
            dll(Bitness::Bit32, &["_RVExtension@12"]),
        )
        .unwrap();
        let found = extensions(&dir);
        assert_eq!(found.label(), "32-bit, 64-bit");
        assert!(found.all64 && found.ok32 && found.corrupted.is_empty());

        // A 32-bit file named for 64-bit: there, but broken.
        fs::remove_file(dir.join("Radio_x64.DLL")).unwrap();
        fs::write(
            dir.join("radio_x64.dll"),
            dll(Bitness::Bit32, &["_RVExtension@12"]),
        )
        .unwrap();
        let found = extensions(&dir);
        assert!(found.supports64 && !found.ok64 && !found.all64);
        assert_eq!(found.corrupted, ["radio_x64.dll"]);
        assert!(
            found
                .tooltip()
                .ends_with("Corrupted game extensions:\n- radio_x64.dll")
        );

        fs::remove_file(dir.join("radio_x64.dll")).unwrap();
        assert_eq!(extensions(&dir).label(), "32-bit");
        fs::remove_dir_all(dir).unwrap();
    }
}
