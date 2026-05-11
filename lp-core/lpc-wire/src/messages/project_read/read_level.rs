//! Shared level-of-detail selector for stateless project reads.

/// Amount of data requested for one read domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ReadLevel {
    /// Identifiers only.
    Ids,
    /// Metadata and skeleton information, but no heavy payloads.
    Summary,
    /// Full domain detail selected by the query.
    Detail,
}

impl Default for ReadLevel {
    fn default() -> Self {
        Self::Summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_level_round_trips_snake_case() {
        let json = serde_json::to_string(&ReadLevel::Detail).unwrap();
        assert_eq!(json, "\"detail\"");
        let back: ReadLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ReadLevel::Detail);
    }
}
