//! The 2D mapping editor: the ONE project canvas and the editing grammar
//! over `lpc-mapping` documents.
//!
//! Boundary (the one-project-canvas ADR): **data in, events out.** This
//! crate owns the SURFACE — geometry, the project-space camera, the
//! gesture grammar, tools, per-document session/undo, the fixture sprite
//! layer, and the placement seam (a dived document renders through its
//! `Placement` inside project space; the session never learns it is
//! placed). It has **no** knowledge of projects, assets, routes, or the
//! Studio server: fixtures enter as plain [`FixtureSprite`] props, intent
//! leaves as [`FixtureEvent`]s, and the host owns every policy decision
//! (persistence, ops, journal, prefetch, packing).
//!
//! There is no wrapper editor component: hosts compose [`EditorCanvas`]
//! with the floats ([`ZoomFloat`], [`HelpFloat`]), the [`tool_hint`], and
//! the keyboard grammar ([`handle_editor_key`]) — the canvas IS the
//! editor.
//!
//! `editor_core` is pure Rust — sessions, tools, selection, camera,
//! placement, and the shared lamp-view geometry are host-testable with no
//! browser or Dioxus involvement. View components (Dioxus) stay thin over
//! the core.

pub mod editor_core;
pub mod view;

pub use editor_core::camera::Camera;
pub use editor_core::doc_fit::{display_inset_padding, doc_fit_bounds};
pub use editor_core::doc_refusal::{DocOpen, DocRefusal};
pub use editor_core::editor_session::MapEditorSession;
pub use editor_core::map_selection::MapSelection;
pub use editor_core::map_tool::MapTool;
pub use editor_core::placement::Placement;
pub use editor_core::shape_path::{ShapePath, structural_child, structural_child_count};
pub use editor_core::view_geometry::{
    ArrowInput, MapArrowOverlay, MapArrowSeg, neutral_lamp_rgb, wiring_arrows,
};
pub use view::canvas::{
    CanvasAnchor, CanvasDrag, EditorCanvas, FixtureBody, FixtureEvent, FixtureSprite,
    capture_pointer, object_color,
};
pub use view::floats::{HelpFloat, ZoomFloat, tool_hint};
pub use view::keys::{EditorKeyOutcome, handle_editor_key};
pub use view::object_properties::{ObjectPropertiesPane, shape_kind_label};
pub use view::reference::ReferenceImage;
pub use view::view_options::EditorViewOptions;
pub use view::wheel::{WheelGesture, wheel_gesture};
// The document type IS the component input type; re-exported so hosts that
// only embed the editor need no direct lpc-mapping dependency.
pub use lpc_mapping::Map2dDoc;
