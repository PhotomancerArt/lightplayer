//! Effective project asset materialization.

mod materialize_asset;
mod materialize_asset_error;
mod materialized_asset;

pub use materialize_asset::{materialize_asset, materialize_asset_text};
pub use materialize_asset_error::MaterializeAssetError;
pub use materialized_asset::{MaterializedAsset, MaterializedTextAsset};
