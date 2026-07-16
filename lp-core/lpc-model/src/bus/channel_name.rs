use alloc::string::String;
use core::fmt;

/// A **bus channel** name: a convention-only string shaped
/// `purpose[.in|.out][/instance]` (naming norms decided 2026-07-08;
/// supersedes the retired `<kind>/<dir>/<index>` convention from the
/// archived quantity.md):
///
/// - **purpose**: lowercase dotted segments (`time`, `time.delta`,
///   `transport.next`). Dots group families for display/pickers; the full
///   string is the channel identity — no structural resolution.
/// - **`.in` / `.out`**: only on channels that cross the project boundary
///   (`visual.out` toward fixtures, `visual.in` from a camera). Interior
///   channels (`time`, `trigger`) carry no direction — every channel has
///   writers and readers internally, so direction only means something at
///   the boundary.
/// - **`/instance`**: optional parallel-channel suffix, name or number
///   (`visual.out/left`, `visual.out/2`). The unadorned name is the primary
///   instance.
/// - **Units are not encoded in canonical names** (`time`, not
///   `time.seconds`): unit truth lives in slot metadata and the well-known
///   channel registry, and the UX displays it. A unit segment stays legal to
///   mark a deviating channel (`time.millis`).
///
/// The type does not enforce the grammar; channels are created lazily by
/// reference, and the editing UX teaches the norms (picker + well-known
/// registry) rather than a validator gatekeeping them.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
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
