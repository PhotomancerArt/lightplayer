//! Kind-specific node card faces.
//!
//! A face is the permanent top of a node card (preview + panel controls
//! [+ chat on shader]); code/advanced/settings drawers expand beneath it and
//! growth is downward-only from a stable top. Faces are type-aware and
//! hand-built per node kind; only the front-panel metadata (which slots are
//! on the panel, widget kind, range/unit) is data-driven from slot shapes.
//!
//! Faces exist for shader, fixture, playlist, button, and output nodes,
//! derived from the finished section DTOs in `node_face_builder` (project
//! layer) so a panel control and its backing slot row can never disagree.
//! Every other kind gets `None` and renders today's generic sections — the
//! universal fallback, also always available inside the advanced drawer.
//!
//! The button and output faces are the runtime-command channel's faces:
//! their affordance is not an edit at all but a poke at the live runtime
//! (simulate a press; drive a diagnostic pattern), so they carry a node
//! ADDRESS and action constructors rather than slot addresses.

mod ui_button_face;
mod ui_fixture_face;
mod ui_fixture_power;
mod ui_node_face;
mod ui_output_face;
mod ui_panel_control;
mod ui_panel_widget;
mod ui_playlist_entry;
mod ui_playlist_face;
mod ui_shader_face;

pub use ui_button_face::UiButtonFace;
pub use ui_fixture_face::UiFixtureFace;
pub use ui_fixture_power::UiFixturePower;
pub use ui_node_face::UiNodeFace;
pub use ui_output_face::UiOutputFace;
pub use ui_panel_control::UiPanelControl;
pub use ui_panel_widget::UiPanelWidget;
pub use ui_playlist_entry::UiPlaylistEntry;
pub use ui_playlist_face::UiPlaylistFace;
pub use ui_shader_face::UiShaderFace;
