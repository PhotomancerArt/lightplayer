//! A board, as the host sees it: its manifest (`M`, authoritative) and the
//! facts the decision needs, derived.
//!
//! [`BoardView::absent`] is a board that never answered `Q`, or whose hello
//! carries no `firmware` block: monolithic and pre-update firmware (E9,
//! DM25). It cannot be updated over a link.

use lpc_update::code_table::{chip_code, layout_known};
use lpc_update::{BoardManifest, BoardState, PieceKind, sha256_from_hex};

/// A transfer the board reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Updating {
    pub kind: PieceKind,
    pub done: u32,
    pub total: u32,
    /// Another link owns it and is live (E6).
    pub busy: bool,
    /// The build hash of the build it installs.
    pub build_hash: u32,
}

/// The host's view of one board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardView {
    /// `None`: no manifest (see [`BoardView::absent`]).
    pub manifest: Option<BoardManifest>,
}

impl BoardView {
    /// A board that said nothing on channel 3.
    #[must_use]
    pub fn absent() -> Self {
        Self { manifest: None }
    }

    /// From an `M`'s JSON; `None` if it is not a manifest.
    #[must_use]
    pub fn from_json(json: &[u8]) -> Option<Self> {
        BoardManifest::from_json(json).ok().map(Self::from_manifest)
    }

    #[must_use]
    pub fn from_manifest(manifest: BoardManifest) -> Self {
        Self {
            manifest: Some(manifest),
        }
    }

    /// From a hello's `firmware` block (wire proto 37): the same manifest
    /// as `M`, or [`Self::absent`] when the hello has none (a single image,
    /// E9). A convenience: channel 3's `M` stays authoritative (DM9), and a
    /// driver always asks `Q` on its link.
    #[must_use]
    pub fn from_hello_firmware(firmware: Option<BoardManifest>) -> Self {
        firmware.map_or_else(Self::absent, Self::from_manifest)
    }

    /// Whether this board can take an update over a link: a split layout
    /// this host knows and a chip in the code table.
    #[must_use]
    pub fn can_update_over_link(&self) -> bool {
        self.manifest.as_ref().is_some_and(|m| {
            m.layout >= 1 && layout_known(m.layout) && chip_code(&m.chip).is_some()
        })
    }

    /// The core's SHA-256, if reported and well-formed.
    #[must_use]
    pub fn core_sha256(&self) -> Option<[u8; 32]> {
        sha256_from_hex(&self.manifest.as_ref()?.core_sha256)
    }

    /// The engine this core needs (its digest slot).
    #[must_use]
    pub fn engine_sha256(&self) -> Option<[u8; 32]> {
        sha256_from_hex(&self.manifest.as_ref()?.engine_sha256)
    }

    /// The engine's length, while its header is valid.
    #[must_use]
    pub fn engine_len(&self) -> Option<u32> {
        self.manifest.as_ref()?.engine_len
    }

    #[must_use]
    pub fn state(&self) -> Option<BoardState> {
        self.manifest.as_ref().map(|m| m.state)
    }

    /// Core-only, waiting for its engine (E1, E13).
    #[must_use]
    pub fn needs_engine(&self) -> bool {
        self.state() == Some(BoardState::NeedsEngine)
    }

    /// Core-only because the engine keeps crashing (E10).
    #[must_use]
    pub fn crashing(&self) -> bool {
        self.state() == Some(BoardState::EngineCrashing)
    }

    /// The transfer the board reports, if any.
    #[must_use]
    pub fn updating(&self) -> Option<Updating> {
        let t = self.manifest.as_ref()?.transfer?;
        Some(Updating {
            kind: t.kind,
            done: t.done,
            total: t.total,
            busy: t.busy,
            build_hash: t.build_hash,
        })
    }

    /// The build hash of the build that failed its trial here (E3).
    #[must_use]
    pub fn refused_build(&self) -> Option<u32> {
        self.manifest.as_ref()?.refused_build
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hello_without_firmware_is_a_board_that_said_nothing() {
        assert_eq!(BoardView::from_hello_firmware(None), BoardView::absent());
        assert!(!BoardView::from_hello_firmware(None).can_update_over_link());
    }
}
