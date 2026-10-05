//! The update facts: the model's mirror of the board manifest
//! (`lpc_update::BoardManifest`), cut to what a face or a verb turns on.
//!
//! A board speaks it twice: as `M` on channel 3 (authoritative — the
//! board's own answer, sent on link-up and when asked) and, from update
//! protocol Part B, as the hello's `firmware` field (a convenience). The
//! adapter (`lpa-link`, the one place this crate's vocabulary meets the
//! wire's) decodes either with `lpc-update` and fills this mirror; this
//! crate depends on serde only, the same discipline as [`crate::wire`].
//!
//! The manifest's JSON rides along verbatim in
//! [`UpdateFacts::manifest_json`] for `lpa-studio-core`, which parses it for
//! the update decision. The fold never reads it.

use serde::{Deserialize, Serialize};

/// What a board said about its firmware, as the device model reads it.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct UpdateFacts {
    /// What the board is doing — the fact the core-only face is made of
    /// (a board waiting for its engine is a LightPlayer, never a blank chip).
    pub state: UpdateBoardState,
    /// The board's app version (`2026.10.05-3`) — what the core-only face
    /// names, since a core-only board sends no hello to name it.
    pub version: Option<String>,
    /// The opaque target name (`esp32c6-4mb`) — what decides whether this
    /// Studio holds a build for the board at all (Update offered or not).
    pub target: Option<String>,
    /// `<version>+<commit[..12]>` — what tells "this Studio's build" from
    /// another build of the same version (Update offered, or Reinstall).
    pub build_id: Option<String>,
    /// A transfer pending or running — the card's progress, and whether
    /// another link owns it ("another device is updating it").
    pub transfer: Option<UpdateTransferFacts>,
    /// The build hash of a build that failed its trial here — the board
    /// refuses it forever, so the card must stop offering it.
    pub refused_build: Option<u32>,
    /// The manifest's JSON, verbatim — what `lpa-studio-core` parses for the
    /// update decision. The fold never reads it.
    pub manifest_json: String,
}

impl UpdateFacts {
    /// Whether the board may be running only its core: anything but
    /// [`UpdateBoardState::Running`]. A core-only board sends no hello, so
    /// these facts with no hello in the window are the core-only verdict.
    pub fn is_core_only(&self) -> bool {
        self.state != UpdateBoardState::Running
    }
}

/// What the board is doing — the mirror of `lpc_update::BoardState`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum UpdateBoardState {
    /// The engine runs (the board also says hello).
    Running,
    /// Core-only, with no valid engine: it waits for its engine.
    NeedsEngine,
    /// Core-only, because the engine keeps crashing.
    EngineCrashing,
    /// A transfer is pending or running.
    Updating,
    /// A new core on trial, not yet confirmed.
    OnTrial,
    /// A state this build does not know.
    #[default]
    Unknown,
}

impl UpdateBoardState {
    /// The state in a few plain words, for the model's own labels and
    /// outcome lines (the card's words are the app layer's).
    pub fn describe(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::NeedsEngine => "waiting for its engine",
            Self::EngineCrashing => "its engine keeps crashing",
            Self::Updating => "updating",
            Self::OnTrial => "new firmware on trial",
            Self::Unknown => "state not known",
        }
    }
}

/// A transfer, as the board reports it — the mirror of
/// `lpc_update::TransferView`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UpdateTransferFacts {
    /// Which piece is moving — the card's stage ("updating the engine").
    pub kind: UpdatePieceKind,
    /// Bytes written and read back — the card's percent.
    pub done: u32,
    /// The piece's length — the card's percent.
    pub total: u32,
    /// Another link owns the transfer and is live — the card waits rather
    /// than offering an update the board would refuse.
    pub busy: bool,
    /// The build hash the transfer installs — tells "continue my update"
    /// from "another build is pending" off the board's word alone.
    pub build_hash: u32,
}

/// Which piece of a split image a transfer moves — the mirror of
/// `lpc_update::PieceKind`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UpdatePieceKind {
    Core,
    Engine,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_running_board_is_not_core_only() {
        let facts = |state| UpdateFacts {
            state,
            ..UpdateFacts::default()
        };
        assert!(!facts(UpdateBoardState::Running).is_core_only());
        for state in [
            UpdateBoardState::NeedsEngine,
            UpdateBoardState::EngineCrashing,
            UpdateBoardState::Updating,
            UpdateBoardState::OnTrial,
            UpdateBoardState::Unknown,
        ] {
            assert!(facts(state).is_core_only(), "{state:?}");
        }
    }
}
