//! **M2 UX spike** — the module face, the panel, and play mode.
//!
//! Prototype surfaces for the module container model
//! (`docs/design/modules.md`, `docs/design/panel.md`), built for the G2
//! visual gate. Everything here is fed by mock DTOs: the engine has no
//! scopes, no panel writers, and no `PanelWrite`/`PanelClear` ops yet — M4
//! owns that. What the spike is meant to answer is whether the *shapes*
//! work on screen:
//!
//! 1. Does one face hold up at all three zoom levels — effect author
//!    (children expanded), artist (a card in the workspace), end user
//!    (play mode)?
//! 2. Is bus-as-controls-on-the-face + bus-as-wiring-in-a-drawer the right
//!    split, with the sidebar bus pane gone?
//! 3. Does the root module back in the node area read better or worse?
//! 4. Is play mode as "root face only" the right direction?
//! 5. Where do the reset and auto-save gestures belong?
//!
//! The components are deliberately parallel to the node-face family
//! (`app/node/face/`) rather than merged into it: this is a spike, and the
//! seam should be easy to either promote or delete. The widgets themselves
//! (knob v2, fader, toggle) are the production ones, extended with one
//! `engaged` prop.

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

pub use module_face::ModuleFace;
pub use module_panel::{ModulePanel, NestedPanelGroup};
pub use module_panel_control::ModulePanelControl;
pub use panel_gesture::PanelGesture;
pub use play_mode::PlayModeSurface;
