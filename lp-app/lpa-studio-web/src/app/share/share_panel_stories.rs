//! Stories for the Share pill and its panel.
//!
//! The live control renders from the `CloudSession` context and a
//! `GetProject`, which stories never provide — so these mount the pure
//! halves with fixtures.
//!
//! Three of them mount the **real popover, open, at chrome width** (the
//! `sign-in-popover-open` story is the exemplar). That is the story shape
//! that catches what a panel-only fixture cannot: the pill's open treatment
//! (a selection border tint, not a bright pill), and the panel's placement
//! and width against the bar's right edge. It is also the only way to judge
//! these at all — the agent browser pane misdraws every outline popover.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::{Access, MemberRole};
use lpc_history::{PrefixedUid, UidPrefix};

use crate::app::share::share_panel::{SharePanel, SharePillPopover};
use crate::app::share::share_person::SharePerson;
use crate::app::share::share_url::ShareUrl;

#[story(
    label = "Share popover, restricted",
    description = "The panel as it ships: the real popover, mounted open at the end of a chrome-width row. Restricted is the level where the link opens nothing — the segment's line says so in the words the gate approved, and the people list below is the only way in."
)]
pub(crate) fn share_popover_restricted() -> Element {
    open_in_bar(Access::None, crew())
}

#[story(
    label = "Share popover, anyone can view",
    description = "The default a published project is born at (D2/D3): the link opens the project running, no account needed, and only the people below can save. The URL is the hero because copying it IS the share."
)]
pub(crate) fn share_popover_link_view() -> Element {
    open_in_bar(Access::View, crew())
}

#[story(
    label = "Share popover, anyone can edit",
    description = "The level that hands out write access to whoever holds the link. The pressed segment goes warn-gold rather than the plain selection fill (post-gate refinement) and takes its description line with it: the uid IS the capability, and the control should look like the thing it is."
)]
pub(crate) fn share_popover_link_edit() -> Element {
    open_in_bar(Access::Edit, crew())
}

#[story(
    label = "People, the awkward set",
    description = "The list at its worst: a very long Latin name over a very long address, a family-first CJK name, a pending invitation that has never been claimed, and the owner row that offers no control at all. Every line must ellipsize inside 348px rather than widening the panel. (Names are shown here because the panel renders them when the service has one — `MemberInfo` carries no display name today, so live rows lead with the email.)"
)]
pub(crate) fn share_panel_people_awkward() -> Element {
    panel(rsx! {
        SharePanel {
            name: "radiance-dome".to_string(),
            url: url(),
            access: Access::View,
            people: crew(),
        }
    })
}

#[story(
    label = "Add people, unfolded",
    description = "The add affordance sits at the list's BOTTOM (house rule: add-buttons at the insertion point), and unfolds in place into one email box. Membership is keyed by email, so an address that has never signed in is a legal answer — it lands as a pending invitation."
)]
pub(crate) fn share_panel_adding() -> Element {
    panel(rsx! {
        SharePanel {
            name: "radiance-dome".to_string(),
            url: url(),
            access: Access::View,
            people: vec![yona()],
            adding: true,
        }
    })
}

/// The panel mounted open in a chrome-width row, the way it ships.
fn open_in_bar(access: Access, people: Vec<SharePerson>) -> Element {
    rsx! {
        // Room for the panel under the bar; the popover paints in the top
        // layer, so the frame only has to hold the trigger's row.
        div { class: "tw:min-h-[520px]",
            div { class: "tw:flex tw:items-center tw:justify-end tw:gap-2 tw:border-b tw:border-border-subtle tw:pb-2.5",
                SharePillPopover {
                    name: "radiance-dome".to_string(),
                    url: url(),
                    access,
                    people,
                    initially_open: true,
                }
            }
        }
    }
}

/// The panel's own box, for the fixtures that do not need the popover.
fn panel(children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:w-[min(348px,calc(100vw-24px))] tw:min-w-0 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:text-sm tw:text-muted-foreground tw:shadow-lg",
            {children}
        }
    }
}

/// The spike's link, verbatim shape: a readable slug and a forever uid.
fn url() -> ShareUrl {
    ShareUrl {
        origin: "lightplayer.app".to_string(),
        slug: "radiance-dome".to_string(),
        uid: PrefixedUid::mint(UidPrefix::Project, &[11u8; 16]),
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
