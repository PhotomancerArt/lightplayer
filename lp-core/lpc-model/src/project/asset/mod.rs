//! Project asset identities and effective asset state.
//!
//! Assets are non-node-definition project resources such as GLSL source files,
//! fixture SVG mappings, image files, text blobs, or future binary payloads.
//! A project asset may be backed by an artifact, inline inside a node
//! definition, or eventually by a URL.
//!
//! Related modules:
//!
//! - [`crate::project::inventory`] stores referenced assets in
//!   [`crate::ProjectInventory`].
//! - [`crate::project::overlay`] can replace or delete artifact-backed asset
//!   bodies.
//! - [`crate::nodes`] discovers assets from authored node definitions.

pub mod asset_entry;
pub mod asset_kind;
pub mod asset_source;
pub mod asset_state;
pub mod referenced_asset;

pub use crate::project::overlay_mutation::asset_change_set::{
    AssetChange, AssetChangeKind, AssetChangeSet,
};
pub use asset_entry::AssetEntry;
pub use asset_kind::AssetKind;
pub use asset_source::AssetSource;
pub use asset_state::{AssetBodySource, AssetState};
pub use referenced_asset::ReferencedAsset;
