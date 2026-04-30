//! Child kind discriminator: determines how a child was authored and its
//! lifetime rules.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` §ChildKind.

use crate::node::NodeName;

/// Discriminator on tree nodes determining how the child is realated to the
/// parent node
///
/// All three are fully realized `NodeEntry`s: addressable by `NodePath`,
/// bindable from anywhere, and walked identically by tick. The difference is
/// *lifecycle ownership*:
///
/// - `Input` and `Sidecar` are parent-owned; they live as long as the parent.
/// - `Inline` is binding-owned; when the binding is removed or changed, the
///   child is destroyed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChildKind {
    /// Structural input declared by the artifact, e.g. `[input] visual = "..."`.
    /// Lifetime: parent's lifetime.
    Input {
        /// Source slot index in the parent artifact's slot list.
        source: SlotIdx,
    },

    /// Programmer-side declared child, e.g. `[children.lfo]`.
    /// Lifetime: parent's lifetime.
    Sidecar {
        /// The declared name in the parent's TOML.
        name: NodeName,
    },

    /// Inline child created from a slot binding override, e.g.
    /// `[params.gradient.bind] visual = "..."`.
    /// Lifetime: the slot binding's lifetime.
    Inline {
        /// The prop path in the parent that holds this inline binding.
        source: crate::prop::PropPath,
    },
}

/// Index into a parent's slot list. Placeholder until the full `Slot`
/// indexing work lands (see artifact schema §05).
#[derive(
    Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct SlotIdx(pub u32);

#[cfg(test)]
mod tests {
    use super::{ChildKind, SlotIdx};
    use crate::node::NodeName;
    use crate::prop::parse_path;

    #[test]
    fn child_kind_input_round_trips() {
        let kind = ChildKind::Input { source: SlotIdx(3) };
        let json = serde_json::to_string(&kind).unwrap();
        let decoded: ChildKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, decoded);
    }

    #[test]
    fn child_kind_sidecar_round_trips() {
        let kind = ChildKind::Sidecar {
            name: NodeName::parse("my_lfo").unwrap(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let decoded: ChildKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, decoded);
    }

    #[test]
    fn child_kind_inline_round_trips() {
        let kind = ChildKind::Inline {
            source: parse_path("params.gradient.bind").unwrap(),
        };
        let json = serde_json::to_string(&kind).unwrap();
        let decoded: ChildKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, decoded);
    }
}
