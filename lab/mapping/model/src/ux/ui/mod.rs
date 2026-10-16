//! The UI data tree: what core emits for the page to draw (architecture §3).
//! Semantic and small, with Studio's names where they fit, so moving it
//! into Studio is a transplant. It carries **no positions**: the view lays
//! the nodes out. Dense data (lamps, pixel colours) travels beside it,
//! read from the editor by id.

mod ui_canvas_edge;
mod ui_canvas_node;
mod ui_canvas_view;
mod ui_inspector;
mod ui_menu;
mod ui_node_face;
mod ui_node_header;
mod ui_overlay;
mod ui_toolbar;
mod ui_tree;

pub use ui_canvas_edge::UiCanvasEdge;
pub use ui_canvas_node::{DetailLevel, Emphasis, FrameKind, UiCanvasNode};
pub use ui_canvas_view::{LayoutKind, SizeClass, UiCanvasView, UiCursor, UiCursorNote};
pub use ui_inspector::{UiCrumb, UiInspector, UiItem, UiSection};
pub use ui_menu::{UiMenu, UiMenuAction, UiMenuItem};
pub use ui_node_face::{
    UiFixtureFace, UiNodeFace, UiObject, UiOutputFace, UiPlainFace, UiPortRow, UiRun, UiShape,
    UiSourceFace, UiWire,
};
pub use ui_node_header::UiNodeHeader;
pub use ui_overlay::{OutlineKind, UiDraftObject, UiHandle, UiOutline, UiOverlay};
pub use ui_toolbar::UiToolbar;
pub use ui_tree::{UiTree, UiTreeRow};
