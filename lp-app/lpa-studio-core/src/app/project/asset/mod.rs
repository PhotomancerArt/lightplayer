//! Asset body editing: whole-file pending edits alongside slot edits.
//!
//! Assets (shader GLSL, fixture SVGs) are edited as **whole bodies**: an
//! applied edit stages one `MutationOp::SetArtifactBody` carrying the full
//! byte body, mirrored as an `ArtifactOverlay::Asset` entry in the overlay.
//! The pipeline deliberately mirrors slot editing — buffer entry with the
//! same ack lifecycle, overlay-derived dirty, save-panel row with per-entry
//! revert (`MutationOp::ClearArtifact`) — with one deliberate exception:
//! **unapplied editor text is client-local**. Text typed into an editor that
//! has not been applied yet lives only in the editor component (Apply
//! enablement chrome); everything visible in the tree and save panel derives
//! strictly from the overlay mirror plus the un-acked buffer, exactly like
//! slot edits (editing-model ADR D1).

pub mod asset_content_fetch_op;
pub mod asset_edit_op;
pub mod pending_asset_edit;
pub mod ui_asset_content;
pub mod ui_shader_error;

pub use asset_content_fetch_op::AssetContentFetchOp;
pub use asset_edit_op::{AssetEditOp, MAX_ASSET_BODY_BYTES};
pub use pending_asset_edit::PendingAssetEdit;
pub use ui_asset_content::{UiAssetContent, UiAssetContentBody};
pub use ui_shader_error::UiShaderError;
