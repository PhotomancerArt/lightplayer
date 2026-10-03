//! Who nearby gets in with no password at all.

use serde::{Deserialize, Serialize};

use crate::tier::Tier;

/// What an untrusted link holds before (or without) logging in: the device
/// store's `open` setting.
///
/// - `nobody` — a key or password is needed for anything.
/// - `play` — anyone nearby can play; authoring needs a key or password.
/// - `edit` — anyone nearby can play AND author. The default for a board
///   with no store, for now (development and alpha: "like WLED").
///
/// It only ever adds to a login: a link holds the higher of what its login
/// granted and this. Serialized `"nobody"` / `"play"` / `"edit"`, in the
/// persisted device store and on the wire alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum OpenTo {
    Nobody,
    Play,
    Edit,
}

impl OpenTo {
    /// The tier anyone nearby holds, if any.
    #[must_use]
    pub fn tier(self) -> Option<Tier> {
        match self {
            Self::Nobody => None,
            Self::Play => Some(Tier::Play),
            Self::Edit => Some(Tier::Edit),
        }
    }

    /// The setting that grants `tier` to anyone nearby.
    #[must_use]
    pub fn from_tier(tier: Option<Tier>) -> Self {
        match tier {
            None => Self::Nobody,
            Some(Tier::Play) => Self::Play,
            Some(Tier::Edit) => Self::Edit,
        }
    }

    /// Whether anyone nearby holds `tier` with no password.
    #[must_use]
    pub fn grants(self, tier: Tier) -> bool {
        self.tier().is_some_and(|open| open.satisfies(tier))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_open_grants_play_too() {
        assert!(OpenTo::Edit.grants(Tier::Play));
        assert!(OpenTo::Edit.grants(Tier::Edit));
        assert!(OpenTo::Play.grants(Tier::Play));
        assert!(!OpenTo::Play.grants(Tier::Edit));
        assert!(!OpenTo::Nobody.grants(Tier::Play));
    }

    #[test]
    fn serializes_lowercase_and_round_trips_through_a_tier() {
        assert_eq!(
            serde_json::to_string(&OpenTo::Nobody).unwrap(),
            "\"nobody\""
        );
        assert_eq!(serde_json::to_string(&OpenTo::Edit).unwrap(), "\"edit\"");
        for open in [OpenTo::Nobody, OpenTo::Play, OpenTo::Edit] {
            assert_eq!(OpenTo::from_tier(open.tier()), open);
        }
    }
}
