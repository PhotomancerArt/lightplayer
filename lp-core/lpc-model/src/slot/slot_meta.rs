use alloc::string::String;

/// Human-facing metadata for a slot shape.
///
/// Metadata describes how a slot should be presented to authors and tools. It
/// does not participate in value validation.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SlotMeta {
    /// Metadata with no presentation hints.
    pub fn empty() -> Self {
        Self::default()
    }
}
