//! Arma's A2S_RULES payload (`ServerRulesService` in the vanilla launcher's SteamLayer): the
//! server's difficulty settings, DLC, mods and signatures, split over 2-byte rule keys and escaped
//! so that no zero byte appears.

/// Server settings and content from the rules reply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    /// 0 Recruit, 1 Regular, 2 Veteran, else Custom.
    pub difficulty: u8,
    /// 0 Novice, 1 Normal, 2 Expert, else Custom.
    pub ai_level: u8,
    pub advanced_flight_model: bool,
    pub third_person: bool,
    pub crosshair: bool,
    /// Bits: 0 Kart, 1 Marksmen, 2 Heli, 3 Zeus, 4 Apex, 5 Jets, 6 Laws of War, 7 Argo, 8 Tac-Ops,
    /// 9 Tanks, 10 Contact, 11 Enoch, 12 Art of War.
    pub dlc_flags: u16,
    pub requires_apex_terrain: bool,
    pub requires_contact_terrain: bool,
    /// The mod list was too long to send in full.
    pub mods_overflow: bool,
    pub mods: Vec<ServerMod>,
    pub signatures: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerMod {
    pub hash: u32,
    /// The workshop id, or the app id of an optional DLC.
    pub published_id: u64,
    pub optional_dlc: bool,
    pub name: String,
}

const MAX_PARTS: usize = 64;

/// Joins the rule values in part order. Keys are 2 bytes: part index + 1, then the part count.
pub fn assemble(rules: &[(Vec<u8>, Vec<u8>)]) -> Option<Vec<u8>> {
    let total = *rules.first()?.0.get(1)? as usize;
    if total == 0 || total > MAX_PARTS {
        return None;
    }
    let mut parts: Vec<Option<&[u8]>> = vec![None; total];
    for (key, value) in rules {
        if key.len() != 2 || key[1] as usize != total {
            return None;
        }
        let index = (key[0] as i8 as i32 - 1) as usize;
        *parts.get_mut(index)? = Some(value);
    }
    Some(parts.into_iter().flatten().flatten().copied().collect())
}

/// Undoes the escaping: `01 01` → `01`, `01 02` → `00`, `01 03` → `FF`.
pub fn unescape(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut bytes = data.iter();
    while let Some(&b) = bytes.next() {
        if b == 0x01 {
            out.push(match bytes.next()? {
                0x01 => 0x01,
                0x02 => 0x00,
                0x03 => 0xFF,
                _ => return None,
            });
        } else {
            out.push(b);
        }
    }
    Some(out)
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from(self.u8()?) | u16::from(self.u8()?) << 8)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from(self.u16()?) | u32::from(self.u16()?) << 16)
    }

    fn bytes(&mut self, n: usize) -> Option<&[u8]> {
        let slice = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(slice)
    }

    fn string(&mut self) -> Option<String> {
        let len = self.u8()? as usize;
        Some(String::from_utf8_lossy(self.bytes(len)?).into_owned())
    }
}

