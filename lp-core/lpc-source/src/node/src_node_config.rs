//! Per-instance authored use-site data for a node.
//!
//! The artifact is the *class*; [`SrcNodeConfig`] is the *instance customization*.

use crate::artifact::src_artifact_spec::SrcArtifactSpec;
use crate::prop::src_binding::SrcBinding;
use alloc::vec::Vec;
use lpc_model::prop::prop_path::PropPath;

/// Per-instance authored use-site data for a node.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct SrcNodeConfig {
    pub artifact: SrcArtifactSpec,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<(PropPath, SrcBinding)>,
}

impl SrcNodeConfig {
    /// New config with no overrides.
    pub fn new(artifact: SrcArtifactSpec) -> Self {
        Self {
            artifact,
            overrides: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prop::src_value_spec::SrcValueSpec;
    use alloc::string::String;
    use lpc_model::ModelValue;
    use lpc_model::bus::ChannelName;
    use lpc_model::prop::prop_path::parse_path;

    #[test]
    fn node_config_round_trips_empty_overrides() {
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./fluid.vis")));
        let json = serde_json::to_string(&config).unwrap();
        let back: SrcNodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
        assert!(back.overrides.is_empty());
    }

    #[test]
    fn overrides_omitted_when_empty() {
        let config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./test.lp")));
        let json = serde_json::to_string(&config).unwrap();
        assert!(
            !json.contains("overrides"),
            "empty overrides should be skipped"
        );
    }

    #[test]
    fn node_config_round_trips_with_literal_override() {
        let mut config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./shader.lp")));
        let path = parse_path("params.scale").unwrap();
        let binding = SrcBinding::Literal(SrcValueSpec::Literal(ModelValue::F32(6.0)));
        config.overrides.push((path, binding));

        let json = serde_json::to_string(&config).unwrap();
        let back: SrcNodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_config_round_trips_with_bus_override() {
        let mut config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./output.lp")));
        let path = parse_path("inputs.level").unwrap();
        let binding = SrcBinding::Bus(ChannelName(String::from("audio/in/0")));
        config.overrides.push((path, binding));

        let json = serde_json::to_string(&config).unwrap();
        let back: SrcNodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_config_toml_round_trips() {
        let mut config = SrcNodeConfig::new(SrcArtifactSpec(String::from("./pattern.lp")));
        let path = parse_path("params.speed").unwrap();
        let binding = SrcBinding::Literal(SrcValueSpec::Literal(ModelValue::F32(1.5)));
        config.overrides.push((path, binding));

        let toml_str = toml::to_string(&config).unwrap();
        let back: SrcNodeConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, back);
    }
}
