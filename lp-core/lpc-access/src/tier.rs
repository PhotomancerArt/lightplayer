//! What a link is allowed to do.

use serde::{Deserialize, Serialize};

/// An access tier. `Edit` implies `Play`: the derived ordering is
/// `Play < Edit`, so "has at least tier X" is `held >= X`.
///
/// - **Play** — the panel (knobs, brightness) plus reads: project reads,
///   project listings, and reading project files over fs.
/// - **Edit** — everything: loading projects, authoring, fs writes and
///   deletes, device commands.
///
/// No tier, on any link, reads an access file (`**/.lp/access.json`).
///
/// Serialized as `"play"` / `"edit"`, in the persisted access files and on
/// the wire alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    Play,
    Edit,
}

impl Tier {
    /// Whether holding `self` satisfies a request that `needs` that tier.
    #[must_use]
    pub fn satisfies(self, needs: Tier) -> bool {
        self >= needs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_implies_play() {
        assert!(Tier::Edit.satisfies(Tier::Play));
        assert!(Tier::Edit.satisfies(Tier::Edit));
        assert!(Tier::Play.satisfies(Tier::Play));
        assert!(!Tier::Play.satisfies(Tier::Edit));
    }

    #[test]
    fn serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Tier::Play).unwrap(), "\"play\"");
        assert_eq!(serde_json::to_string(&Tier::Edit).unwrap(), "\"edit\"");
        let back: Tier = serde_json::from_str("\"edit\"").unwrap();
        assert_eq!(back, Tier::Edit);
    }
}
