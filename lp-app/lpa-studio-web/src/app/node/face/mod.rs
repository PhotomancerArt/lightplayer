//! Kind-specific node card faces (permanent face + drawers grammar).
//!
//! A face is the permanent top of a node card: preview + panel controls
//! (+ chat on shader). It never leaves; the code/advanced drawers expand
//! beneath it and growth is downward-only from a stable top. Nodes without
//! a face keep today's generic section rendering ([`super::NodePane`]'s
//! fallback branch).
//!
//! Disclosure state (drawer open, agent collapse, mirrored composer
//! draft) is CORE-OWNED (`NodeCardUiState`, keyed by node address): the
//! components here render `UiNodeView::card_ui` and dispatch
//! [`node_ui_action`]s instead of holding `use_signal`s.

use lpa_studio_core::{NodeUiOp, ProjectEditorOp, ProjectEditorTarget, UiAction};

mod clock_face;
mod fixture_face;
mod node_card_drawers;
mod node_card_section;
mod node_face_body;
mod output_face;
mod phasor_trace;
mod playlist_face;
mod preview_spaces;
mod shader_face;
mod space_section;
mod tape_driver;
mod tape_transport;

pub use clock_face::ClockFace;
pub use fixture_face::FixtureFace;
pub use node_card_drawers::NodeCardDrawers;
pub use node_card_section::NodeCardSection;
pub use node_face_body::NodeFaceBody;
pub use output_face::OutputFace;
pub use playlist_face::PlaylistFace;
pub use shader_face::ShaderFace;
pub(crate) use space_section::{face_space_badge, space_badge_title};
pub use tape_transport::TapeTransport;
pub(crate) use tape_transport::{
    RATE_DETENTS, adjacent_detent, apply_detents, frac_rate, rate_frac,
};

/// Wrap a node-card UI mutation in its dispatchable action (targeted at
/// the project editor's node-tree surface; the op carries its own node
/// key).
pub(crate) fn node_ui_action(op: NodeUiOp) -> UiAction {
    UiAction::from_op(
        ProjectEditorTarget::node_tree().node_id(),
        ProjectEditorOp::NodeUi(op),
    )
}
