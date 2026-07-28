//! The kind-specific face variants a node card can render.

use crate::{UiEffectFace, UiFixtureFace, UiPlaylistFace, UiShaderFace};

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
    /// Effect card (embedded project): mirror preview + promoted-control
    /// knobs aliasing inner-child slots; children render as sibling cards.
    Effect(UiEffectFace),
}
