//! `O`, host→board: an offer of one build (DM10, `one-way-doors.md` §5).
//!
//! ```text
//! 'O' proto:u8 flags:u8 chip:u16 layout:u16 min_loader:u16
//!     core_len:u32 engine_len:u32 core_sha256[32] engine_sha256[32] build_id[64]
//! ```
//!
//! 145 bytes with the type byte. The board checks it **before erasing
//! anything** (E8): must-understand flags, chip, layout (equal), loader
//! (≥ `min_loader`), and fit. The **install kind is decided by the hashes**
//! ([`crate::install_kind`]), never by `build_id`: the build id is a label,
//! and the input of the build hash the boot and progress records key on.

use alloc::vec::Vec;

use crate::build_id::{BUILD_ID_LEN, build_hash_of_field, build_id_text};
use crate::wire_reader::WireReader;

/// The `O` message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    /// Information: the host's protocol version ([`crate::code_table::PROTO_V1`]).
    pub proto: u8,
    /// See [`crate::flag_rule`]. v1 defines no offer flag.
    pub flags: u8,
    /// [`crate::code_table`]'s chip code.
    pub chip: u16,
    /// The layout the build was linked for; must equal the board's.
    pub layout: u16,
    /// The oldest loader the build's boot records need; the board's must be
    /// at least this.
    pub min_loader: u16,
    pub core_len: u32,
    pub engine_len: u32,
    /// SHA-256 of `core.bin` ([`crate::hash_rules`]).
    pub core_sha256: [u8; 32],
    /// SHA-256 of `engine.bin` as flashed ([`crate::hash_rules`]).
    pub engine_sha256: [u8; 32],
    /// `<version>+<commit[..12]>`, zero-padded.
    pub build_id: [u8; BUILD_ID_LEN],
}

impl Offer {
    /// Bytes after the type byte.
    pub const BODY_LEN: usize = 1 + 1 + 2 + 2 + 2 + 4 + 4 + 32 + 32 + BUILD_ID_LEN;

    /// The build id's text, without its padding.
    #[must_use]
    pub fn build_id_text(&self) -> &[u8] {
        build_id_text(&self.build_id)
    }

    /// The build hash of the offered build ([`crate::build_id`]).
    #[must_use]
    pub fn build_hash(&self) -> u32 {
        build_hash_of_field(&self.build_id)
    }

    /// The whole message, type byte first.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + Self::BODY_LEN);
        out.push(b'O');
        out.push(self.proto);
        out.push(self.flags);
        out.extend_from_slice(&self.chip.to_le_bytes());
        out.extend_from_slice(&self.layout.to_le_bytes());
        out.extend_from_slice(&self.min_loader.to_le_bytes());
        out.extend_from_slice(&self.core_len.to_le_bytes());
        out.extend_from_slice(&self.engine_len.to_le_bytes());
        out.extend_from_slice(&self.core_sha256);
        out.extend_from_slice(&self.engine_sha256);
        out.extend_from_slice(&self.build_id);
        out
    }

    pub(crate) fn decode(r: &mut WireReader<'_>) -> Option<Self> {
        Some(Self {
            proto: r.u8()?,
            flags: r.u8()?,
            chip: r.u16()?,
            layout: r.u16()?,
            min_loader: r.u16()?,
            core_len: r.u32()?,
            engine_len: r.u32()?,
            core_sha256: r.array()?,
            engine_sha256: r.array()?,
            build_id: r.array()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_144_bytes() {
        assert_eq!(Offer::BODY_LEN, 144);
    }
}
