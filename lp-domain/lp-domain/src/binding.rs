//! **Bindings** connect parameter slots to **runtime signals** on the implicit
//! bus (`docs/design/lightplayer/quantity.md` §8).
//!
//! There is no separate “bus object” in authored files: **channels exist** when
//! at least one binding references them; direction (read vs write) comes from
//! the slot’s **role** in its container (e.g. under `params` vs an output
//! declaration), not from the [`Binding`] enum (`quantity.md` §8 “Direction is
//! contextual”). The first writer/reader to a channel establishes its
//! [`Kind`](crate::kind::Kind); mismatches are compose-time errors (same
//! section).
//!
//! # JSON / TOML shape
//!
//! The bus model in `quantity.md` uses `bind = { bus = "…" }` in TOML. M2’s
//! serde shape for [`Binding::Bus`] may differ from the final on-disk sugar;
//! see the `// TODO(M3)` note in this file.

// TODO(M3): align serde JSON shape with quantity.md §8 on-disk form `bind = { bus = "…" }`
// (inline string per channel). Current externally-tagged form is
// `{"bus":{"channel":"…"}}` for M2.

use crate::types::ChannelName;

/// A **connection** from a slot to a bus channel. v0 has a single variant.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    /// Read or write (per container context) the named channel. Convention:
    /// names like `time`, `video/in/0`, `audio/in/0` — see
    /// `docs/design/lightplayer/quantity.md` §8 and §11 (channel naming).
    Bus { channel: ChannelName },
}

/// **Compose-time** lookup for “what [`Kind`](crate::kind::Kind) does this
/// channel carry?”, used to validate that a slot’s kind matches the bus. A real
/// implementation lands in M3+; M2 is only a trait shape
/// (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md`).
pub trait BindingResolver {
    /// The kind currently associated with `channel`, if any. `None` means the
    /// channel will be **declared** by this binding (first use), per
    /// `docs/design/lightplayer/quantity.md` §8 “Compose-time validation”.
    fn channel_kind(&self, channel: &ChannelName) -> Option<crate::kind::Kind>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn bus_binding_serde_round_trips() {
        let b = Binding::Bus {
            channel: ChannelName(String::from("audio/in/0")),
        };
        let json = serde_json::to_string(&b).unwrap();
        let back: Binding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, back);
    }

    #[test]
    fn bus_binding_serde_form_includes_channel_string() {
        let b = Binding::Bus {
            channel: ChannelName(String::from("audio/in/0")),
        };
        let json = serde_json::to_string(&b).unwrap();
        assert!(json.contains("audio/in/0"));
    }
}
