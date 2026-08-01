//! Board display metadata: the catalog/drawing layer for supported boards.
//!
//! Boundary: display data in, nothing out. This crate owns the
//! `boards/<vendor>/<product>.display.json` sidecar format — catalog identity
//! (tier, price, purchase links), and the drawing block the board diagram
//! renderer consumes. It knows nothing about projects, devices, routes, or the
//! runtime [`lpc-hardware`] manifest types; the two stay consistent through
//! the drift tests in `tests/manifest_drift.rs`, not through code coupling.
//!
//! The runtime board manifest (`boards/<vendor>/<product>.json`) is compiled
//! into firmware and stays flash-lean; everything presentation-only lives
//! here, app-side. See `docs/adr/2026-07-31-board-display-metadata-split.md`.

mod catalog;
#[cfg(feature = "diagram")]
mod diagram;
mod display_manifest;
pub mod geometry;

pub use catalog::{DISPLAY_MANIFEST_SOURCES, all_boards, board_by_id};
#[cfg(feature = "diagram")]
pub use diagram::{BoardDiagram, DiagramMargin};
pub use display_manifest::{
    BoardDisplayError, BoardDisplayFile, BoardDrawing, CapKind, DrawnButton, DrawnModule, DrawnPin,
    DrawnRgb, DrawnTerminal, DrawnUsb, PinCap, PinRole, PurchaseUrl, SupportTier,
};
pub use geometry::{DiagramMode, DiagramOptions, PinSwatch, WiredConnection};
