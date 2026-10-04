//! A build the host holds and can serve: its identity, `core.bin`,
//! `engine.bin`, and optionally each piece in encoding 1.
//!
//! [`HostBuild::from_parts`] builds one from parts. Part B adds
//! `from_ota_manifest`, which reads the firmware-distribution plan's
//! `ota-manifest.json` and its files and picks the encoding by `id` alone;
//! nothing here reads the split image's package `split` block.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_update::build_id::{BUILD_ID_LEN, build_hash, build_id_field};
use lpc_update::code_table::{CHUNK, PROTO_V1, chip_code};
use lpc_update::hash_rules::{core_sha256, engine_sha256};
use lpc_update::{Offer, PieceKind};

use crate::encoded_piece::{EncodedPiece, EncodedPieceError};

/// What a build says about itself: the identity fields of
/// `ota-manifest.json` (doors #2/#3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostIdentity {
    /// The opaque target name. Compared only to say "another target"; never
    /// parsed.
    pub target: String,
    /// The chip word (`"esp32c6"`).
    pub chip: String,
    pub version: String,
    /// `<version>+<commit[..12]>`.
    pub build_id: String,
    pub wire_proto: u32,
    /// `requires.layout`: the board's must be equal.
    pub layout: u16,
    /// `requires.loader`: the board's must be at least this.
    pub min_loader: u16,
}

/// One piece the host holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostPiece {
    pub bytes: Vec<u8>,
    /// By the hash rules (`lpc_update::hash_rules`), computed here.
    pub sha256: [u8; 32],
    /// Encoding 1 of this piece, if the host has it.
    pub encoded: Option<EncodedPiece>,
    /// Where each chunk's stream starts in `encoded.stream`.
    offsets: Vec<u32>,
}

impl HostPiece {
    /// Chunk `idx`'s raw bytes.
    #[must_use]
    pub fn raw_chunk(&self, idx: u32) -> Option<&[u8]> {
        let start = (idx * CHUNK) as usize;
        let end = (start + CHUNK as usize).min(self.bytes.len());
        self.bytes.get(start..end).filter(|b| !b.is_empty())
    }

    /// Chunk `idx` in encoding 1, if it has a compressed form.
    #[must_use]
    pub fn encoded_chunk(&self, idx: u32) -> Option<&[u8]> {
        let e = self.encoded.as_ref()?;
        let len = *e.chunks.get(idx as usize)? as usize;
        if len == 0 {
            return None;
        }
        let start = *self.offsets.get(idx as usize)? as usize;
        e.stream.get(start..start + len)
    }

    /// Chunks in the piece.
    #[must_use]
    pub fn chunk_count(&self) -> u32 {
        (self.bytes.len() as u32).div_ceil(CHUNK)
    }
}

/// Why parts do not make a build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostBuildError {
    /// The chip word is not in `lpc_update`'s code table.
    UnknownChip(String),
    /// The build id is longer than its 64-byte field, or holds a zero byte.
    BuildId,
    /// A piece is empty.
    EmptyPiece(PieceKind),
    /// An encoding does not describe its piece.
    Encoding(PieceKind, EncodedPieceError),
}

/// A build the host can offer and serve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostBuild {
    pub identity: HostIdentity,
    pub core: HostPiece,
    pub engine: HostPiece,
    chip_code: u16,
    build_id_field: [u8; BUILD_ID_LEN],
}

impl HostBuild {
    /// A build from its identity, `core.bin`, `engine.bin`, and each piece's
    /// encoding 1 if the host has it. The hashes are computed here; the
    /// chip, the build id and each encoding's shape are checked.
    pub fn from_parts(
        identity: HostIdentity,
        core: Vec<u8>,
        engine: Vec<u8>,
        core_encoded: Option<EncodedPiece>,
        engine_encoded: Option<EncodedPiece>,
    ) -> Result<Self, HostBuildError> {
        let chip_code = chip_code(&identity.chip)
            .ok_or_else(|| HostBuildError::UnknownChip(identity.chip.clone()))?;
        let build_id_field =
            build_id_field(identity.build_id.as_bytes()).ok_or(HostBuildError::BuildId)?;
        let core = piece(PieceKind::Core, core_sha256(&core), core, core_encoded)?;
        let engine = piece(
            PieceKind::Engine,
            engine_sha256(&engine),
            engine,
            engine_encoded,
        )?;
        Ok(Self {
            identity,
            core,
            engine,
            chip_code,
            build_id_field,
        })
    }

