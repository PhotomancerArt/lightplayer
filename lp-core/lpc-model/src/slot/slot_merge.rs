//! Receiver-owned policy for combining multiple bound slot inputs.

use serde::{Deserialize, Serialize};

/// How a consumed slot combines multiple candidate binding inputs.
///
/// The merge policy belongs to the receiver because it describes the semantics
/// of the consumed slot, not the intent of any individual producer.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotMerge {
    /// Multiple inputs are a configuration error.
    Error,
    /// Use the selected/latest input and ignore other candidates.
    #[default]
    Latest,
    /// Merge stable-key maps by key.
    ByKey,
}
