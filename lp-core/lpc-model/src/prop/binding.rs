//! **Bindings** connect parameter slots to **runtime signals** on the implicit
//! bus (`docs/design/lightplayer/quantity.md` §8).
//!
//! There is no separate "bus object" in authored files: **channels exist** when
//! at least one binding references them; direction (read vs write) comes from
//! the slot's **role** in its container (e.g. under `params` vs an output
//! declaration), not from the [`Binding`] enum (`quantity.md` §8 "Direction is
//! contextual"). The first writer/reader to a channel establishes its
//! [`Kind`](crate::prop::kind::Kind); mismatches are compose-time errors (same
//! section).
//!
//! # On-disk shape
//!
//! Bindings serialize as the inline form `bind = { bus = "<channel>" }` in
//! TOML and `{"bus":"<channel>"}` in JSON. New variants land additively as
//! sibling keys (`bind = { constant = ... }`, etc.); the on-disk grammar
//! stays a flat key-mutex on the `bind` table.

use crate::bus::ChannelName;

/// A **connection** from a slot to a bus channel. v0 has a single variant.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    /// Read or write (per container context) the named channel. Convention:
    /// names like `time`, `video/in/0`, `audio/in/0/level` — see
    /// `docs/design/lightplayer/quantity.md` §8 and §11 (channel naming).
    Bus(ChannelName),
}

/// **Compose-time** lookup for "what [`Kind`](crate::prop::kind::Kind) does this
/// channel carry?", used to validate that a slot's kind matches the bus. A
/// real implementation lands in M3+; this is only a trait shape
/// (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md`).
pub trait BindingResolver {
    /// The kind currently associated with `channel`, if any. `None` means the
    /// channel will be **declared** by this binding (first use), per
    /// `docs/design/lightplayer/quantity.md` §8 "Compose-time validation".
    fn channel_kind(&self, channel: &ChannelName) -> Option<crate::prop::kind::Kind>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn bus_binding_serde_round_trips() {
        let b = Binding::Bus(ChannelName(String::from("audio/in/0")));
        let json = serde_json::to_string(&b).unwrap();
        let back: Binding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn bus_binding_json_form_is_flat_string_under_bus_key() {
        let b = Binding::Bus(ChannelName(String::from("audio/in/0/level")));
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(json, r#"{"bus":"audio/in/0/level"}"#);
    }

    #[test]
    fn bus_binding_deserializes_from_inline_string_form() {
        let b: Binding = serde_json::from_str(r#"{"bus":"video/in/0"}"#).unwrap();
        match b {
            Binding::Bus(ChannelName(s)) => assert_eq!(s, "video/in/0"),
        }
    }
}