    /// The piece of `kind`.
    #[must_use]
    pub fn piece(&self, kind: PieceKind) -> &HostPiece {
        match kind {
            PieceKind::Core => &self.core,
            PieceKind::Engine => &self.engine,
        }
    }

    /// The build hash the board's records key on (`refusedBuild`).
    #[must_use]
    pub fn build_hash(&self) -> u32 {
        build_hash(self.identity.build_id.as_bytes())
    }

    /// The offer of this build (`O`, flags 0: v1 defines no offer flag).
    #[must_use]
    pub fn offer(&self) -> Offer {
        Offer {
            proto: PROTO_V1,
            flags: 0,
            chip: self.chip_code,
            layout: self.identity.layout,
            min_loader: self.identity.min_loader,
            core_len: self.core.bytes.len() as u32,
            engine_len: self.engine.bytes.len() as u32,
            core_sha256: self.core.sha256,
            engine_sha256: self.engine.sha256,
            build_id: self.build_id_field,
        }
    }
}

fn piece(
    kind: PieceKind,
    sha256: [u8; 32],
    bytes: Vec<u8>,
    encoded: Option<EncodedPiece>,
) -> Result<HostPiece, HostBuildError> {
    if bytes.is_empty() {
        return Err(HostBuildError::EmptyPiece(kind));
    }
    if let Some(e) = &encoded {
        e.check(bytes.len())
            .map_err(|err| HostBuildError::Encoding(kind, err))?;
    }
    let offsets = encoded
        .as_ref()
        .map(EncodedPiece::offsets)
        .unwrap_or_default();
    Ok(HostPiece {
        bytes,
        sha256,
        encoded,
        offsets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    pub(crate) fn identity() -> HostIdentity {
        HostIdentity {
            target: "esp32c6-4mb".into(),
            chip: "esp32c6".into(),
            version: "2026.10.06-1".into(),
            build_id: "2026.10.06-1+abcdefabcdef".into(),
            wire_proto: 36,
            layout: 1,
            min_loader: 1,
        }
    }

    #[test]
    fn parts_make_a_build_and_its_offer() {
        let b =
            HostBuild::from_parts(identity(), vec![1; 5000], vec![2; 9000], None, None).unwrap();
        let o = b.offer();
        assert_eq!((o.chip, o.layout, o.min_loader), (1, 1, 1));
        assert_eq!((o.core_len, o.engine_len), (5000, 9000));
        assert_eq!(o.core_sha256, core_sha256(&[1; 5000]));
        assert_eq!(o.build_id_text(), b"2026.10.06-1+abcdefabcdef");
        assert_eq!(b.core.raw_chunk(1).unwrap().len(), 5000 - 4096);
        assert_eq!(b.core.raw_chunk(2), None);
    }

    #[test]
    fn bad_parts_are_refused() {
        let mut id = identity();
        id.chip = "esp32".into();
        assert!(matches!(
            HostBuild::from_parts(id, vec![1], vec![2], None, None),
            Err(HostBuildError::UnknownChip(_))
        ));
        let mut id = identity();
        id.build_id = "x".repeat(65);
        assert_eq!(
            HostBuild::from_parts(id, vec![1], vec![2], None, None),
            Err(HostBuildError::BuildId)
        );
        let wrong = EncodedPiece {
            stream: vec![],
            chunks: vec![0, 0],
        };
        assert!(matches!(
            HostBuild::from_parts(identity(), vec![1; 10], vec![2], Some(wrong), None),
            Err(HostBuildError::Encoding(PieceKind::Core, _))
        ));
    }

    #[test]
    fn encoded_chunks_are_cut_from_the_stream_by_the_index() {
        let e = EncodedPiece {
            stream: vec![7, 7, 9, 9, 9],
            chunks: vec![2, 0, 3],
        };
        let b =
            HostBuild::from_parts(identity(), vec![0; 3 * 4096], vec![1], Some(e), None).unwrap();
        assert_eq!(b.core.encoded_chunk(0), Some(&[7, 7][..]));
        assert_eq!(b.core.encoded_chunk(1), None, "0 = send raw");
        assert_eq!(b.core.encoded_chunk(2), Some(&[9, 9, 9][..]));
        assert_eq!(b.engine.encoded_chunk(0), None);
    }
}
