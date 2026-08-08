//! A published project, as the service records it.

use alloc::string::String;
use lpc_cloud_api::{Actor, ProjectMeta, Visibility};
use lpc_history::PrefixedUid;

/// Server-side record of a published project.
///
/// The uid is **client-minted** (D21): its 95 bits of entropy are the share
/// link, so the client owns it and `PublishProject` records what it was
/// handed. The slug is cosmetic — it decorates share URLs and is
/// re-derivable from a later push's sidecar name; the uid alone is
/// authoritative.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudProject {
    /// The project's uid (`prj…`), minted client-side.
    pub uid: PrefixedUid,
    /// The account that published it. The owner can never be removed as a
    /// member.
    pub owner: PrefixedUid,
    /// Current access level.
    pub visibility: Visibility,
    /// Human-readable slug for share URLs. Cosmetic.
    pub slug: String,
    /// When the project was published, f64 epoch seconds from the clock
    /// port.
    pub created_at: f64,
}

impl CloudProject {
    /// The client-facing view of this record.
    pub fn to_meta(&self) -> ProjectMeta {
        ProjectMeta {
            uid: self.uid,
            slug: self.slug.clone(),
            visibility: self.visibility,
            owner: Actor::User(self.owner),
        }
    }
}
