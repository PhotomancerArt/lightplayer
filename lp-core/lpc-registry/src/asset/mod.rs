//! Effective project asset materialization.

mod asset_content;
mod asset_read_error;
mod materialize_asset;

pub use asset_content::{AssetBytes, AssetText};
pub use asset_read_error::AssetReadError;
pub use materialize_asset::{materialize_asset, materialize_asset_text};
