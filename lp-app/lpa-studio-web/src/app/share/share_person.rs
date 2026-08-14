//! One row of the share panel's People list, as the panel needs it.
//!
//! [`MemberInfo`] is the wire record: an email, a role, whether the
//! invitation has been claimed, and the account it resolved to. The panel
//! wants a little more than that — who *you* are in the list, and a display
//! name when one exists — so the mapping lives here, host-tested, instead of
//! inside the markup.
//!
//! # The missing name (P2 friction)
//!
//! `MemberInfo` carries **no display name**: membership is keyed by email,
//! and an invitation that has never been claimed has no account to take a
//! name from. So [`SharePerson::display_name`] is `Option` and every live
//! row today fills it with `None`, rendering the email as the headline. The
//! field is not speculative decoration — the spike's people rows are
//! name-over-email, the stories exercise that layout with the awkward set,
//! and the day the service answers with names this is the one line that
//! changes.

use lpc_cloud_api::{MemberInfo, MemberRole};

/// One person's access to this project, ready to render.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharePerson {
    /// The invited address — the row's identity, and what `RemoveMember`
    /// addresses.
    pub email: String,
    /// Owner (fixed) or Editor (removable).
    pub role: MemberRole,
    /// The invitation is still waiting for its first login.
    pub pending: bool,
    /// The account's name when the service knows one; see the module docs.
    pub display_name: Option<String>,
    /// This row is the signed-in account (gets the "(you)" marker).
    pub you: bool,
}

impl SharePerson {
    /// The wire record as a row, told who the viewer is.
    pub fn of_member(member: &MemberInfo, me_email: Option<&str>) -> Self {
        Self {
            email: member.email.clone(),
            role: member.role,
            pending: member.pending,
            display_name: None,
            you: me_email.is_some_and(|me| me.eq_ignore_ascii_case(&member.email)),
        }
    }

    /// The row's headline: the name when there is one, the email otherwise.
    pub fn headline(&self) -> &str {
        self.display_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.email)
    }

    /// The quieter second line: the email, unless it is already the
    /// headline (a nameless row must not print its address twice).
    pub fn secondary(&self) -> Option<&str> {
        (self.headline() != self.email).then_some(self.email.as_str())
    }

    /// One or two letters for the avatar. Never empty — a blank circle
    /// reads as a rendering fault, so a nameless, wordless address still
    /// yields its first character.
    pub fn initials(&self) -> String {
        crate::app::layout::cloud_account::initials(self.headline(), None, None)
    }

    /// The avatar's hue, keyed on the email — the identity the service
    /// keys on, so the same person keeps the same circle everywhere.
    pub fn hue(&self) -> u16 {
        crate::app::layout::cloud_account::avatar_hue(&self.email)
    }
}

/// The member list as rows, in the order the panel shows them: the owner
/// first (there is exactly one, and it anchors the list), then everyone
/// else by address so the order does not shuffle between fetches.
pub fn people_of(members: &[MemberInfo], me_email: Option<&str>) -> Vec<SharePerson> {
    let mut people: Vec<SharePerson> = members
        .iter()
        .map(|member| SharePerson::of_member(member, me_email))
        .collect();
    people.sort_by(|a, b| {
        owner_first(a.role)
            .cmp(&owner_first(b.role))
            .then_with(|| a.email.cmp(&b.email))
    });
    people
}

/// Sort key: owners ahead of editors.
fn owner_first(role: MemberRole) -> u8 {
    match role {
        MemberRole::Owner => 0,
        MemberRole::Editor => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(email: &str, role: MemberRole, pending: bool) -> MemberInfo {
        MemberInfo {
            email: email.to_string(),
            role,
            pending,
            user: None,
        }
    }

    #[test]
    fn the_owner_leads_and_the_rest_are_stable() {
        let members = vec![
            member("zed@example.com", MemberRole::Editor, false),
            member("owner@example.com", MemberRole::Owner, false),
            member("ana@example.com", MemberRole::Editor, true),
        ];
        let people = people_of(&members, Some("ana@example.com"));
        let emails: Vec<&str> = people.iter().map(|p| p.email.as_str()).collect();
        assert_eq!(
            emails,
            ["owner@example.com", "ana@example.com", "zed@example.com"]
        );
        assert!(people[1].you, "the signed-in account is marked");
        assert!(people[1].pending);
        assert!(!people[0].you);
    }

    /// Emails are normalized to lowercase by the service, but a `MeInfo`
    /// from a provider may not be — matching must not depend on it.
    #[test]
    fn you_matches_case_insensitively() {
        let members = vec![member("yona@example.com", MemberRole::Owner, false)];
        let people = people_of(&members, Some("Yona@Example.com"));
        assert!(people[0].you);
    }

    /// Without a name the email IS the headline, and must not be repeated
    /// on the second line.
    #[test]
    fn a_nameless_row_prints_its_address_once() {
        let person = SharePerson::of_member(
            &member("oliver@dustcamp.org", MemberRole::Editor, true),
            None,
        );
        assert_eq!(person.headline(), "oliver@dustcamp.org");
        assert_eq!(person.secondary(), None);
        assert_eq!(person.initials(), "O");
    }

    #[test]
    fn a_named_row_puts_the_address_underneath() {
        let mut person =
            SharePerson::of_member(&member("rin@zookdome.org", MemberRole::Editor, false), None);
        person.display_name = Some("リン・ハヤシ".to_string());
        assert_eq!(person.headline(), "リン・ハヤシ");
        assert_eq!(person.secondary(), Some("rin@zookdome.org"));
    }

    /// A blank name is not a name: the row falls back rather than rendering
    /// an empty headline over its own address.
    #[test]
    fn a_blank_name_falls_back_to_the_address() {
        let mut person = SharePerson::of_member(
            &member("blank@example.com", MemberRole::Editor, false),
            None,
        );
        person.display_name = Some("   ".to_string());
        assert_eq!(person.headline(), "blank@example.com");
        assert_eq!(person.secondary(), None);
    }
}
