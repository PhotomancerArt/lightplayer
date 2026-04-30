use alloc::string::String;
use core::fmt;

/// A **string payload** for referring to an on-disk *artifact* (pattern, effect, …) from
/// another file. v0 is intentionally opaque and file-resolution rules land in
/// M3+ (`docs/roadmaps/2026-04-22-lp-domain/overview.md` — “Artifact resolution model is intentionally minimal in v0”).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SrcArtifactSpec(pub String);

impl fmt::Display for SrcArtifactSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::{String, ToString};

    use super::SrcArtifactSpec;

    #[test]
    fn artifact_spec_display_round_trips() {
        assert_eq!(
            SrcArtifactSpec(String::from("./fluid.vis")).to_string(),
            "./fluid.vis",
        );
    }
}
