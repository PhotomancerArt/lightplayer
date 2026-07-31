//! The kind-specific face variants a node card can render.

use crate::{UiButtonFace, UiFixtureFace, UiOutputFace, UiPlaylistFace, UiShaderFace};

/// Kind-specific permanent face for a node card.
///
/// `None` (every kind without a hand-built face) means the card renders
/// the generic fallback sections; a face replaces the generic top while
/// the advanced/settings drawer keeps the full slot view.
#[derive(Clone, Debug, PartialEq)]
pub enum UiNodeFace {
    /// Shader card: preview, knob row, agent chat, code drawer.
    Shader(UiShaderFace),
    /// Fixture card: lit preview plus the dominant brightness fader.
    Fixture(UiFixtureFace),
    /// Playlist card: entry strip; the active child's real card renders
    /// below via the existing [`crate::UiNodeChild`].
    Playlist(UiPlaylistFace),
    /// Button card: the simulate-press control (a runtime poke, not an
    /// edit).
    Button(UiButtonFace),
    /// Output card: the test-pattern toggle (a runtime poke, not an edit).
    Output(UiOutputFace),
}
