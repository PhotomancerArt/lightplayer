//! The kind-specific face variants a node card can render.

use crate::{UiFixtureFace, UiModuleFace, UiPanelGroup, UiPlaylistFace, UiShaderFace};

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
    /// Module card (`docs/design/modules.md` §5): output-mirror hero, the
    /// scope's panel, and the bus-wiring drawer. Worn at every depth — root
    /// workspace card and embedded child card alike. Its children render
    /// below it as sibling cards via [`crate::UiNodeChild`], like every
    /// other kind.
    Module(UiModuleFace),
    /// A non-module node whose whole face is its bound slots, presented as
    /// panel controls (`docs/design/modules.md` R3): the clock, the control
    /// node, the fixture sitting under a module. The group carries the
    /// ENCLOSING scope, because a leaf introduces no scope of its own — so
    /// these controls share their `(scope, channel)` identity, and their
    /// panel state, with the module panel above (`panel.md` P1).
    Controls(UiPanelGroup),
}
