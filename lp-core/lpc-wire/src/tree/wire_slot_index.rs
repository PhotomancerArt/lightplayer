//! Index into a parent's slot list on the wire (`WireSlotIndex`).

/// Index into a parent's slot list.
#[derive(
    Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct WireSlotIndex(pub u32);
