//! Dataflow direction for a slot field.

use serde::{Deserialize, Serialize};

/// How a slot field participates in the node dataflow graph.
///
/// Direction is semantic metadata: it changes how authored bindings and the
/// resolver interpret a slot. It is intentionally separate from
/// [`crate::SlotMeta`], which describes presentation hints for tools and UIs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SlotDirection {
    /// Local/default data owned by the slot root.
    #[default]
    Local,
    /// The slot may consume data through bindings, falling back to authored data.
    Consumed,
    /// The slot is produced by its owner at runtime.
    Produced,
}
