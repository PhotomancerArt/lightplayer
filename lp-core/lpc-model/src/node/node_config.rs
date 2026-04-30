//! Per-instance authored use-site data for a node.
//!
//! The artifact ([design/03](../../docs/roadmaps/2026-04-28-node-runtime/design/03-artifact.md))
//! is the *class*; `NodeConfig` is the *instance customization*.
//! Two fields:
//!
//! - `artifact` — which on-disk artifact this instance uses.
//! - `overrides` — per-slot bindings that replace the artifact's
//!   defaults / declared `bind`.
//!
//! See [design/04-config.md](../../docs/roadmaps/2026-04-28-node-runtime/design/04-config.md).
//!
//! Distinct from the legacy `NodeConfig` *trait* in `lpl-model`
//! (which dispatches over kind). The trait retires in M5; the two
//! types coexist until then because they live in different crates.

use crate::ArtifactSpec;
use crate::prop::binding::Binding;
use crate::prop::prop_path::PropPath;
use alloc::vec::Vec;

/// Per-instance authored use-site data for a node.
///
/// The artifact ([design/03](../../docs/roadmaps/2026-04-28-node-runtime/design/03-artifact.md))
/// is the *class*; `NodeConfig` is the *instance customization*.
/// Two fields:
///
/// - `artifact` — which on-disk artifact this instance uses.
/// - `overrides` — per-slot bindings that replace the artifact's
///   defaults / declared `bind`.
///
/// See [design/04-config.md](../../docs/roadmaps/2026-04-28-node-runtime/design/04-config.md).
///
/// Distinct from the legacy `NodeConfig` *trait* in `lpl-model`
/// (which dispatches over kind). The trait retires in M5; the two
/// types coexist until then because they live in different crates.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct NodeConfig {
    pub artifact: ArtifactSpec,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<(PropPath, Binding)>,
}

impl NodeConfig {
    /// New `NodeConfig` with no overrides.
    pub fn new(artifact: ArtifactSpec) -> Self {
        Self {
            artifact,
            overrides: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::ChannelName;
    use crate::prop::prop_path::parse_path;
    use alloc::string::String;

    #[test]
    fn node_config_round_trips_empty_overrides() {
        let config = NodeConfig::new(ArtifactSpec(String::from("./fluid.vis")));
        let json = serde_json::to_string(&config).unwrap();
        let back: NodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
        assert!(back.overrides.is_empty());
    }

    #[test]
    fn overrides_omitted_when_empty() {
        let config = NodeConfig::new(ArtifactSpec(String::from("./test.lp")));
        let json = serde_json::to_string(&config).unwrap();
        assert!(
            !json.contains("overrides"),
            "empty overrides should be skipped"
        );
    }

    #[test]
    fn node_config_round_trips_with_literal_override() {
        use crate::LpsValue;
        use crate::value_spec::ValueSpec;

        let mut config = NodeConfig::new(ArtifactSpec(String::from("./shader.lp")));
        let path = parse_path("params.scale").unwrap();
        let binding = Binding::Literal(ValueSpec::Literal(LpsValue::F32(6.0)));
        config.overrides.push((path, binding));

        let json = serde_json::to_string(&config).unwrap();
        let back: NodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_config_round_trips_with_bus_override() {
        let mut config = NodeConfig::new(ArtifactSpec(String::from("./output.lp")));
        let path = parse_path("inputs.level").unwrap();
        let binding = Binding::Bus(ChannelName(String::from("audio/in/0")));
        config.overrides.push((path, binding));

        let json = serde_json::to_string(&config).unwrap();
        let back: NodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn node_config_toml_round_trips() {
        use crate::LpsValue;
        use crate::value_spec::ValueSpec;

        let mut config = NodeConfig::new(ArtifactSpec(String::from("./pattern.lp")));
        let path = parse_path("params.speed").unwrap();
        let binding = Binding::Literal(ValueSpec::Literal(LpsValue::F32(1.5)));
        config.overrides.push((path, binding));

        let toml_str = toml::to_string(&config).unwrap();
        let back: NodeConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, back);
    }
}
