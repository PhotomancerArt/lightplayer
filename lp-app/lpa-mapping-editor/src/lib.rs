//! The 2D mapping editor module: a standalone, project-agnostic editor for
//! `lpc-mapping` documents.
//!
//! Boundary (parent plan D5): document in, edits out. This crate knows the
//! mapping document schema and how to edit it — it has **no** knowledge of
//! projects, assets, routes, or the Studio server. Hosts (the `#/mapping`
//! page, later the fixture face) own persistence and mount the editor.
//!
//! `editor_core` is pure Rust — sessions, tools, selection, camera, and the
//! shared lamp-view geometry are host-testable with no browser or Dioxus
//! involvement. View components (Dioxus) arrive in later phases and stay
//! thin over the core.

pub mod editor_core;

pub use editor_core::camera::Camera;
pub use editor_core::editor_session::MapEditorSession;
pub use editor_core::map_selection::MapSelection;
pub use editor_core::map_tool::MapTool;
pub use editor_core::view_geometry::{
    ArrowInput, LAMPS_PER_UNIVERSE, MapArrowOverlay, MapArrowSeg, lamp_universe, neutral_lamp_rgb,
    universe_rgb, wiring_arrows,
};
