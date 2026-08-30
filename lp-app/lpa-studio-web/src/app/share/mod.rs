//! The sharing surface: the project popover's Where/Access content, the
//! live roster behind it, and the archive drawer the removal verb lands in.
//!
//! The premise (identity vision D1/D13) is that the **address bar is the
//! share link** — every project lives at `/p/<slug>-<uid>`, published from
//! birth, and copying the URL is the whole act of sharing. So nothing here
//! creates a link. This is *access control*: what holding that link grants
//! (`none | view | edit`), orthogonally to who has been added by name, and
//! how a project leaves the library without anything being destroyed.
//!
//! **One door.** The standalone Share pill (`ProjectShareControl` /
//! `SharePillPopover`), its ⋯-menu "Sharing & access…" row, and the
//! visitor's read-only variant of the same slot (`VisitorSharePopover`)
//! all retired with relationship-control P5. The chrome's PROJECT segment
//! is the entry now, for owner and visitor alike, and its popover renders
//! from the derived relationship rather than from whether the service
//! answered a roster.
//!
//! - [`access_controls`] — the controls themselves (URL hero, access
//!   segment + description, people list, add row), pure and
//!   story-mountable; the relationship panel composes them.
//! - [`project_roster`] — the live half: one `GetProject` decides whether
//!   there is anything to administer, and the events become `SetAccess` /
//!   `AddMember` / `RemoveMember`.
//! - [`project_relationship_panel`] — the PROJECT segment's popover
//!   (relationship-control D9): one skeleton — Where / Access / action row
//!   — rendered for all five [`ProjectRelationship`] states.
//! - [`share_person`] / [`share_url`] — the two small view models the panel
//!   renders from, host-tested away from the markup.
//! - [`archived_projects`] — the Projects page's collapsed archive drawer,
//!   its one loud verb Restore, and the `archive_project` half.
//! - [`visitor_mode`] / [`visitor_banner`] — the P6 visitor surface: who
//!   this viewer is per the service, and the status strip under the chrome
//!   (its fork CTA moved into the project popover's action row).
//! - [`relationship`] — the relationship-control vision's one derived
//!   `ProjectRelationship`, plus the pristine-transient fork dispatch.
//!
//! Visual reference: `spikes/project-share/index.html` §1-A, §2-B, §2-D,
//! §3-A and §5 (gate rulings G1/G2/G3/G4 + Q12). Production code never
//! imports from `spikes/`.

pub mod access_controls;
#[cfg(feature = "stories")]
pub(crate) mod access_controls_stories;
pub mod archived_projects;
#[cfg(feature = "stories")]
pub(crate) mod archived_projects_stories;
pub mod project_relationship_panel;
#[cfg(feature = "stories")]
pub(crate) mod project_relationship_panel_stories;
pub mod project_roster;
pub mod relationship;
pub mod share_person;
pub mod share_url;
pub mod visitor_banner;
#[cfg(feature = "stories")]
pub(crate) mod visitor_banner_stories;
pub mod visitor_mode;
pub mod visitor_session;

pub use archived_projects::{
    ArchivedProject, ArchivedProjectsList, ArchivedProjectsSection, archive_project,
};
pub use project_relationship_panel::{
    ForkVerb, PanelTab, ProjectRelationshipPanel, PublishStatus, RosterFacts, fork_verb,
};
pub use project_roster::{ProjectRoster, RosterState, use_project_roster, viewer_actor};
pub use relationship::{
    ProjectRelationship, RelationshipFace, derive_relationship, fork_transient_session,
    relationship_face,
};
pub use share_person::{SharePerson, people_of};
pub use share_url::ShareUrl;
pub use visitor_banner::{BannerState, VisitorBanner, VisitorBannerView};
pub use visitor_mode::ShareMode;
pub use visitor_session::VisitorBannerHost;
pub(crate) use visitor_session::use_visitor_session;
