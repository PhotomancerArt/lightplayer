//! Stories for the project popover — the five relationship states, one
//! skeleton.
//!
//! The live panel renders from the `CloudSession`, a `GetProject`, the
//! route, and the tab's publish ledger, none of which a story provides —
//! so these mount the pure component with fixtures, the way
//! `share_panel_stories` mounts the Share panel.
//!
//! **Read them as a column.** The point of the phase is that the five
//! states do NOT each invent a shape: header → tabs → Where → Access →
//! action row, in that order, every time. What changes between the
//! captures is the standing sentence, whether Access has controls or a
//! sentence, and which verb sits in slot 1 — never the skeleton.

use dioxus::prelude::*;
use lpa_studio_core::{UiHistoryKind, UiProjectHistory, UiProjectHistoryEntry};
use lpa_studio_web_story_macros::story;
use lpc_cloud_api::{Access, MemberRole};
use lpc_history::{PrefixedUid, UidPrefix};

use crate::app::home::package_export::ExportTarget;
use crate::app::share::project_relationship_panel::{
    PanelTab, ProjectRelationshipPanel, PublishStatus, RosterFacts,
};
use crate::app::share::relationship::ProjectRelationship;
use crate::app::share::share_person::SharePerson;
use crate::app::share::share_url::ShareUrl;

#[story(
    label = "Project popover — example",
    description = "The driving pain, fixed. A built-in example you have not kept: the address is the bare /p/<slug> (no uid — a transient session's uid is RAM-minted and never reaches a URL), Access has nothing to administer and says so in one sentence, and slot 1 is the hero \"Save a copy\" — which works on a PRISTINE session, with no edit required first."
)]
pub(crate) fn project_popover_example() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Small Dome".to_string(),
            relationship: ProjectRelationship::Example,
            url: example_url(),
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
        }
    })
}

#[story(
    label = "Project popover — mine, local",
    description = "A project in this browser's library that the cloud has never answered for. Access has no controls because there is nothing published to control — the honest sentence stands in — and the Where section carries the publish line from the tab's auto-publish ledger (here: a retry, which is the state most worth reading). Slot 1 is the quiet owned verb, Duplicate (D11)."
)]
pub(crate) fn project_popover_mine_local() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Aurora Field".to_string(),
            relationship: ProjectRelationship::MineLocal,
            url: project_url(),
            publish: PublishStatus {
                label: "retrying".to_string(),
                detail: "Last attempt could not reach the cloud.".to_string(),
                trouble: true,
            },
            export: export(),
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
        }
    })
}

#[story(
    label = "Project popover — mine, published",
    description = "The state that owns the sharing controls: the service answered the roster, so Access is the shipped three-way segment, its gate-approved description line, the people list, and the add row at the list's bottom. Same skeleton as every other state — the controls sit in the Access slot rather than replacing it."
)]
pub(crate) fn project_popover_mine_published() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Aurora Field".to_string(),
            relationship: ProjectRelationship::MinePublished,
            url: project_url(),
            roster: RosterFacts {
                access: Access::View,
                people: crew(),
                busy: false,
                can_administer: true,
            },
            export: export(),
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
            on_access: EventHandler::new(|_| {}),
            on_add: EventHandler::new(|_| {}),
            on_remove: EventHandler::new(|_| {}),
        }
    })
}

#[story(
    label = "Project popover — member of someone else's",
    description = "An editor on a project somebody else owns. The roster renders READ-ONLY (no segment, no add row, no Remove buttons): you can see who else is here, and you administer none of it. The owner's name is deliberately absent — the service exposes no owner profile, so \"someone else's\" stands rather than a guess. Slot 1 is the quiet \"Fork my copy\"."
)]
pub(crate) fn project_popover_member() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Halcyon Drift".to_string(),
            relationship: ProjectRelationship::MemberOfSomeoneElses,
            url: shared_url(),
            roster: RosterFacts {
                access: Access::View,
                people: crew(),
                busy: false,
                can_administer: false,
            },
            export: export(),
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
        }
    })
}

#[story(
    label = "Project popover — viewing someone else's",
    description = "A guest on a shared link: no roster (the service answers member lists to members only), one sentence saying what the link grants, and the hero \"Fork — make it yours\" in slot 1, because taking a copy is the only way anything you do here survives. Note there is no export row in the ⋯ menu — there is no library package behind a visit."
)]
pub(crate) fn project_popover_visitor() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Halcyon Drift".to_string(),
            relationship: ProjectRelationship::ViewingSomeoneElses,
            url: shared_url(),
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
        }
    })
}

