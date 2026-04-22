//! Binding enum (bus connection) + BindingResolver trait stub.
//! See docs/design/lightplayer/quantity.md §8.

// TODO(M3): align serde JSON shape with quantity.md §8 on-disk form `bind = { bus = "…" }`
// (inline string per channel). Current externally-tagged form is
// `{"bus":{"channel":"…"}}` for M2.

use crate::types::ChannelName;

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

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Binding {
    Bus { channel: ChannelName },
}

/// Trait stub — compose-time validation that a Slot's binding is
/// type-compatible with its target bus channel. Real impl lands in M3+.
pub trait BindingResolver {
    /// The [`Kind`](crate::kind::Kind) that the channel currently carries (set by the first
    /// binding to it). `None` means the channel doesn't exist yet and
    /// will be declared by this binding.
    fn channel_kind(&self, channel: &ChannelName) -> Option<crate::kind::Kind>;
}
