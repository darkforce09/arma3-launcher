//! Images and fonts of the vanilla laun&r.take(len)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect::<Vec<_>>(),her, read from its resource assemblies in the user's
//! Arma 3 install, so none of Bohemia's files ship with this launcher.
//!
//! The WPF resources live in the En-us satellite assemblies as one embedded .NET `.resources`
//! blob each, keyed by lower-case paths such as `themes/defaultstyle/images/bg_blue.jpg`. The
//! assemblies are memory-mapped, so only the pages of resources actually used are read.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::Path;

use memmap2::Mmap;

/// The satellite assemblies, relative to the game directory.
const ASSEMBLIES: [&str; 2] = [
    "Launcher/En-us/arma3launcher.resources.dll",
    "Launcher/En-us/SharedResources.resources.dll",
];

/// `ResourceManager` header magic number.
const MAGIC: u32 = 0xBEEF_CACE;
/// `ResourceTypeCode` values of binary entries: `byte[]` and `Stream`.
const TYPE_BYTE_ARRAY: u32 = 0x20;
const TYPE_STREAM: u32 = 0x21;

pub struct Assets {
    maps: Vec<Mmap>,
    /// Resource name → (map index, byte range).
    entries: HashMap<String, (usize, std::ops::Range<usize>)>,
}

impl Assets {
    pub fn load(game_dir: &Path) -> io::Result<Self> {
        let mut maps = Vec::new();
        let mut entries = HashMap::new();
        for relative in ASSEMBLIES {
            let path = crate::fs_case::resolve(game_dir, relative).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{relative} in {}", game_dir.display()),
                )
            })?;
            // SAFETY: the game files are not modified while the launcher runs; Steam only updates
            // them while the game isn't running.
            let map = unsafe { Mmap::map(&File::open(&path)?)? };
            let index = maps.len();
            let blob_start = find_resources_blob(&map)
                .ok_or_else(|| invalid(format!("no .resources blob in {}", path.display())))?;
            for (name, range) in read_resources(&map[blob_start..])? {
                entries.insert(
                    name,
                    (index, range.start + blob_start..range.end + blob_start),
                );
            }
            maps.push(map);
        }
        Ok(Self { maps, entries })
    }

    /// The bytes of the resource `name` (lower-case path, e.g. `themes/defaultstyle/images/bg_blue.jpg`).
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        let (index, range) = self.entries.get(name)?;
        Some(&self.maps[*index][range.clone()])
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Offset of the first `.resources` blob: its magic number, preceded by the u32 length the
/// assembly's resource section stores before each blob.
fn find_resources_blob(bytes: &[u8]) -> Option<usize> {
    let magic = MAGIC.to_le_bytes();
    bytes.windows(4).position(|w| w == magic)
}

/// The binary entries of a `.resources` blob (starting at its magic number), with their byte
/// ranges relative to the blob.
fn read_resources(blob: &[u8]) -> io::Result<Vec<(String, std::ops::Range<usize>)>> {
    let mut r = Reader {
        bytes: blob,
        pos: 0,
    };
    if r.u32()? != MAGIC {
        return Err(invalid("bad .resources magic".into()));
    }
    let _manager_version = r.u32()?;
    let skip = r.u32()? as usize;
    r.pos += skip; // reader and resource-set type names
    let _set_version = r.u32()?;
    let count = r.u32()? as usize;
    let type_count = r.u32()? as usize;
    let mut type_names = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let len = r.var_u32()? as usize;
        type_names.push(r.take(len)?.to_vec());
    }
    // Padding ("PAD…") aligns the hash table to 8 bytes.
    r.pos = r.pos.next_multiple_of(8);
    r.pos += 4 * count; // name hashes
    let mut name_positions = Vec::with_capacity(count);
    for _ in 0..count {
        name_positions.push(r.u32()? as usize);
    }
    let data_section = r.u32()? as usize;
    let names_section = r.pos;

    let mut entries = Vec::with_capacity(count);
    for position in name_positions {
        r.pos = names_section + position;
        let len = r.var_u32()? as usize;
        let name = String::from_utf16_lossy(
            &r.take(len)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect::<Vec<_>>(),
        );
        let data_offset = r.u32()? as usize;
        r.pos = data_section + data_offset;
        let type_code = r.var_u32()?;
        if type_code == TYPE_STREAM || type_code == TYPE_BYTE_ARRAY {
            let len = r.u32()? as usize;
            let start = r.pos;
            r.take(len)?;
            entries.push((name, start..start + len));
        }
    }
    Ok(entries)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, len: usize) -> io::Result<&[u8]> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + len)
            .ok_or_else(|| invalid("truncated .resources".into()))?;
        self.pos += len;
        Ok(slice)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// .NET's 7-bit encoded integer.
    fn var_u32(&mut self) -> io::Result<u32> {
        let mut value = 0u32;
        for shift in (0..35).step_by(7) {
            let byte = self.take(1)?[0];
            value |= u32::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(invalid("bad 7-bit integer".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var_u32(mut v: u32, out: &mut Vec<u8>) {
        while v >= 0x80 {
            out.push((v as u8) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    /// A `.resources` blob laid out as .NET's `ResourceWriter` writes it.
    fn blob(entries: &[(&str, u32, &[u8])]) -> Vec<u8> {
        let mut names = Vec::new();
        let mut data = Vec::new();
        let mut positions = Vec::new();
        for (name, type_code, bytes) in entries {
            positions.push(names.len() as u32);
            let utf16: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            var_u32(utf16.len() as u32, &mut names);
            names.extend(&utf16);
            names.extend((data.len() as u32).to_le_bytes());
            var_u32(*type_code, &mut data);
            data.extend((bytes.len() as u32).to_le_bytes());
            data.extend(*bytes);
        }
        let types = b"reader-and-set-type-names";
        let mut out = Vec::new();
        out.extend(MAGIC.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend((types.len() as u32).to_le_bytes());
        out.extend(types);
        out.extend(2u32.to_le_bytes());
        out.extend((entries.len() as u32).to_le_bytes());
        out.extend(0u32.to_le_bytes());
        while out.len() % 8 != 0 {
            out.push(b'P');
        }
        out.extend(std::iter::repeat_n(0, 4 * entries.len()));
        for p in &positions {
            out.extend(p.to_le_bytes());
        }
        let data_section = (out.len() + 4 + names.len()) as u32;
        out.extend(data_section.to_le_bytes());
        out.extend(names);
        out.extend(data);
        out
    }

    #[test]
    fn reads_stream_entries_and_skips_others() {
        let png = b"\x89PNG fake";
        let font = b"OTTO fake";
        let blob = blob(&[
            ("themes/defaultstyle/images/arrow.png", TYPE_STREAM, png),
            ("app.baml-as-string", 0x01, b"x"),
            ("fonts/purista-light.otf", TYPE_BYTE_ARRAY, font),
        ]);
        let mut assembly = b"MZ...headers...".to_vec();
        assembly.extend((blob.len() as u32).to_le_bytes());
        assembly.extend(&blob);

        let start = find_resources_blob(&assembly).unwrap();
        let entries = read_resources(&assembly[start..]).unwrap();
        let found: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(n, r)| (n.as_str(), &assembly[start + r.start..start + r.end]))
            .collect();
        assert_eq!(
            found,
            [
                ("themes/defaultstyle/images/arrow.png", &png[..]),
                ("fonts/purista-light.otf", &font[..])
            ]
        );
    }
}
