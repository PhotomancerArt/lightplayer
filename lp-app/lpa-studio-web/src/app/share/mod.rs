//! The owner's sharing surface: the Share pill, the panel it opens, and
//! the archive drawer the removal verb lands in.
//!
//! The premise (identity vision D1/D13) is that the **address bar is the
//! share link** — every project lives at `/p/<slug>-<uid>`, published from
//! birth, and copying the URL is the whole act of sharing. So nothing here
//! creates a link. This is *access control*: what holding that link grants
//! (`none | view | edit`), orthogonally to who has been added by name, and
//! how a project leaves the library without anything being destroyed.
//!
//! - [`share_panel`] — the pill and the panel, pure and story-mountable.
//! - [`project_share_control`] — the live half: one `GetProject` decides
//!   whether the door exists at all, and the panel's events become
//!   `SetAccess` / `AddMember` / `RemoveMember`.
//! - [`share_person`] / [`share_url`] — the two small view models the panel
//!   renders from, host-tested away from the markup.
//! - [`archived_projects`] — the Projects page's collapsed archive drawer
//!   and its one loud verb, Restore.
//! - [`visitor_mode`] / [`visitor_banner`] / [`visitor_popover`] — the P6
//!   visitor surface: who this viewer is per the service, the strip under
//!   the chrome, and the read-only share door in the pill slot.
//! - [`relationship`] — the relationship-control vision's one derived
//!   `ProjectRelationship`, plus the pristine-transient fork dispatch.
//!
//! Visual reference: `spikes/project-share/index.html` §1-A, §2-B, §2-D,
//! §3-A and §5 (gate rulings G1/G2/G3/G4 + Q12). Production code never
//! imports from `spikes/`.

pub mod archived_projects;
#[cfg(feature = "stories")]
pub(crate) mod archived_projects_stories;
pub mod project_share_control;
pub mod relationship;
pub mod share_panel;
#[cfg(feature = "stories")]
pub(crate) mod share_panel_stories;
pub mod share_person;
pub mod share_url;
pub mod visitor_banner;
#[cfg(feature = "stories")]
pub(crate) mod visitor_banner_stories;
pub mod visitor_mode;
pub mod visitor_popover;
pub mod visitor_session;

pub use archived_projects::{ArchivedProject, ArchivedProjectsList, ArchivedProjectsSection};
pub use project_share_control::{ProjectShareControl, archive_project};
pub use relationship::{
    ProjectRelationship, RelationshipFace, derive_relationship, fork_transient_session,
    relationship_face,
};
pub use share_panel::{SharePanel, SharePillPopover};
pub use share_person::{SharePerson, people_of};
pub use share_url::ShareUrl;
pub use visitor_banner::{BannerState, VisitorBanner, VisitorBannerView};
pub use visitor_mode::ShareMode;
pub use visitor_popover::VisitorSharePopover;
pub(crate) use visitor_session::use_visitor_session;
pub use visitor_session::{VisitorBannerHost, VisitorShareSlot};
