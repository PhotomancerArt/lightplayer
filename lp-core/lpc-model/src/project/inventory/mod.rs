//! Effective project inventory.
//!
//! Inventory is the read-side model produced after project artifacts and the
//! current [`crate::ProjectOverlay`] have been combined. It contains:
//!
//! - unique referenced node definitions keyed by [`crate::NodeDefLocation`],
//! - unique referenced assets keyed by [`crate::AssetLocation`],
//! - an expanded [`crate::ProjectTree`] keyed by [`crate::NodeUseLocation`].
//!
//! The registry owns deriving inventory; these types only describe the portable
//! model shape.

pub mod asset_content_type;
pub mod asset_entry;
pub mod asset_location;
pub mod asset_state;
pub mod node_def_entry;
pub mod node_def_state;
pub mod project_inventory;
pub mod project_node;
pub mod project_node_placement;
pub mod project_tree;
pub mod referenced_asset;

pub use crate::project::overlay_mutation::asset_change_summary::{
    AssetChange, AssetChangeKind, AssetChangeSummary,
};
pub use crate::project::overlay_mutation::node_use_change_summary::{
    NodeUseChange, NodeUseChangeKind, NodeUseChangeSummary,
};
pub use asset_content_type::AssetContentType;
pub use asset_entry::AssetEntry;
pub use asset_location::AssetLocation;
pub use asset_state::{AssetBodyOrigin, AssetState};
pub use referenced_asset::ReferencedAsset;
