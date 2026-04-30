use alloc::string::String;
use core::fmt;

/// A **bus channel** name: convention-only string with shape like
/// `<sort>/<in|out>/<id>/…` (e.g. `time`, `video/in/0`, `audio/in/0`), as in
/// `docs/design/lightplayer/quantity.md` §8 and §11 (channel naming). The type
/// does not enforce the grammar in v0; compose-time code validates against the
/// project’s bus graph.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct ChannelName(pub String);

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::{String, ToString};

    use super::ChannelName;

    #[test]
    fn channel_name_display_round_trips() {
        assert_eq!(
            ChannelName(String::from("audio/in/0")).to_string(),
            "audio/in/0",
        );
    }
}
