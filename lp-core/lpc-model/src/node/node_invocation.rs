//! Parent-owned instruction to instantiate a child node.
//!
//! The parent says "instantiate the node definition located at this
//! [`ArtifactLocator`] here". Inline node definitions and artifact-plus-local
//! field merges are reserved for richer invocation forms.

use crate::artifact::artifact_loc::ArtifactLocator;
use crate::{ArtifactPathSlot, SlotRecord};
use alloc::string::ToString;

/// Parent-owned child node invocation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, SlotRecord)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeInvocation {
    /// Artifact to load for this child node definition.
    pub artifact: ArtifactPathSlot,
}

impl NodeInvocation {
    /// New artifact-only invocation with no overrides.
    pub fn new(artifact: ArtifactLocator) -> Self {
        Self {
            artifact: ArtifactPathSlot::new(artifact.to_string()),
        }
    }

    pub fn artifact_locator(&self) -> Result<ArtifactLocator, &'static str> {
        ArtifactLocator::parse(self.artifact.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_invocation_round_trips() {
        let config = NodeInvocation::new(ArtifactLocator::path("./fluid.vis"));
        let json = serde_json::to_string(&config).unwrap();
        let back: NodeInvocation = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_invocation_json_has_artifact_only_shape() {
        let config = NodeInvocation::new(ArtifactLocator::path("./test.lp"));
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("artifact"));
        assert!(!json.contains("override"));
    }

    #[test]
    fn node_invocation_toml_round_trips() {
        let config = NodeInvocation::new(ArtifactLocator::path("./pattern.lp"));
        let toml_str = toml::to_string(&config).unwrap();
        let back: NodeInvocation = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_invocation_toml_table_form_loads() {
        let toml = r#"
            artifact = "./texture.toml"
        "#;
        let invocation: NodeInvocation = toml::from_str(toml).unwrap();
        assert_eq!(
            invocation.artifact_locator().unwrap(),
            ArtifactLocator::path("./texture.toml")
        );
    }
}