#[story(
    label = "Project popover — ⋯ open, dirty",
    description = "The overflow disclosed on a project with three unsaved edits. Download .zip and Copy as JSON both read the LIBRARY SNAPSHOT — the bytes on disk — so while edits are pending they would silently hand over the last saved version; they disable and the line says why. Details is the door back to the settings/identity/stats sections the popover replaced, so nothing from the old popup is homeless."
)]
pub(crate) fn project_popover_overflow_open() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Aurora Field".to_string(),
            relationship: ProjectRelationship::MinePublished,
            url: project_url(),
            roster: RosterFacts {
                access: Access::Edit,
                people: vec![yona()],
                busy: false,
                can_administer: true,
            },
            export: export(),
            unsaved: 3,
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
            on_access: EventHandler::new(|_| {}),
            menu_open: true,
        }
    })
}

#[story(
    label = "Project popover — history tab",
    description = "The History tab (D10): the document's own events, newest first, read-only — version, kind, what, when. The top row is NOT an event: three unsaved edits are work in flight, so it wears the warning family and its own box above the hairline stack. A push names its device by uid, not by name — resolving names needs the device registry, which the synchronous view build cannot read — and the footer says plainly that restore has not landed yet."
)]
pub(crate) fn project_popover_history() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Aurora Field".to_string(),
            relationship: ProjectRelationship::MinePublished,
            url: project_url(),
            export: export(),
            unsaved: 3,
            history: history(),
            now_secs: STORY_NOW,
            initial_tab: PanelTab::History,
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
        }
    })
}

#[story(
    label = "Project popover — history, example",
    description = "The same tab on a built-in example. A transient session DOES carry history — the open seeds a provenance origin plus an initial save of the bytes it opened — but that is bookkeeping, not something the person did, so the tab says the true thing instead of listing it. It is also the sentence that stays true after Save a copy: the real rows begin at the first save."
)]
pub(crate) fn project_popover_history_example() -> Element {
    panel(rsx! {
        ProjectRelationshipPanel {
            name: "Small Dome".to_string(),
            relationship: ProjectRelationship::Example,
            url: example_url(),
            history: history(),
            now_secs: STORY_NOW,
            initial_tab: PanelTab::History,
            on_fork: EventHandler::new(|()| {}),
            on_copy: EventHandler::new(|()| {}),
        }
    })
}

/// A fixed clock, so the relative times below never drift between
/// captures.
const STORY_NOW: f64 = 1_800_000_000.0;

/// A representative log: a fork origin, saves, a push, and a join —
/// every row kind the projection emits, newest first the way core hands
/// it over.
fn history() -> UiProjectHistory {
    UiProjectHistory {
        entries: vec![
            entry(Some(12), UiHistoryKind::Saved, "", STORY_NOW - 20.0 * 60.0),
            entry(
                Some(12),
                UiHistoryKind::Pushed,
                "\u{2192} dev7g2k\u{2026}",
                STORY_NOW - 18.0 * 60.0,
            ),
            entry(Some(11), UiHistoryKind::Saved, "", STORY_NOW - 2.0 * 3600.0),
            entry(
                Some(10),
                UiHistoryKind::Joined,
                "kept this version \u{2014} the other was set aside",
                STORY_NOW - 5.0 * 3600.0,
            ),
            entry(
                Some(1),
                UiHistoryKind::Origin,
                "forked from prj4m1x\u{2026}",
                STORY_NOW - 3.0 * 86_400.0,
            ),
        ],
        next_version: Some(13),
    }
}

fn entry(version: Option<u64>, kind: UiHistoryKind, label: &str, at: f64) -> UiProjectHistoryEntry {
    UiProjectHistoryEntry {
        version,
        kind,
        label: label.to_string(),
        at,
    }
}

/// The popover's own box, at the detail card's shipped width — the panel
/// paints no chrome of its own (the popover primitive owns background,
/// border, and shadow), so the story frame supplies one.
fn panel(children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:w-[min(320px,calc(100vw-24px))] tw:min-w-0 tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:text-sm tw:text-muted-foreground tw:shadow-lg",
            {children}
        }
    }
}

/// The example's address: the bare `/p/<slug>`, no uid.
fn example_url() -> ShareUrl {
    ShareUrl {
        origin: "lightplayer.app".to_string(),
        slug: "small-dome".to_string(),
        uid: None,
    }
}

fn project_url() -> ShareUrl {
    ShareUrl {
        origin: "lightplayer.app".to_string(),
        slug: "aurora-field".to_string(),
        uid: Some(PrefixedUid::mint(UidPrefix::Project, &[11u8; 16])),
    }
}

fn shared_url() -> ShareUrl {
    ShareUrl {
        origin: "lightplayer.app".to_string(),
        slug: "halcyon-drift".to_string(),
        uid: Some(PrefixedUid::mint(UidPrefix::Project, &[23u8; 16])),
    }
}

fn export() -> ExportTarget {
    ExportTarget {
        uid: "prj7g2k9m3qxw4tn0b".to_string(),
        slug: "aurora-field".to_string(),
    }
}

/// The share panel's own people fixtures, so the two surfaces are judged
/// against the same roster.
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
