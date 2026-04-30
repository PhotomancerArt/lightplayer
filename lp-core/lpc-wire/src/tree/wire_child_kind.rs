//! Child kind discriminator on the wire (`WireChildKind`).
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md`.

use lpc_model::PropPath;
use lpc_model::node::NodeName;

use super::WireSlotIndex;

/// How a child relates to its parent for lifecycle purposes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireChildKind {
    /// Structural input from the artifact (`[input]`).
    Input {
        /// Index into the parent's slot list.
        source: WireSlotIndex,
    },
    /// Programmer-declared sidecar (`[children.*]`).
    Sidecar {
        /// Declared name in parent TOML.
        name: NodeName,
    },
    /// Inline child from a binding override.
    Inline {
        /// Prop path holding the inline binding.
        source: PropPath,
    },
}

#[cfg(test)]
mod tests {
    use super::{WireChildKind, WireSlotIndex};
    use lpc_model::node::NodeName;
    use lpc_model::prop::parse_path;

    #[test]
    fn child_kind_input_round_trips() {
        let kind = WireChildKind::Input {
            source: WireSlotIndex(3),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let decoded: WireChildKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, decoded);
    }

    #[test]
    fn child_kind_sidecar_round_trips() {
        let kind = WireChildKind::Sidecar {
            name: NodeName::parse("my_lfo").unwrap(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let decoded: WireChildKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, decoded);
    }

    #[test]
    fn child_kind_inline_round_trips() {
        let kind = WireChildKind::Inline {
            source: parse_path("params.gradient.bind").unwrap(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let decoded: WireChildKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, decoded);
    }
}
