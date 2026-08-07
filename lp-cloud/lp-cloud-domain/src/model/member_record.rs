//! One person's access to one project.

use alloc::string::String;
use lpc_history::PrefixedUid;

use crate::model::member_role::MemberRole;

/// A membership row: project × email, with the account it resolved to.
///
/// **Pending membership** (Q4): a member is invited by *email*, which may
/// belong to someone who has never logged in. Such a row is stored with
/// `user: None` and grants nothing until that email authenticates for the
/// first time, at which point
/// [`MetaStore::resolve_pending_members`](crate::ports::meta_store::MetaStore::resolve_pending_members)
/// fills in the uid. Access checks match on the resolved uid only — an
/// unresolved row is an invitation, not a key.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberRecord {
    /// The project this grants access to.
    pub project: PrefixedUid,
    /// The invited email, normalized to lowercase. The row's identity
    /// together with `project`.
    pub email: String,
    /// The account this email resolved to, or `None` while pending.
    pub user: Option<PrefixedUid>,
    /// Owner or plain member.
    pub role: MemberRole,
    /// When the row was created, f64 epoch seconds from the clock port.
    pub added_at: f64,
}
