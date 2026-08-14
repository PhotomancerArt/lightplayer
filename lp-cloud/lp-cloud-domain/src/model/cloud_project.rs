//! A published project, as the service records it.

use alloc::string::String;
use lpc_cloud_api::{Access, Actor, ProjectMeta};
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
    /// member, and is the only account that can archive or restore it.
    pub owner: PrefixedUid,
    /// What holding the link grants. Orthogonal to membership.
    pub access: Access,
    /// Human-readable slug for share URLs. Cosmetic.
    pub slug: String,
    /// When the project was published, f64 epoch seconds from the clock
    /// port.
    pub created_at: f64,
    /// When the owner archived it, or `None` while it is live. A timestamp
    /// rather than a flag because "when did this stop being shared" is the
    /// question an operator asks; the wire only carries the boolean
    /// ([`ProjectMeta::archived`]).
    pub archived_at: Option<f64>,
}

impl CloudProject {
    /// The client-facing view of this record.
    pub fn to_meta(&self) -> ProjectMeta {
        ProjectMeta {
            uid: self.uid,
            slug: self.slug.clone(),
            access: self.access,
            owner: Actor::User(self.owner),
            archived: self.archived_at.is_some(),
        }
    }

    /// What this project's *link* grants right now.
    ///
    /// Archiving is expressed here rather than at every call site: an
    /// archived project's link grants nothing, so it drops out of anonymous
    /// reads and public writes by the same rule that governs
    /// [`Access::None`]. Members are unaffected — their access comes from a
    /// membership row, not from the link.
    pub fn effective_access(&self) -> Access {
        match self.archived_at {
            Some(_) => Access::None,
            None => self.access,
        }
    }
}
