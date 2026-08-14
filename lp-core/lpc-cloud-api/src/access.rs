//! What holding a project's link lets you do.

use serde::{Deserialize, Serialize};

/// What the *link* grants — the access anyone holding a project's uid has,
/// independent of who they are.
///
/// This is deliberately orthogonal to membership
/// ([`crate::request::AddMember`]): members always read and write, and this
/// says what everybody *else* can do. `None` is not "private to the owner",
/// it is "the link opens nothing"; a project with members and `Access::None`
/// is a shared project that no link reaches.
///
/// The variants are ordered — `None < View < Edit` — and comparisons are
/// meant to be used: the read rule is `access >= Access::View`. Adding a
/// level between two existing ones is a deliberate act that must keep the
/// order meaningful.
///
/// `Public` (search-indexed/discoverable) is deliberately absent day one —
/// the only way to reach a project is to already hold its uid, which doubles
/// as the share link. Adding indexed discovery later is a new variant, not a
/// reinterpretation of an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    /// The link opens nothing. Only the owner and explicit members
    /// ([`crate::request::AddMember`]) can reach the project at all, and a
    /// caller who is neither is told the project does not exist.
    None,
    /// Anyone holding the project's uid (the link) can read it anonymously —
    /// no login, no membership check. This is the "share one project by URL"
    /// path.
    View,
    /// Anyone holding the uid can also *write*: push commits, no account
    /// required. The 95-bit uid is the capability, and handing it out is
    /// handing out edit rights.
    Edit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trips_every_level() {
        for access in [Access::None, Access::View, Access::Edit] {
            let json = serde_json::to_string(&access).unwrap();
            let back: Access = serde_json::from_str(&json).unwrap();
            assert_eq!(back, access);
        }
    }

    /// Pinned JSON literals: the deployed format is the contract.
    #[test]
    fn pinned_json_literals() {
        assert_eq!(serde_json::to_string(&Access::None).unwrap(), "\"none\"");
        assert_eq!(serde_json::to_string(&Access::View).unwrap(), "\"view\"");
        assert_eq!(serde_json::to_string(&Access::Edit).unwrap(), "\"edit\"");
    }

    /// The read rule is written as a comparison, so the order is part of the
    /// contract rather than an accident of declaration.
    #[test]
    fn levels_are_ordered_none_view_edit() {
        assert!(Access::None < Access::View);
        assert!(Access::View < Access::Edit);
        assert!(Access::View >= Access::View);
        assert!(Access::Edit >= Access::View);
        assert!(Access::None < Access::Edit);
    }
}
