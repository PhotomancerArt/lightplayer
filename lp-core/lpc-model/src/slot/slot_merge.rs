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
    /// Accept EVERY provider on the channel as an ordered fragment set.
    ///
    /// The receiver gets all candidates, in the resolver's deterministic
    /// provider order, and decides what each one covers — it does not pick a
    /// winner and does not combine values. The output node's control input is
    /// the case this exists for: N fixtures render into disjoint sub-slices of
    /// one wire's sample buffer, so "two producers" is a composition, not the
    /// ambiguity [`Self::Error`] and [`Self::Latest`] treat it as.
    ///
    /// Unlike [`Self::ByKey`], nothing is keyed and nothing is replaced:
    /// order is the whole meaning, and the receiver is responsible for
    /// reporting overlaps in whatever coordinate system it owns.
    Fragments,
}
