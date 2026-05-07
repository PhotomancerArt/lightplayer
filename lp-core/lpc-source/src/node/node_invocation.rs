//! Parent-owned instruction to instantiate a child node.
//!
//! The parent says "instantiate the node definition located at this
//! [`ArtifactLocator`] here". Inline node definitions and artifact-plus-local
//! field merges are reserved for richer invocation forms.

use crate::artifact::artifact_loc::ArtifactLocator;
use crate::prop::src_binding::SrcBinding;
use alloc::string::ToString;
use alloc::vec::Vec;
use lpc_model::{ArtifactPathSlot, value::value_path::ValuePath};

/// Parent-owned child node invocation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, lpc_model::SlotRecord)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeInvocation {
    /// Artifact to load for this child node definition.
    pub artifact: ArtifactPathSlot,

    /// Use-site binding overrides.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[slot(skip)]
    pub overrides: Vec<(ValuePath, SrcBinding)>,
}

impl NodeInvocation {
    /// New artifact-only invocation with no overrides.
    pub fn new(artifact: ArtifactLocator) -> Self {
        Self {
            artifact: ArtifactPathSlot::new(artifact.to_string()),
            overrides: Vec::new(),
        }
    }

    pub fn artifact_locator(&self) -> Result<ArtifactLocator, &'static str> {
        ArtifactLocator::parse(self.artifact.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prop::src_value_spec::SrcValueSpec;
    use alloc::string::String;
    use lpc_model::LpValue;
    use lpc_model::bus::ChannelName;
    use lpc_model::value::value_path::parse_path;

    #[test]
    fn node_invocation_round_trips_empty_overrides() {
        let config = NodeInvocation::new(ArtifactLocator::path("./fluid.vis"));
        let json = serde_json::to_string(&config).unwrap();
        let back: NodeInvocation = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
        assert!(back.overrides.is_empty());
    }

    #[test]
    fn overrides_omitted_when_empty() {
        let config = NodeInvocation::new(ArtifactLocator::path("./test.lp"));
        let json = serde_json::to_string(&config).unwrap();
        assert!(
            !json.contains("overrides"),
            "empty overrides should be skipped"
        );
    }

    #[test]
    fn node_invocation_round_trips_with_literal_override() {
        let mut config = NodeInvocation::new(ArtifactLocator::path("./shader.lp"));
        let path = parse_path("params.scale").unwrap();
        let binding = SrcBinding::Literal(SrcValueSpec::Literal(LpValue::F32(6.0)));
        config.overrides.push((path, binding));

        let json = serde_json::to_string(&config).unwrap();
        let back: NodeInvocation = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_invocation_round_trips_with_bus_override() {
        let mut config = NodeInvocation::new(ArtifactLocator::path("./output.lp"));
        let path = parse_path("inputs.level").unwrap();
        let binding = SrcBinding::Bus(ChannelName(String::from("audio/in/0")));
        config.overrides.push((path, binding));

        let json = serde_json::to_string(&config).unwrap();
        let back: NodeInvocation = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_invocation_toml_round_trips() {
        let mut config = NodeInvocation::new(ArtifactLocator::path("./pattern.lp"));
        let path = parse_path("params.speed").unwrap();
        let binding = SrcBinding::Literal(SrcValueSpec::Literal(LpValue::F32(1.5)));
        config.overrides.push((path, binding));

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
        assert!(invocation.overrides.is_empty());
    }
}
