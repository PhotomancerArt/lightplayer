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
//!
//! This crate also owns the **computed board↔firmware join**
//! ([`compatible_builds`]): which checked-in firmware build a board can run,
//! derived from chip identity and flash size — never hand-listed. Consumers
//! are the boards catalog and (when it lands) the provisioning picker. See
//! `firmware_join.rs` and
//! `docs/adr/2026-08-01-firmware-manifest-architecture.md`.

mod catalog;
#[cfg(feature = "diagram")]
mod catalog_page;
#[cfg(feature = "diagram")]
mod diagram;
mod display_manifest;
mod firmware_join;
pub mod geometry;
pub mod usb_bridge;

pub use catalog::{DISPLAY_MANIFEST_SOURCES, all_boards, board_by_id};
#[cfg(feature = "diagram")]
pub use catalog_page::BoardsCatalogPage;
#[cfg(feature = "diagram")]
pub use diagram::{BoardDiagram, DiagramMargin};
pub use display_manifest::{
    BoardDisplayError, BoardDisplayFile, BoardDrawing, BoardNote, CapKind, DrawnButton,
    DrawnModule, DrawnPin, DrawnRgb, DrawnTerminal, DrawnUsb, FirmwarePin, NoteOs, PadStyle,
    PinCap, PinRole, PurchaseUrl, SupportTier,
};
pub use firmware_join::{
    BUILD_DEF_SOURCES, BuildChip, BuildFeatureSummary, CompatibilityBasis, CompatibleBuild,
    FirmwareBuild, NoBuildReason, all_builds, build_by_id, build_features, compatible_builds,
    compatible_builds_for, feature_summary, no_build_reason, no_build_reason_for, node_kind_label,
};
pub use geometry::{DiagramMode, DiagramOptions, PinSwatch, WiredConnection};
pub use usb_bridge::{DriverGuidance, DriverNeedLevel, HostOs, UsbBridge};
