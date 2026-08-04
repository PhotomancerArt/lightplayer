use alloc::string::String;

/// Human-facing metadata for a slot shape.
///
/// Metadata describes how a slot should be presented to authors and tools. It
/// does not participate in value validation, resolver behavior, permissions, or
/// save/writeback policy.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SlotMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Display unit suffix rendered near the value (e.g. "Hz", "%").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

impl SlotMeta {
    /// Metadata with no presentation hints.
    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_meta_defaults_to_no_presentation_hints() {
        let meta = SlotMeta::default();
        assert_eq!(meta.label, None);
        assert_eq!(meta.description, None);
        assert_eq!(meta.unit, None);
    }

    #[test]
    fn slot_meta_presentation_bucket_is_additive_over_old_payloads() {
        let meta: SlotMeta = serde_json::from_str("{}").expect("empty meta decodes");
        assert_eq!(meta.unit, None);

        let default_json = serde_json::to_string(&SlotMeta::default()).expect("meta encodes");
        assert_eq!(default_json, "{}", "default meta serializes no keys");
    }
}
