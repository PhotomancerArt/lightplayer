//! Stories for the sharing controls themselves — the access segment, its
//! description line, and the people list.
//!
//! These used to mount the chrome's Share pill and its panel; both retired
//! with relationship-control P5, and the controls now live in the project
//! popover's Access section. The popover's own stories
//! (`project_relationship_panel_stories`) cover them IN that skeleton with
//! one roster; these cover what a single fixture cannot — all three access
//! levels side by side, and the people list at its worst — mounted pure,
//! with no cloud service, no session and no context.
//!
//! They mount at the popover's own 320px, because ellipsizing inside that
//! width is the thing being judged.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::{Access, MemberRole};

use crate::app::share::access_controls::{
    AccessDescription, AccessSegment, AddPersonRow, PeopleList,
};
use crate::app::share::share_person::SharePerson;

#[story(
    label = "General access — all three levels",
    description = "The three-way segment and the line under it, in one column so the words can be read against each other. Restricted is the level where the link opens nothing; Anyone can view is what a published project is born at (D2/D3); Anyone can edit goes warn-gold rather than accent-green, pressed segment and description together — the uid IS the capability, so handing out the link is handing out write access and the control should look like the thing it is."
)]
pub(crate) fn access_levels() -> Element {
    panel(rsx! {
        for access in [Access::None, Access::View, Access::Edit] {
            div { key: "{access:?}", class: "tw:grid tw:min-w-0 tw:gap-1.5",
                AccessSegment { access, busy: false }
                AccessDescription { access }
            }
        }
    })
}

#[story(
    label = "People, the awkward set",
    description = "The list at its worst: a very long Latin name over a very long address, a family-first CJK name, a pending invitation that has never been claimed, and the owner row that offers no control at all. Every line must ellipsize inside the popover's 320px rather than widening the panel. (Names are shown here because the list renders them when the service has one — `MemberInfo` carries no display name today, so live rows lead with the email.)"
)]
pub(crate) fn people_awkward() -> Element {
    panel(rsx! {
        PeopleList { people: crew(), on_remove: EventHandler::new(|_: String| {}) }
        AddPersonRow { adding: false }
    })
}

#[story(
    label = "People, read-only",
    description = "The same list without `on_remove` — the project popover's Member state, where you see who else is on a project you do not administer. No Remove buttons and no add row: a read-only roster states who is here, it does not offer a verb it cannot carry out."
)]
pub(crate) fn people_read_only() -> Element {
    panel(rsx! {
        PeopleList { people: crew(), on_remove: None }
    })
}

#[story(
    label = "Add people, unfolded",
    description = "The add affordance sits at the list's BOTTOM (house rule: add-buttons at the insertion point), and unfolds in place into one email box. Membership is keyed by email, so an address that has never signed in is a legal answer — it lands as a pending invitation."
)]
pub(crate) fn people_adding() -> Element {
    panel(rsx! {
        PeopleList { people: vec![yona()], on_remove: EventHandler::new(|_: String| {}) }
        AddPersonRow { adding: true }
    })
}

/// The popover's own box, at the panel's shipped width — these pieces
/// paint no chrome of their own, so the story frame supplies one.
fn panel(children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:min-w-0 tw:gap-2.5 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:p-3.5 tw:text-sm tw:text-muted-foreground tw:shadow-lg",
            {children}
        }
    }
}

/// The spike's people fixtures.
fn crew() -> Vec<SharePerson> {
    vec![
        yona(),
        person(
            "Oliver Voss",
            "oliver@dustcamp.org",
            MemberRole::Editor,
            true,
        ),
        person(
            "Priyanka Ramaswamy-Krishnamurthy",
            "priyanka.ramaswamy.krishnamurthy@lightcrew.example.org",
            MemberRole::Editor,
            false,
        ),
        person(
            "リン・ハヤシ",
            "rin@zookdome.org",
            MemberRole::Editor,
            false,
        ),
    ]
}

fn yona() -> SharePerson {
    SharePerson {
        you: true,
        ..person(
            "Yona Appletree",
            "lightatplay@gmail.com",
            MemberRole::Owner,
            false,
        )
    }
}

fn person(name: &str, email: &str, role: MemberRole, pending: bool) -> SharePerson {
    SharePerson {
        email: email.to_string(),
        role,
        pending,
        display_name: Some(name.to_string()),
        you: false,
    }
}
