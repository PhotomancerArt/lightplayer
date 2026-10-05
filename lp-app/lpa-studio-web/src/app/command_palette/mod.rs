//! The ⌘K command palette: every offer the view publishes, filtered as you
//! type, pressed through the same dispatch as the buttons (roadmap M2).
//!
//! It is the human's second view of the offer tree, beside the app agent's:
//! a verb missing here is a verb the agent cannot see either. See
//! `docs/adr/2026-10-01-agentic-control-offers-in-core.md`.

pub mod command_palette;
pub mod command_palette_dialog;
pub mod command_palette_hint;
#[cfg(feature = "stories")]
pub(crate) mod command_palette_stories;
pub(crate) mod palette_cursor;

pub use command_palette::CommandPalette;
pub use command_palette_hint::CommandPaletteHint;