/// Decodes the unescaped payload. `None` for an unknown version or truncated data.
pub fn decode(data: &[u8]) -> Option<Rules> {
    let mut r = Reader { data, pos: 0 };
    let version = r.u8()?;
    if version != 2 && version != 3 {
        return None;
    }
    let flags = r.u8()?;
    let dlc_flags = r.u16()?;
    let settings = r.u8()?;
    let crosshair = r.u8()?;
    let mut rules = Rules {
        difficulty: settings & 0x07,
        ai_level: (settings >> 3) & 0x07,
        advanced_flight_model: settings & 0xC0 != 0,
        third_person: settings >> 7 != 0,
        crosshair: crosshair != 0,
        dlc_flags,
        requires_apex_terrain: flags & 0x04 != 0,
        requires_contact_terrain: flags & 0x08 != 0,
        mods_overflow: flags & 0x01 != 0,
        ..Default::default()
    };
    // One hash per DLC bit set.
    for bit in 0..13 {
        if dlc_flags & (1 << bit) != 0 {
            r.u32()?;
        }
    }
    if flags & 0x01 != 0 {
        return Some(rules);
    }
    let count = r.u8()?;
    for _ in 0..count {
        let hash = r.u32()?;
        let b = r.u8()?;
        let id_len = (b & 0x0F) as usize;
        let id = r
            .bytes(id_len)?
            .iter()
            .rev()
            .fold(0u64, |acc, &x| acc << 8 | u64::from(x));
        let name = r.string()?;
        rules.mods.push(ServerMod {
            hash,
            published_id: id,
            optional_dlc: (b >> 4) & 1 != 0,
            name,
        });
    }
    if flags & 0x02 != 0 {
        return Some(rules);
    }
    if let Some(count) = r.u8() {
        for _ in 0..count {
            rules.signatures.push(r.string()?);
        }
    }
    Some(rules)
}

/// Rules from the raw key/value pairs Steam returns.
pub fn parse(rules: &[(Vec<u8>, Vec<u8>)]) -> Option<Rules> {
    decode(&unescape(&assemble(rules)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escape(data: &[u8]) -> Vec<u8> {
        data.iter()
            .flat_map(|&b| match b {
                0x01 => vec![0x01, 0x01],
                0x00 => vec![0x01, 0x02],
                0xFF => vec![0x01, 0x03],
                b => vec![b],
            })
            .collect()
    }

    #[test]
    fn decodes_a_split_escaped_payload() {
        let mut payload = vec![3, 0x00, 0x00, 0x04, 0b1000_1001, 1];
        payload.extend(0xAABB_CCDDu32.to_le_bytes()); // Contact (bit 10) hash
        payload.push(2); // two mods
        payload.extend(0x0102_0304u32.to_le_bytes());
        payload.push(4); // id length 4
        payload.extend(450814997u32.to_le_bytes());
        payload.push(6);
        payload.extend(b"CBA_A3");
        payload.extend(0u32.to_le_bytes());
        payload.push(0x10 | 3); // optional DLC, id length 3
        payload.extend(&1042220u32.to_le_bytes()[..3]);
        payload.push(19);
        payload.extend(b"Global Mobilization");
        payload.push(1);
        payload.push(3);
        payload.extend(b"ace");

        let escaped = escape(&payload);
        let (a, b) = escaped.split_at(escaped.len() / 2);
        // Parts arrive out of order.
        let rules = parse(&[(vec![2, 2], b.to_vec()), (vec![1, 2], a.to_vec())]).unwrap();
        assert_eq!(
            (
                rules.difficulty,
                rules.ai_level,
                rules.third_person,
                rules.advanced_flight_model,
                rules.crosshair
            ),
            (1, 1, true, true, true)
        ); // vanilla masks 0xC0, so the third-person bit also sets the flight model
        assert_eq!(rules.dlc_flags, 0x0400);
        assert_eq!(rules.mods.len(), 2);
        assert_eq!(
            rules.mods[0],
            ServerMod {
                hash: 0x0102_0304,
                published_id: 450814997,
                optional_dlc: false,
                name: "CBA_A3".into()
            }
        );
        assert_eq!(
            (rules.mods[1].published_id, rules.mods[1].optional_dlc),
            (1042220, true)
        );
        assert_eq!(rules.signatures, ["ace"]);
    }

    #[test]
    fn rejects_bad_keys_escapes_and_versions() {
        assert_eq!(assemble(&[(vec![1, 2, 3], vec![])]), None);
        assert_eq!(
            assemble(&[(vec![1, 2], vec![1]), (vec![2, 3], vec![2])]),
            None
        );
        assert_eq!(unescape(&[0x01, 0x07]), None);
        assert_eq!(unescape(&[0x41, 0x01]), None);
        assert_eq!(decode(&[1, 0, 0, 0, 0, 0]), None);
    }
}
