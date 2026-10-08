//! F2's package: one project as one file, a small versioned container.
//!
//! ```text
//! header (16 B)  "LPK1" | version u16 = 1 | count u16 | index_len u32 | index_crc u32
//! index          count × { path_len u16 | path (relative to the slot) |
//!                          offset u32 | stored_len u32 | raw_len u32 |
//!                          raw_crc u32 | codec u8 }
//! members        stored bytes, back to back, at their offsets
//! ```
//!
//! All integers little-endian; offsets from the start of the file. `codec` 0 is
//! stored, 1 is raw deflate (RFC 1951). The index is sorted by path. littlefs
//! has no checksum on file data, so the index carries a CRC and each member
//! the CRC of its raw bytes.

/// The package's magic.
pub const PKG_MAGIC: &[u8; 4] = b"LPK1";
/// The only version there is.
pub const PKG_VERSION: u16 = 1;
/// Header bytes before the index.
pub const PKG_HEADER: usize = 16;

/// How a member's bytes are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberCodec {
    Stored = 0,
    Deflate = 1,
}

/// One index entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PkgMember {
    pub path: String,
    pub offset: u32,
    pub stored_len: u32,
    pub raw_len: u32,
    pub raw_crc: u32,
    pub codec: MemberCodec,
}

/// A parsed header: member count and index length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PkgHeader {
    pub count: u16,
    pub index_len: u32,
    pub index_crc: u32,
}

impl PkgHeader {
    pub fn parse(h: &[u8]) -> Result<Self, String> {
        if h.len() < PKG_HEADER || &h[..4] != PKG_MAGIC {
            return Err("bad package magic".into());
        }
        let version = u16::from_le_bytes([h[4], h[5]]);
        if version != PKG_VERSION {
            return Err(format!("package version {version}"));
        }
        Ok(Self {
            count: u16::from_le_bytes([h[6], h[7]]),
            index_len: u32::from_le_bytes(h[8..12].try_into().unwrap()),
            index_crc: u32::from_le_bytes(h[12..16].try_into().unwrap()),
        })
    }
}

/// The header and index for `members` (offsets already set).
pub fn encode_head(members: &[PkgMember]) -> Vec<u8> {
    let index = encode_index(members);
    let mut out = Vec::with_capacity(PKG_HEADER + index.len());
    out.extend_from_slice(PKG_MAGIC);
    out.extend_from_slice(&PKG_VERSION.to_le_bytes());
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(index.len() as u32).to_le_bytes());
    out.extend_from_slice(&lp_crc32::crc32(&index).to_le_bytes());
    out.extend_from_slice(&index);
    out
}

/// Bytes the header and index of `members` take (to lay out offsets first).
pub fn head_len(members: &[PkgMember]) -> usize {
    PKG_HEADER + members.iter().map(|m| 2 + m.path.len() + 17).sum::<usize>()
}

fn encode_index(members: &[PkgMember]) -> Vec<u8> {
    let mut out = Vec::new();
    for m in members {
        out.extend_from_slice(&(m.path.len() as u16).to_le_bytes());
        out.extend_from_slice(m.path.as_bytes());
        for w in [m.offset, m.stored_len, m.raw_len, m.raw_crc] {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.push(m.codec as u8);
    }
    out
}

/// Parse an index checked against its header.
pub fn decode_index(h: &PkgHeader, mut b: &[u8]) -> Result<Vec<PkgMember>, String> {
    if lp_crc32::crc32(b) != h.index_crc {
        return Err("package index crc".into());
    }
    let mut out = Vec::with_capacity(h.count as usize);
    for _ in 0..h.count {
        let short = || "package index truncated".to_string();
        let pl = u16::from_le_bytes(b.get(..2).ok_or_else(short)?.try_into().unwrap()) as usize;
        let path = std::str::from_utf8(b.get(2..2 + pl).ok_or_else(short)?)
            .map_err(|_| "package path not utf-8")?
            .to_string();
        let rest = b.get(2 + pl..2 + pl + 17).ok_or_else(short)?;
        let w = |i: usize| u32::from_le_bytes(rest[i * 4..i * 4 + 4].try_into().unwrap());
        let codec = match rest[16] {
            0 => MemberCodec::Stored,
            1 => MemberCodec::Deflate,
            c => return Err(format!("package codec {c}")),
        };
        out.push(PkgMember {
            path,
            offset: w(0),
            stored_len: w(1),
            raw_len: w(2),
            raw_crc: w(3),
            codec,
        });
        b = &b[2 + pl + 17..];
    }
    if !b.is_empty() {
        return Err("package index has trailing bytes".into());
    }
    Ok(out)
}

/// A member's bytes as written: deflated (raw, best level) when that is
/// smaller, else stored. Returns (codec, stored bytes).
pub fn encode_member(raw: &[u8]) -> (MemberCodec, Vec<u8>) {
    let z = miniz_oxide::deflate::compress_to_vec(raw, 9);
    if z.len() < raw.len() {
        (MemberCodec::Deflate, z)
    } else {
        (MemberCodec::Stored, raw.to_vec())
    }
}

/// A member's raw bytes from its stored bytes, decoded with `lp-deflate` (the
/// device's own decoder) and checked against the index.
pub fn decode_member(m: &PkgMember, stored: &[u8]) -> Result<Vec<u8>, String> {
    let raw = match m.codec {
        MemberCodec::Stored => stored.to_vec(),
        MemberCodec::Deflate => {
            let mut buf = vec![0u8; m.raw_len as usize];
            let n = lp_deflate::inflate(stored, &mut buf, 0)
                .map_err(|e| format!("inflate {}: {e:?}", m.path))?;
            buf.truncate(n);
            buf
        }
    };
    if raw.len() != m.raw_len as usize || lp_crc32::crc32(&raw) != m.raw_crc {
        return Err(format!("member {} fails its crc", m.path));
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_round_trips_and_member_codecs_decode_with_lp_deflate() {
        let a = b"{\n  \"a\": 1,\n  \"a\": 1,\n  \"a\": 1,\n  \"a\": 1\n}\n".repeat(8);
        let b = b"xy".to_vec();
        let mut members = Vec::new();
        for (p, raw) in [("a.json", &a[..]), ("m/b.glsl", &b[..])] {
            let (codec, stored) = encode_member(raw);
            members.push(PkgMember {
                path: p.into(),
                offset: 0,
                stored_len: stored.len() as u32,
                raw_len: raw.len() as u32,
                raw_crc: lp_crc32::crc32(raw),
                codec,
            });
            assert_eq!(
                decode_member(members.last().unwrap(), &stored).unwrap(),
                raw
            );
        }
        assert_eq!(members[0].codec, MemberCodec::Deflate);
        assert_eq!(members[1].codec, MemberCodec::Stored);
        let head = encode_head(&members);
        assert_eq!(head.len(), head_len(&members));
        let h = PkgHeader::parse(&head).unwrap();
        assert_eq!(decode_index(&h, &head[PKG_HEADER..]).unwrap(), members);
        let mut bad = head.clone();
        bad[PKG_HEADER + 3] ^= 1;
        assert!(decode_index(&h, &bad[PKG_HEADER..]).is_err());
    }
}
