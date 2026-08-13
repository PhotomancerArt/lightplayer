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
pub mod view;

pub use editor_core::camera::Camera;
pub use editor_core::doc_refusal::{DocOpen, DocRefusal};
pub use editor_core::editor_session::MapEditorSession;
pub use editor_core::map_selection::MapSelection;
pub use editor_core::map_tool::MapTool;
pub use editor_core::placement::Placement;
pub use editor_core::shape_path::ShapePath;
pub use editor_core::view_geometry::{
    ArrowInput, MapArrowOverlay, MapArrowSeg, neutral_lamp_rgb, wiring_arrows,
};
pub use view::canvas::{
    CanvasAnchor, CanvasDrag, EditorCanvas, FixtureBody, FixtureEvent, FixtureSprite,
    capture_pointer, object_color,
};
pub use view::context_layer::ContextFixture;
pub use view::map_editor::{EditorFileOps, EditorViewOptions, MapEditor, ReferenceOps};
pub use view::object_properties::{ObjectPropertiesPane, shape_kind_label};
pub use view::reference::ReferenceImage;
pub use view::wheel::{WheelGesture, wheel_gesture};
// The document type IS the component input type; re-exported so hosts that
// only embed the editor need no direct lpc-mapping dependency.
pub use lpc_mapping::Map2dDoc;
