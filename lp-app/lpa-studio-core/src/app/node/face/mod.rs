//! Kind-specific node card faces.
//!
//! A face is the permanent top of a node card (preview + panel controls
//! [+ chat on shader]); code/advanced/settings drawers expand beneath it and
//! growth is downward-only from a stable top. Faces are type-aware and
//! hand-built per node kind; only the front-panel metadata (which slots are
//! on the panel, widget kind, range/unit) is data-driven from slot shapes.
//!
//! Faces exist for shader, fixture, playlist, and output nodes, derived from
//! the finished section DTOs in `node_face_builder` (project layer) so a panel
//! control and its backing slot row can never disagree. Every other kind
//! gets `None` and renders today's generic sections — the universal
//! fallback, also always available inside the advanced drawer.
//!
//! The output face is the one face with facts its node's sections cannot
//! carry (board identity, incoming lamp extent); those arrive through a
//! studio-controller decoration pass, exactly like the shader face's agent
//! chat. See [`UiOutputFace`].

mod ui_clock_face;
mod ui_fixture_face;
mod ui_fixture_power;
mod ui_module_face;
mod ui_node_face;
mod ui_output_face;
mod ui_panel_control;
mod ui_panel_control_view;
mod ui_panel_group;
mod ui_panel_widget;
mod ui_playlist_entry;
mod ui_playlist_face;
mod ui_shader_face;
mod ui_space_section;

pub use ui_clock_face::{
    UiClockFace, UiClockTransport, UiPhasorReading, UiTimebaseState, phasor_rate_display,
};
pub use ui_fixture_face::UiFixtureFace;
pub use ui_fixture_power::UiFixturePower;
pub use ui_module_face::{UiExportsGroup, UiModuleExport, UiModuleFace};
pub use ui_node_face::UiNodeFace;
pub use ui_output_face::{
    UiLedBudget, UiOutputBoardFacts, UiOutputChannelRow, UiOutputFace, UiOutputPin, UiWireStatus,
};
pub use ui_panel_control::{
    UiPanelControl, UiPanelEmit, UiPanelTarget, UiPanelWire, UiPanelWireRole,
};
pub use ui_panel_control_view::{UiPanelControlState, UiPanelControlView};
pub use ui_panel_group::UiPanelGroup;
pub use ui_panel_widget::UiPanelWidget;
pub use ui_playlist_entry::UiPlaylistEntry;
pub use ui_playlist_face::UiPlaylistFace;
pub use ui_shader_face::UiShaderFace;
pub use ui_space_section::{
    UiSpaceCell, UiSpaceCellRole, UiSpaceChoice, UiSpaceFlag, UiSpaceFlagRole, UiSpaceMismatch,
    UiSpaceSection, UiSpaceSide,
};
