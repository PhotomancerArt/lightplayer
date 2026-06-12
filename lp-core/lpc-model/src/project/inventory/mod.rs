//! Effective project inventory.
//!
//! Inventory is the read-side model produced after project artifacts and the
//! current [`crate::ProjectOverlay`] have been combined. It contains:
//!
//! - unique referenced node definitions keyed by [`crate::NodeDefLocation`],
//! - unique referenced assets keyed by [`crate::AssetSource`],
//! - an expanded [`crate::ProjectTree`] keyed by [`crate::NodeUseLocation`].
//!
//! The registry owns deriving inventory; these types only describe the portable
//! model shape.

pub mod asset_entry;
pub mod asset_kind;
pub mod asset_ref;
pub mod asset_source;
pub mod asset_state;
pub mod node_def_entry;
pub mod node_def_state;
pub mod project_inventory;
pub mod project_node;
pub mod project_node_placement;
pub mod project_tree;

pub use crate::project::overlay_mutation::asset_change_summary::{
    AssetChange, AssetChangeKind, AssetChangeSummary,
};
pub use crate::project::overlay_mutation::node_use_change_summary::{
    NodeUseChange, NodeUseChangeKind, NodeUseChangeSummary,
};
pub use asset_entry::AssetEntry;
pub use asset_kind::AssetKind;
pub use asset_ref::AssetRef;
pub use asset_source::AssetSource;
pub use asset_state::{AssetBodySource, AssetState};
