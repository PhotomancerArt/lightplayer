//! The board manifest **without a heap**: [`BoardManifestView`] holds the
//! same facts as a [`BoardManifest`], with its text borrowed (`target`,
//! `chip`, `version`) or kept as bytes (`buildId` as the zero-padded field,
//! the two digests as 32 raw bytes) rather than owned as `String`s.
//!
//! A board that keeps its manifest resident keeps this: its size no longer
//! depends on how long the app version is, and the owned [`BoardManifest`]
//! is built ([`BoardManifestView::to_manifest`]) only when one is sent, and
//! freed after. The serialized manifest is the same bytes either way: the
//! view has no serde of its own.

use alloc::string::String;

use crate::board_manifest::{BoardManifest, BoardState, TransferView};
use crate::build_id::{BUILD_ID_LEN, build_id_text};
use crate::sha256_hex::sha256_to_hex;

/// A [`BoardManifest`] that owns no heap. Field for field the same, in the
/// same order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardManifestView<'a> {
    pub proto: u8,
    pub target: &'a str,
    pub chip: &'a str,
    pub version: &'a str,
    /// The build id's zero-padded field ([`crate::build_id`]).
    pub build_id: [u8; BUILD_ID_LEN],
    pub wire_proto: u32,
    /// SHA-256 of the running core, raw.
    pub core_sha256: [u8; 32],
    pub core_len: u32,
    /// The core's digest slot, raw.
    pub engine_sha256: [u8; 32],
    pub engine_len: Option<u32>,
    pub layout: u16,
    pub loader: u16,
    pub region_len: u32,
    pub state: BoardState,
    pub refused_build: Option<u32>,
    pub transfer: Option<TransferView>,
}

impl<'a> BoardManifestView<'a> {
    /// The same view with its borrowed text replaced: how a resident copy
    /// taken from a short-lived session points at the image's own
    /// `'static` strings instead.
    #[must_use]
    pub fn with_text<'b>(
        &self,
        target: &'b str,
        chip: &'b str,
        version: &'b str,
    ) -> BoardManifestView<'b> {
        BoardManifestView {
            target,
            chip,
            version,
            proto: self.proto,
            build_id: self.build_id,
            wire_proto: self.wire_proto,
            core_sha256: self.core_sha256,
            core_len: self.core_len,
            engine_sha256: self.engine_sha256,
            engine_len: self.engine_len,
            layout: self.layout,
            loader: self.loader,
            region_len: self.region_len,
            state: self.state,
            refused_build: self.refused_build,
            transfer: self.transfer,
        }
    }

    /// The owned manifest, the one that is serialized and sent.
    #[must_use]
    pub fn to_manifest(&self) -> BoardManifest {
        BoardManifest {
            proto: self.proto,
            target: self.target.into(),
            chip: self.chip.into(),
            version: self.version.into(),
            build_id: String::from_utf8_lossy(build_id_text(&self.build_id)).into_owned(),
            wire_proto: self.wire_proto,
            core_sha256: sha256_to_hex(&self.core_sha256),
            core_len: self.core_len,
            engine_sha256: sha256_to_hex(&self.engine_sha256),
            engine_len: self.engine_len,
            layout: self.layout,
            loader: self.loader,
            region_len: self.region_len,
            state: self.state,
            refused_build: self.refused_build,
            transfer: self.transfer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_id::build_id_field;

    #[test]
    fn a_view_serializes_to_the_same_bytes_as_its_manifest() {
        let view = BoardManifestView {
            proto: 1,
            target: "esp32c6-4mb",
            chip: "esp32c6",
            version: "2026.10.05-3",
            build_id: build_id_field(b"2026.10.05-3+abc123456789").unwrap(),
            wire_proto: 36,
            core_sha256: [0x11; 32],
            core_len: 1_160_000,
            engine_sha256: [0x22; 32],
            engine_len: Some(1_830_000),
            layout: 1,
            loader: 1,
            region_len: 3_375_104,
            state: BoardState::Running,
            refused_build: None,
            transfer: None,
        };
        let manifest = view.to_manifest();
        assert_eq!(manifest.build_id, "2026.10.05-3+abc123456789");
        assert_eq!(manifest.core_sha256, "11".repeat(32));
        assert_eq!(manifest.engine_sha256, "22".repeat(32));
        let again = view.with_text("esp32c6-4mb", "esp32c6", "2026.10.05-3");
        assert_eq!(again.to_manifest().to_json(), manifest.to_json());
    }
}
