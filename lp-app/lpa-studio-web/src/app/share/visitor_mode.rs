//! Who is looking at the project in the address bar — the P6 mode split.
//!
//! One `GetProject` answer decides everything (the same anti-oracle shape
//! `project_share_control` documents):
//!
//! - `members: Some(_)` is the service saying "you are on the roster" —
//!   the owner/editor surface (the P5 Share pill, no banner).
//! - `members: None` on a successful answer is a **link-holder**: the
//!   project resolved for this caller only because its general access said
//!   so. What kind of visitor they are is the access level itself.
//! - An error (`NotFound` included) is no mode at all: no pill, no banner,
//!   no visitor door. Private, archived-to-visitors, and absent are one
//!   indistinguishable case, on purpose.
//!
//! Note the owner-signed-out case falls out honestly: their own project
//! answers them as a link-holder, because signed out that is exactly what
//! they are — their saves would be refused like anybody else's.

use lpc_cloud_api::Access;
use lpc_cloud_api::response::ProjectInfo;

/// How this viewer relates to the open project, per the service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareMode {
    /// On the roster (owner or editor): P5's pill, no banner.
    Member,
    /// Link-holder without write: the strip banner + the visitor popover.
    /// Saves land locally; pushes are refused by the service.
    ViewVisitor,
    /// Link-holder whose link grants writes (`access == Edit`): the calm
    /// one-line live banner — saves go live for everyone.
    EditVisitor,
}

impl ShareMode {
    /// Whether this viewer's saves reach the service.
    pub fn can_write(self) -> bool {
        matches!(self, ShareMode::Member | ShareMode::EditVisitor)
    }

    /// Whether this is a visitor surface at all (either banner).
    pub fn is_visitor(self) -> bool {
        matches!(self, ShareMode::ViewVisitor | ShareMode::EditVisitor)
    }
}

/// Classify one `GetProject` answer. `None` means no share surface: the
/// only way a non-member holds an answer with `access == None` would be a
/// service that leaked, so it is treated as no door rather than trusted.
pub fn share_mode(info: &ProjectInfo) -> Option<ShareMode> {
    if info.members.is_some() {
        return Some(ShareMode::Member);
    }
    match info.meta.access {
        Access::Edit => Some(ShareMode::EditVisitor),
        Access::View => Some(ShareMode::ViewVisitor),
        Access::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_cloud_api::{Actor, ProjectMeta, SidecarMeta};
    use lpc_history::{PrefixedUid, UidPrefix};

    fn info(access: Access, member: bool) -> ProjectInfo {
        ProjectInfo {
            meta: ProjectMeta {
                uid: PrefixedUid::mint(UidPrefix::Project, &[9u8; 16]),
                slug: "dome".to_string(),
                access,
                owner: Actor::Anonymous,
                archived: false,
            },
            heads: Vec::new(),
            sidecar: SidecarMeta {
                name: "Dome".to_string(),
                format_version: 5,
                preview_png: None,
            },
            members: member.then(Vec::new),
        }
    }

    #[test]
    fn a_roster_answer_is_a_member_whatever_the_access() {
        for access in [Access::None, Access::View, Access::Edit] {
            assert_eq!(share_mode(&info(access, true)), Some(ShareMode::Member));
        }
    }

    #[test]
    fn a_link_holder_is_the_access_level() {
        assert_eq!(
            share_mode(&info(Access::View, false)),
            Some(ShareMode::ViewVisitor)
        );
        assert_eq!(
            share_mode(&info(Access::Edit, false)),
            Some(ShareMode::EditVisitor)
        );
        assert_eq!(share_mode(&info(Access::None, false)), None);
    }

    #[test]
    fn write_reaches_the_service_for_members_and_edit_links_only() {
        assert!(ShareMode::Member.can_write());
        assert!(ShareMode::EditVisitor.can_write());
        assert!(!ShareMode::ViewVisitor.can_write());
        assert!(!ShareMode::Member.is_visitor());
        assert!(ShareMode::ViewVisitor.is_visitor());
        assert!(ShareMode::EditVisitor.is_visitor());
    }
}
