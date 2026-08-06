//! The module face, the panel, and play mode.
//!
//! The rendering half of the module container model
//! (`docs/design/modules.md`, `docs/design/panel.md`). Everything here is
//! fed by real DTOs: `ProjectController::module_face` derives the face from
//! the engine's scoped channels, and the controls dispatch `PanelWriteOp` /
//! `PanelClearOp` down the runtime command channel. The story fixtures
//! mirror those shapes rather than standing in for them.
//!
//! One face at three zoom levels, all shipped: the effect author works
//! inside the module (children expanded as sibling cards), the artist sees
//! the module face as a card in the workspace, and the end user sees the
//! root module's face alone (play mode, `/sim|device/<key>/play`). The
//! sidebar bus pane is gone: bus-as-controls lives on the face, and
//! bus-as-writers/readers in the wiring drawer.
//!
//! The components stay parallel to the node-face family (`app/node/face/`)
//! rather than merged into it — a module card's surface is a panel, not a
//! kind-specific hero + sections. The widgets themselves (knob v2, fader,
//! toggle) are the production ones, extended with one `engaged` prop.

mod module_face;
mod module_panel;
mod module_panel_control;
mod panel_gesture;
mod play_mode;

#[cfg(feature = "stories")]
pub(crate) mod module_face_stories;
#[cfg(feature = "stories")]
pub(crate) mod module_fixtures;
#[cfg(feature = "stories")]
pub(crate) mod panel_state_stories;
#[cfg(feature = "stories")]
pub(crate) mod play_mode_stories;
#[cfg(feature = "stories")]
pub(crate) mod playlist_panel_stories;
#[cfg(feature = "stories")]
pub(crate) mod wiring_drawer_stories;

pub use module_face::ModuleFace;
pub use module_panel::{ModulePanel, NestedPanelGroup};
pub use module_panel_control::ModulePanelControl;
pub use panel_gesture::{PanelGesture, panel_gesture_actions};
pub use play_mode::PlayModeSurface;
