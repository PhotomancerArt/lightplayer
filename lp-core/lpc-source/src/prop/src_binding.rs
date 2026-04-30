//! **Bindings** connect parameter slots to **runtime signals** on the implicit
//! bus (`docs/design/lightplayer/quantity.md` §8).
//!
//! There is no separate "bus object" in authored files: **channels exist** when
//! at least one binding references them; direction (read vs write) comes from
//! the slot's **role** in its container (e.g. under `params` vs an output
//! declaration), not from the [`Binding`] enum (`quantity.md` §8 "Direction is
//! contextual"). The first writer/reader to a channel establishes its
//! [`Kind`](lpc_model::prop::kind::Kind); mismatches are compose-time errors (same
//! section).
//!
//! # On-disk shape
//!
//! Bindings serialize as the inline form `bind = { bus = "<channel>" }` in
//! TOML and `{"bus":"<channel>"}` in JSON. New variants land additively as
//! sibling keys (`bind = { constant = ... }`, etc.); the on-disk grammar
//! stays a flat key-mutex on the `bind` table.

use crate::prop::src_value_spec::SrcValueSpec;
use lpc_model::NodePropSpec;
use lpc_model::bus::ChannelName;

/// A **connection** from a slot to a data source.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SrcBinding {
    /// Read or write the named bus channel. Convention:
    /// names like `time`, `video/in/0`, `audio/in/0/level` — see
    /// `docs/design/lightplayer/quantity.md` §8 and §11 (channel naming).
    /// Wire form: `{ "bus": "audio/in/0/level" }`.
    Bus(ChannelName),

    /// Inline literal value or texture spec.
    /// Wire form: `{ "literal": { "kind": "literal", "value": 0.7 } }`.
    /// Authoring shorthand (`scale = 6.0`) is a TOML-loader concern (M4.3).
    Literal(SrcValueSpec),

    /// Read another node's output slot.
    /// Wire form: `{ "node": { "node": "/path", "prop": "outputs[0]" } }`.
    /// Per-variant rename so the wire key is `node`, not `node_prop`.
    #[serde(rename = "node")]
    NodeProp(NodePropSpec),
}

/// **Compose-time** lookup for "what [`Kind`](lpc_model::prop::kind::Kind) does this
/// channel carry?", used to validate that a slot's kind matches the bus. A
/// real implementation lands in M3+; this is only a trait shape
/// (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md`).
pub trait BindingResolver {
    /// The kind currently associated with `channel`, if any. `None` means the
    /// channel will be **declared** by this binding (first use), per
    /// `docs/design/lightplayer/quantity.md` §8 "Compose-time validation".
    fn channel_kind(&self, channel: &ChannelName) -> Option<lpc_model::prop::kind::Kind>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn bus_binding_serde_round_trips() {
        let b = SrcBinding::Bus(ChannelName(String::from("audio/in/0")));
        let json = serde_json::to_string(&b).unwrap();
        let back: SrcBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn bus_binding_json_form_is_flat_string_under_bus_key() {
        let b = SrcBinding::Bus(ChannelName(String::from("audio/in/0/level")));
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, r#"{"bus":"audio/in/0/level"}"#);
    }

    #[test]
    fn bus_binding_deserializes_from_inline_string_form() {
        let b: SrcBinding = serde_json::from_str(r#"{"bus":"video/in/0"}"#).unwrap();
        match b {
            SrcBinding::Bus(ChannelName(s)) => assert_eq!(s, "video/in/0"),
            _ => panic!("expected Bus variant"),
        }
    }

    #[test]
    fn literal_binding_serde_round_trips() {
        use lpc_model::WireValue;
        let b = SrcBinding::Literal(SrcValueSpec::Literal(WireValue::F32(0.7)));
        let json = serde_json::to_string(&b).unwrap();
        let back: SrcBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn literal_binding_json_form_is_nested() {
        use lpc_model::WireValue;
        let b = SrcBinding::Literal(SrcValueSpec::Literal(WireValue::F32(0.7)));
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(
            json,
            r#"{"literal":{"kind":"literal","value":{"f32":0.7}}}"#
        );
    }

    #[test]
    fn literal_binding_toml_round_trips() {
        use lpc_model::WireValue;
        let b = SrcBinding::Literal(SrcValueSpec::Literal(WireValue::F32(1.5)));
        let toml_str = toml::to_string(&b).unwrap();
        let back: SrcBinding = toml::from_str(&toml_str).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn node_prop_binding_serde_round_trips() {
        let spec = NodePropSpec::parse("/main.show/fluid.vis#speed").unwrap();
        let b = SrcBinding::NodeProp(spec);
        let json = serde_json::to_string(&b).unwrap();
        let back: SrcBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn node_prop_binding_uses_node_key_on_wire() {
        // NodePropSpec requires name.type format
        let spec = NodePropSpec::parse("/main.show#outputs[0]").unwrap();
        let b = SrcBinding::NodeProp(spec);
        let json = serde_json::to_string(&b).unwrap();
        // Verify the variant serializes with "node" key (not "node_prop")
        assert!(
            json.contains(r#""node":"#),
            "wire key should be 'node', not 'node_prop'"
        );
        // TreePath serializes as array of segments, not a string
        assert!(json.contains(r#""node":"#));
        assert!(json.contains(r#""prop":"#));
    }

    #[test]
    fn node_prop_binding_toml_round_trips() {
        let spec = NodePropSpec::parse("/x.y#a.b[0]").unwrap();
        let b = SrcBinding::NodeProp(spec);
        let toml_str = toml::to_string(&b).unwrap();
        let back: SrcBinding = toml::from_str(&toml_str).unwrap();
        assert_eq!(b, back);
    }
}
