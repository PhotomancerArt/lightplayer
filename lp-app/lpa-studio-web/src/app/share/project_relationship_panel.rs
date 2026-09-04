//! The **project popover**: one skeleton, five relationship states
//! (relationship-control vision D9, spike §4 — visual reference only, never
//! ported).
//!
//! ```text
//! ┌───────────────────────────────────┐
//! │ Aurora Field                      │  identity: name · standing · origin
//! │   ◈ Yours — published; link live. │  standing sentence
//! │   lightplayer.app/p/aurora-…  Copy│  the URL: the project's ADDRESS
//! │ ACCESS ───────────────────────────│
//! │   [Restricted|view|edit]  people… │  what the link grants, and to whom
//! │ ─────────────────────────────────—│
//! │   [Duplicate] [Copy link]     [⋯] │  action row, fixed slots
//! └───────────────────────────────────┘
//! ```
//!
//! **One skeleton, no per-state layout forks.** Every state renders the
//! same sections in the same order with the same three action slots; only
//! the *words* and which controls are live change. That is the whole point
//! of the rebuild — five panels that each invented their own shape is what
//! the spike's rejected rounds looked like.
//!
//! **Where owns the URL** (gate ruling, spike round 4). The address bar IS
//! the share link (identity vision D1/D13), so the link is not a footer
//! button or an Access control — it is *where this document lives*, stated
//! in the section that answers "where am I".
//!
//! **Access is purely who-can-do-what.** For a project whose roster the
//! service answered, that is the shipped [`AccessSegment`] +
//! [`AccessDescription`] + [`PeopleList`] — the same controls the Share
//! pill has always had, with the gate-approved description strings
//! verbatim. For every other state it is one honest sentence, because
//! there is no administration to offer.
//!
//! **History does not live here** (D14, amending D10). The document's
//! banked timeline moved into the header control's CHANGES popup — changes
//! and history are one temporal axis — so this panel answers the IDENTITY
//! axis alone: what this document is, where it lives, who can touch it.
//! The rows themselves are [`crate::app::project::HistoryList`].
//!
//! **The fork-family verb is always present**, in slot 1, hero-tinted only
//! where it is *the* move (an example you have not kept, a project you are
//! only visiting). This is the driving pain the vision opened with: a
//! pristine example could not be saved as a copy without editing it first.
//!
//! # Pure
//!
//! Props in, events out — the stories mount all five states with fixtures
//! and no cloud service, no session, no context. The live wiring lives
//! where the project segment mounts this
//! (`app::layout::session_control`); the roster it renders comes from
//! [`super::project_roster`].

use dioxus::prelude::*;
use lpc_cloud_api::Access;

use crate::app::home::package_export::{ExportForm, ExportTarget, export_package_as};
use crate::app::project::{ProjectDetailContent, ProjectDetailSections};
use crate::app::share::access_controls::{
    AccessDescription, AccessSegment, AddPersonRow, PeopleList, ShareUrlHero,
};
use crate::app::share::relationship::ProjectRelationship;
use crate::app::share::share_person::SharePerson;
use crate::app::share::share_url::ShareUrl;
use crate::base::{StudioIcon, StudioIconName};
use crate::core::inline_link_row_class;

/// The Access section's live half, when the service answered the roster.
/// `None` means there is nothing to administer — an example, a visit, or a
/// project the cloud has never heard of — and the section says so in one
/// sentence instead of showing dead controls.
#[derive(Clone, Debug, PartialEq)]
pub struct RosterFacts {
    /// What holding the link grants right now.
    pub access: Access,
    /// The member rows (empty is legal).
    pub people: Vec<SharePerson>,
    /// A `SetAccess` is in flight (the update is optimistic; the segment
    /// stays interactive).
    pub busy: bool,
    /// Whether this viewer may change any of it. False for
    /// [`ProjectRelationship::MemberOfSomeoneElses`]: an editor sees the
    /// roster they are on, and administers nothing.
    pub can_administer: bool,
}

/// The publish line the `MineLocal` Where section carries, read off the
/// tab's auto-publish ledger (`cloud::sync::sync_status`).
///
/// `None` at the call site means the ledger has nothing for this project —
/// the driver has not concluded a trip yet — and the panel falls back to
/// the honest static wording rather than inventing an outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishStatus {
    /// The ledger's badge word: "published", "no save yet", "retrying"…
    pub label: String,
    /// The one human sentence the driver recorded, verbatim.
    pub detail: String,
    /// Whether this reads as trouble (`SyncOutcomeKind::is_failure`).
    pub trouble: bool,
}

/// The project popover's content. Pure: everything below comes from props.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
#[allow(
    clippy::too_many_arguments,
    reason = "Dioxus props are named; this is one struct spelled as arguments"
)]
pub fn ProjectRelationshipPanel(
    /// The document's display name — the panel's title.
    name: String,
    /// The derived relationship (vision D1) that selects every word here.
    relationship: ProjectRelationship,
    /// The project's address. `None` while the session has no addressable
    /// project (a device-hosted project this library does not know), and
    /// the Where section then says so rather than painting a fake link.
    #[props(default)]
    url: Option<ShareUrl>,
    /// The Access section's live half; see [`RosterFacts`].
    #[props(default)]
    roster: Option<RosterFacts>,
    /// The auto-publish ledger's last word on this project (`MineLocal`).
    #[props(default)]
    publish: Option<PublishStatus>,
    /// The library package behind the open project, for the ⋯ menu's two
    /// export forms. `None` — the storeless demo path, or a device-hosted
    /// project — and those rows are absent rather than dead.
    #[props(default)]
    export: Option<ExportTarget>,
    /// Unsaved persisted edits: while any exist both export forms would
    /// hand over the LAST SAVED bytes, so they disable and say so.
    #[props(default = 0)]
    unsaved: usize,
    /// The surviving settings / identity / stats sections, behind the ⋯
    /// menu's "Details" row. `None` with nothing to show.
    #[props(default)]
    details: Option<ProjectDetailContent>,
    /// The fork-family verb. `None` renders it disabled with
    /// `fork_blocked` as its tooltip — a slot that is always present must
    /// still be honest about a verb it cannot dispatch.
    #[props(default)]
    on_fork: Option<EventHandler<()>>,
    /// Why the fork verb is unavailable, when it is.
    #[props(default = String::new())]
    fork_blocked: String,
    #[props(default)] on_copy: Option<EventHandler<()>>,
    #[props(default)] on_access: Option<EventHandler<Access>>,
    #[props(default)] on_add: Option<EventHandler<String>>,
    #[props(default)] on_remove: Option<EventHandler<String>>,
    /// ISO date the project was created (manifest `created`); rides the
    /// identity block's dim line when authored.
    #[props(default)]
    created: Option<String>,
    /// The core view's monotonic fork counter. The panel remembers the
    /// value it MOUNTED with; a bump while this popover is open means the
    /// fork happened right here, and the panel says so loudly — G1
    /// feedback: "it should be much more obvious that something changed".
    #[props(default = 0)]
    fork_generation: u64,
    /// Stories only: mount with the ⋯ overflow already disclosed.
    #[props(default = false)]
    menu_open: bool,
) -> Element {
    // Captured once per popover-open (the popover unmounts its content on
    // close): a bump after mount = the fork happened under this very panel.
    let generation_at_open = use_hook(|| fork_generation);
    let forked_here = fork_generation > generation_at_open;
    let mut menu = use_signal(|| menu_open);
    let mut details_open = use_signal(|| false);

    // The ⋯ menu's Details row swaps the panel's BODY for the surviving
    // detail sections rather than opening a second popover on top of this
    // one (two stacked top-layer panels off one trigger read as stuck).
    // The sections carry their own padding and dividers, so they mount at
    // card level, outside the panel's padded grid.
    if details_open() {
        return rsx! {
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:px-3.5 tw:pb-1 tw:pt-3.5",
                button {
                    class: BACK_BUTTON_CLASS,
                    r#type: "button",
                    onclick: move |_| details_open.set(false),
                    span { class: "tw:text-[11px] tw:font-semibold", "\u{2190} Back" }
                }
                span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:text-dim-foreground",
                    "Details"
                }
            }
            if let Some(details) = details.clone() {
                ProjectDetailSections { content: details }
            }
        };
    }

    let WhereWords {
        glyph,
        lead,
        rest,
        sub,
    } = where_words(relationship);
    let ForkVerb {
        label: fork_label,
        title: fork_title,
        hero,
    } = fork_verb(relationship);
    let fork_class = fork_button_class(hero);
    // The always-present slot has to say WHY when it cannot act.
    let fork_title = if on_fork.is_some() {
        fork_title.to_string()
    } else {
        fork_blocked
    };
    let menu_shown = menu();
    let note = address_note(relationship);
    let access_line = access_sentence(relationship);

    rsx! {
        // One explicit grid wrapper: the popover primitive nests children
        // in its own content div, so the panel class never reaches them.
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:p-3.5",
            if forked_here {
                // The fork's receipt, unmissable (G1): the state flip alone
                // was too quiet for the moment the document became yours.
                div { class: FORKED_BANNER_CLASS,
                    span { "\u{2713} Saved \u{2014} this copy is yours. It's in your Projects now." }
                }
            }
            // The identity block (G1 rework): name, standing, and origin
            // TOGETHER — a bare title read as odd, and a "Where" rule-line
            // under it drew a border through one thought.
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                strong { class: "tw:min-w-0 tw:truncate tw:text-[12.5px] tw:font-bold tw:text-strong-foreground",
                    "{name}"
                }
                div { class: "tw:flex tw:min-w-0 tw:items-start tw:gap-2",
                    span { class: "tw:flex tw:flex-none tw:items-center tw:pt-px tw:text-dim-foreground",
                        StudioIcon { name: glyph, size: 12 }
                    }
                    p { class: "tw:m-0 tw:min-w-0 tw:text-[11px] tw:leading-snug tw:text-muted-foreground",
                        strong { class: "tw:font-semibold tw:text-strong-foreground", "{lead}" }
                        "{rest}"
                    }
                }
                p { class: "tw:m-0 tw:px-0.5 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
                    "{sub}"
                    if let Some(created) = created.as_ref() {
                        span { " \u{b7} Created {created}" }
                    }
                }
            }
            // No tab row (D14 amended D10): history moved to the changes
            // popup, so the identity panel is the only content here. A
            // hairline stands where the tabs were, closing the identity
            // block the way the tab row's rule used to.
            div { class: "tw:h-px tw:min-w-0 tw:bg-border-muted" }

            if let Some(url) = url.clone() {
                    ShareUrlHero { url, on_copy }
                    if let Some(note) = note {
                        p { class: "tw:m-0 tw:px-0.5 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
                            "{note}"
                        }
                    }
                    if relationship == ProjectRelationship::MineLocal {
                        PublishLine { publish }
                    }
                } else {
                    p { class: "tw:m-0 tw:px-0.5 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
                        "No address yet \u{2014} this project has no link the browser can point at."
                    }
                }

                SectionHead { label: "Access" }
                match (relationship, roster.clone()) {
                    // The roster answered and this viewer administers it:
                    // the shipped controls, gate-approved strings and all.
                    (_, Some(facts)) if facts.can_administer => rsx! {
                        AccessSegment { access: facts.access, busy: facts.busy, on_access }
                        AccessDescription { access: facts.access }
                        span { class: GROUP_HEADER_CLASS, "People" }
                        PeopleList { people: facts.people, on_remove }
                        AddPersonRow { on_add, adding: false }
                    },
                    // A roster you are ON but do not administer: your
                    // standing, then who else is here — read-only.
                    (ProjectRelationship::MemberOfSomeoneElses, Some(facts)) => rsx! {
                        p { class: "tw:m-0 tw:px-0.5 tw:text-[10.5px] tw:leading-snug tw:text-status-good-foreground",
                            "You can edit \u{2014} your saves go to the shared project."
                        }
                        span { class: GROUP_HEADER_CLASS, "People" }
                        PeopleList { people: facts.people, on_remove: None }
                    },
                    // Nothing to administer: one honest sentence.
                    _ => rsx! {
                        p { class: "tw:m-0 tw:px-0.5 tw:text-[10.5px] tw:leading-snug tw:text-dim-foreground",
                            "{access_line}"
                        }
                    },
                }

                div { class: ACTION_ROW_CLASS,
                    button {
                        class: "{fork_class}",
                        r#type: "button",
                        disabled: on_fork.is_none(),
                        title: "{fork_title}",
                        onclick: move |_| {
                            if let Some(on_fork) = on_fork {
                                on_fork.call(());
                            }
                        },
                        StudioIcon { name: StudioIconName::Copy, size: 12 }
                        span { class: "tw:min-w-0 tw:truncate tw:text-[11px] tw:font-semibold", "{fork_label}" }
                    }
                    button {
                        class: SIDE_BUTTON_CLASS,
                        r#type: "button",
                        title: "Copy this project's link",
                        disabled: on_copy.is_none(),
                        onclick: move |_| {
                            if let Some(on_copy) = on_copy {
                                on_copy.call(());
                            }
                        },
                        span { class: "tw:text-[11px] tw:font-semibold", "Copy link" }
                    }
                    button {
                        class: SIDE_BUTTON_CLASS,
                        r#type: "button",
                        aria_label: "More project actions",
                        aria_expanded: "{menu_shown}",
                        title: "Download .zip \u{00b7} Copy as JSON \u{00b7} Details",
                        onclick: move |_| {
                            let open = menu();
                            menu.set(!open);
                        },
                        StudioIcon { name: StudioIconName::More, size: 13 }
                    }
                }
                if menu_shown {
                    // A disclosure, not a second popover: a menu panel in
                    // the top layer above a panel already in the top layer
                    // is exactly the "stuck menu" the chrome's ⋯ row
                    // closes itself to avoid.
                    OverflowRows {
                        export,
                        unsaved,
                        has_details: details.is_some(),
                        on_details: EventHandler::new(move |()| {
                            menu.set(false);
                            details_open.set(true);
                        }),
                    }
                }
        }
    }
}

/// A section head: the small-caps label with a hairline running to the
/// panel's edge (D9 — heads are rules, NOT a vertical rail; the rail was
/// retired at spike round 5).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SectionHead(label: &'static str) -> Element {
    rsx! {
        div { class: "tw:mt-1.5 tw:flex tw:min-w-0 tw:items-center tw:gap-2",
            span { class: GROUP_HEADER_CLASS, "{label}" }
            // The `::after` rule of the spike, as a flex child — Tailwind
            // has no content-generating utility, and a div is one node.
            div { class: "tw:h-px tw:flex-1 tw:bg-border-muted" }
        }
    }
}

/// The `MineLocal` publish line: the tab's auto-publish ledger, or the
/// honest static wording when it has concluded nothing yet.
///
/// This surface never *drives* publishing — it reads the notebook the
/// driver keeps (`cloud::sync::sync_status`). No ledger row is not a
/// failure; it is a driver that has not run a trip for this project in
/// this tab.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PublishLine(publish: Option<PublishStatus>) -> Element {
    let Some(publish) = publish else {
        return rsx! {
            p { class: "tw:m-0 tw:px-0.5 tw:text-[10px] tw:leading-snug tw:text-dim-foreground",
                "Publishes on save while you\u{2019}re signed in \u{2014} nothing has gone up from this tab yet."
            }
        };
    };
    let tone = if publish.trouble {
        "tw:text-status-warning-foreground"
    } else {
        "tw:text-dim-foreground"
    };
    rsx! {
        p { class: "tw:m-0 tw:px-0.5 tw:text-[10px] tw:leading-snug {tone}",
            span { class: "tw:font-semibold", "{publish.label}" }
            " \u{2014} {publish.detail}"
        }
    }
}

/// One overflow row: icon, label, and a disabled state that explains
/// itself through the row's title.
///
/// Inherited verbatim from the retired `ProjectShareSection` (the detail
/// popup's old "Share" block, P5) — same rows, same words, same
/// dirty-disable rule, one home.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShareRow(
    label: &'static str,
    hint: &'static str,
    icon: StudioIconName,
    disabled: bool,
    on_press: EventHandler<()>,
) -> Element {
    let title = if disabled {
        "Save this project to share it."
    } else {
        hint
    };
    let class = inline_link_row_class(disabled);

    rsx! {
        button {
            class,
            r#type: "button",
            disabled,
            title,
            onclick: move |event| {
                event.stop_propagation();
                if !disabled {
                    on_press.call(());
                }
            },
            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:flex-none tw:items-center tw:justify-center", aria_hidden: "true",
                StudioIcon { name: icon, size: 14 }
            }
            span { class: "tw:min-w-0 tw:truncate", "{label}" }
        }
    }
}

/// The ⋯ overflow's rows: the two export forms and the door back to the
/// settings/identity/stats sections.
///
/// The forms read the **library snapshot** — the bytes on disk — while
/// unsaved edits live in the overlay, so a dirty project would silently
/// export its last saved version. They disable and say so instead (the
/// rule and its line come from `ProjectShareSection`, which this replaced
/// at P5).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn OverflowRows(
    export: Option<ExportTarget>,
    unsaved: usize,
    has_details: bool,
    on_details: EventHandler<()>,
) -> Element {
    let dirty = unsaved > 0;
    let zip_target = export.clone();
    let json_target = export.clone();
    rsx! {
        div { class: MENU_CLASS,
            if let Some(zip_target) = zip_target {
                ShareRow {
                    label: "Download .zip",
                    hint: "Download this project as a zip archive.",
                    icon: StudioIconName::Download,
                    disabled: dirty,
                    on_press: move |_| export_package_as(zip_target.clone(), ExportForm::Zip),
                }
            }
            if let Some(json_target) = json_target {
                ShareRow {
                    label: "Copy as JSON",
                    hint: "Copy this project as a shareable JSON envelope.",
                    icon: StudioIconName::Copy,
                    disabled: dirty,
                    on_press: move |_| {
                        export_package_as(json_target.clone(), ExportForm::JsonToClipboard)
                    },
                }
            }
            if has_details {
                ShareRow {
                    label: "Details",
                    hint: "Project settings, identity, and stats.",
                    icon: StudioIconName::Info,
                    disabled: false,
                    on_press: move |_| on_details.call(()),
                }
            }
            if dirty && export.is_some() {
                p { class: "tw:m-0 tw:pt-1 tw:text-[10px] tw:leading-snug tw:text-subtle-foreground",
                    "Save first \u{2014} sharing sends the last saved version, not your unsaved edits."
                }
            }
        }
    }
}

/// The Where section's standing sentence, per state.
///
/// The lead is bold and the rest completes it; `sub` is the quieter line
/// below. Provenance prose ("Forked from Plasma Duo · 3 days ago") is NOT
/// reachable from an open project today — it lives on `PackageMeta` and
/// surfaces only on gallery cards, and `view.home` is `None` while an
/// editor is open — so `sub` says what this surface actually knows.
struct WhereWords {
    glyph: StudioIconName,
    lead: &'static str,
    rest: &'static str,
    sub: &'static str,
}

fn where_words(relationship: ProjectRelationship) -> WhereWords {
    match relationship {
        ProjectRelationship::Example => WhereWords {
            glyph: StudioIconName::Test,
            lead: "Built-in example",
            rest: " \u{2014} curated, and the same for everyone.",
            sub: "Play freely \u{2014} nothing saves until you keep a copy.",
        },
        ProjectRelationship::MineLocal => WhereWords {
            glyph: StudioIconName::RelationshipPrivate,
            lead: "Yours",
            rest: " \u{2014} in this browser\u{2019}s library.",
            sub: "Nobody else can open it.",
        },
        ProjectRelationship::MinePublished => WhereWords {
            glyph: StudioIconName::RelationshipShared,
            lead: "Yours",
            rest: " \u{2014} published; the link is live.",
            sub: "Saved versions publish to the cloud copy behind this link.",
        },
        ProjectRelationship::MemberOfSomeoneElses => WhereWords {
            glyph: StudioIconName::RelationshipMember,
            lead: "Someone else\u{2019}s",
            rest: " \u{2014} shared with you.",
            sub: "You are on the roster below; the owner\u{2019}s name is not shown.",
        },
        ProjectRelationship::ViewingSomeoneElses => WhereWords {
            glyph: StudioIconName::RelationshipViewing,
            lead: "Someone else\u{2019}s",
            rest: " \u{2014} you\u{2019}re viewing it as a guest.",
            sub: "The owner is unnamed \u{2014} the service only tells members.",
        },
    }
}

/// The line under the URL, where the address needs one. `MineLocal` has no
/// note here — [`PublishLine`] is its line.
fn address_note(relationship: ProjectRelationship) -> Option<&'static str> {
    match relationship {
        ProjectRelationship::Example => {
            Some("The canonical example link \u{2014} opens for anyone, always.")
        }
        ProjectRelationship::MinePublished => {
            Some("Same link as the address bar \u{2014} copying either works.")
        }
        ProjectRelationship::MineLocal
        | ProjectRelationship::MemberOfSomeoneElses
        | ProjectRelationship::ViewingSomeoneElses => None,
    }
}

/// The Access section for a state with nothing to administer.
fn access_sentence(relationship: ProjectRelationship) -> &'static str {
    match relationship {
        ProjectRelationship::Example => {
            "Everyone can open this example \u{2014} it\u{2019}s built in. Your copy gets its own access control once you save it."
        }
        ProjectRelationship::ViewingSomeoneElses => {
            "You hold view access \u{2014} the link is the key, no account needed. Nothing you touch changes their copy."
        }
        ProjectRelationship::MineLocal => {
            "Not shared \u{2014} it lives in this browser\u{2019}s library. Access controls appear once it reaches the cloud."
        }
        // Both of these carry a roster when the service has answered; this
        // is the answer-not-yet-in case, and saying so beats dead controls.
        ProjectRelationship::MinePublished | ProjectRelationship::MemberOfSomeoneElses => {
            "Waiting on the service for this project\u{2019}s access and roster."
        }
    }
}

/// Slot 1 of the action row: the fork-family verb.
///
/// It is ALWAYS present (D9) — there is always a way to end up with your
/// own copy — and hero-tinted only where forking is *the* move. "Duplicate"
/// is the owned verb for a project already yours (D11): the same word the
/// gallery card uses, so one gesture has one name in the app.
pub struct ForkVerb {
    pub label: &'static str,
    pub title: &'static str,
    /// Hero emphasis (status-good family). Quiet otherwise.
    pub hero: bool,
}

pub fn fork_verb(relationship: ProjectRelationship) -> ForkVerb {
    match relationship {
        ProjectRelationship::Example => ForkVerb {
            label: "Save a copy",
            title: "Keep this example as your own project \u{2014} no edit required",
            hero: true,
        },
        ProjectRelationship::ViewingSomeoneElses => ForkVerb {
            label: "Fork \u{2014} make it yours",
            title: "Take a copy into your library so your changes are kept",
            hero: true,
        },
        ProjectRelationship::MineLocal | ProjectRelationship::MinePublished => ForkVerb {
            label: "Duplicate",
            title: "Duplicate this project \u{2014} an independent copy at its current saved version",
            hero: false,
        },
        ProjectRelationship::MemberOfSomeoneElses => ForkVerb {
            label: "Fork my copy",
            title: "Take an independent copy \u{2014} it leaves the shared project behind",
            hero: false,
        },
    }
}

/// Slot 1's paint. Hero = the status-good family (the spike's `.act-hero`);
/// quiet = the neutral outline every other panel button wears. No accent
/// tokens anywhere (D12) — this is an act, not an identity.
fn fork_button_class(hero: bool) -> String {
    let state = if hero {
        "tw:border-status-good-border tw:bg-status-good-bg tw:text-status-good-foreground tw:hover:border-status-good-foreground"
    } else {
        "tw:border-border-strong tw:bg-card-subtle tw:text-muted-foreground tw:hover:border-dim-foreground tw:hover:text-strong-foreground"
    };
    format!("{ACTION_BASE_CLASS} tw:min-w-0 tw:flex-1 {state}")
}

/// The fork receipt (G1): status-good, full-width, impossible to miss.
const FORKED_BANNER_CLASS: &str = "tw:flex tw:min-w-0 tw:items-center tw:gap-2 tw:rounded-md tw:border tw:border-status-good-border tw:bg-status-good-bg tw:px-2.5 tw:py-2 tw:text-[11px] tw:font-semibold tw:text-status-good-foreground";

/// The action row: a hairline above it, because it is the panel's footer
/// even though it is not a footer bar.
const ACTION_ROW_CLASS: &str = "tw:mt-1.5 tw:flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:border-t tw:border-border-muted tw:pt-2.5";
/// Every action-row button's geometry, before its family.
const ACTION_BASE_CLASS: &str = "tw:inline-flex tw:cursor-pointer tw:items-center tw:justify-center tw:gap-1.5 tw:rounded-sm tw:border tw:px-2.5 tw:py-1.5 tw:transition-colors tw:disabled:cursor-not-allowed tw:disabled:opacity-60";
/// Slots 2 and 3: quiet, transparent, flex-none — the fork verb is the one
/// that grows.
const SIDE_BUTTON_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:gap-1.5 tw:rounded-sm tw:border tw:border-border-strong tw:bg-transparent tw:px-2.5 tw:py-1.5 tw:text-subtle-foreground tw:transition-colors tw:hover:border-dim-foreground tw:hover:text-strong-foreground tw:disabled:cursor-not-allowed tw:disabled:opacity-60";
/// The ⋯ disclosure's box.
const MENU_CLASS: &str = "tw:grid tw:min-w-0 tw:gap-1 tw:rounded-sm tw:border tw:border-border-muted tw:bg-card-muted tw:px-2.5 tw:py-2";
/// The back door out of Details.
const BACK_BUTTON_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:rounded-sm tw:border tw:border-border-strong tw:bg-transparent tw:px-2 tw:py-1 tw:text-subtle-foreground tw:transition-colors tw:hover:border-dim-foreground tw:hover:text-strong-foreground";
/// The small-caps mini-header a section head wears — the Share panel's own
/// group header, so both surfaces label a group the same way.
const GROUP_HEADER_CLASS: &str = "tw:flex-none tw:text-[0.68rem] tw:font-bold tw:uppercase tw:tracking-wide tw:text-subtle-foreground";
#[cfg(test)]
mod tests {
    use super::*;
    /// Slot 1 is never empty, and the hero tint belongs to the two states
    /// where forking is THE move — the example you have not kept, and the
    /// project you are only visiting.
    #[test]
    fn every_state_has_a_fork_verb_and_only_two_are_heroes() {
        let verbs = [
            ProjectRelationship::Example,
            ProjectRelationship::MineLocal,
            ProjectRelationship::MinePublished,
            ProjectRelationship::MemberOfSomeoneElses,
            ProjectRelationship::ViewingSomeoneElses,
        ]
        .map(fork_verb);

        assert_eq!(
            verbs.each_ref().map(|verb| verb.label),
            [
                "Save a copy",
                "Duplicate",
                "Duplicate",
                "Fork my copy",
                "Fork \u{2014} make it yours",
            ]
        );
        assert_eq!(
            verbs.each_ref().map(|verb| verb.hero),
            [true, false, false, false, true]
        );
        // Every slot names a tooltip: an always-present verb that cannot
        // say what it does is worse than one that is sometimes absent.
        for verb in &verbs {
            assert!(!verb.title.is_empty());
        }
    }

    /// D11: a project already yours is DUPLICATED, in the gallery's word.
    #[test]
    fn a_project_already_yours_wears_the_owned_verb() {
        assert_eq!(fork_verb(ProjectRelationship::MineLocal).label, "Duplicate");
        assert_eq!(
            fork_verb(ProjectRelationship::MinePublished).label,
            "Duplicate"
        );
    }

    /// One skeleton means every state answers Where and Access — no state
    /// may fall through to an empty section.
    #[test]
    fn every_state_says_something_in_both_sections() {
        for relationship in [
            ProjectRelationship::Example,
            ProjectRelationship::MineLocal,
            ProjectRelationship::MinePublished,
            ProjectRelationship::MemberOfSomeoneElses,
            ProjectRelationship::ViewingSomeoneElses,
        ] {
            let words = where_words(relationship);
            assert!(!words.lead.is_empty(), "{relationship:?} has no lead");
            assert!(!words.rest.is_empty(), "{relationship:?} has no rest");
            assert!(!words.sub.is_empty(), "{relationship:?} has no sub line");
            assert!(
                !access_sentence(relationship).is_empty(),
                "{relationship:?} has no access sentence"
            );
        }
    }

    /// The link note is a per-state fact, not decoration: the example's
    /// link is canonical and forever, a published project's is the address
    /// bar's own, and the other three have nothing extra to claim.
    #[test]
    fn only_the_two_states_with_a_link_claim_carry_a_note() {
        assert!(address_note(ProjectRelationship::Example).is_some());
        assert!(address_note(ProjectRelationship::MinePublished).is_some());
        assert!(address_note(ProjectRelationship::MineLocal).is_none());
        assert!(address_note(ProjectRelationship::MemberOfSomeoneElses).is_none());
        assert!(address_note(ProjectRelationship::ViewingSomeoneElses).is_none());
    }

    /// No preflight: every button class in this panel names its own border
    /// AND background, or the browser paints UA `buttonface`.
    #[test]
    fn every_button_class_names_a_border_and_a_background() {
        for class in [
            fork_button_class(true),
            fork_button_class(false),
            SIDE_BUTTON_CLASS.to_string(),
            BACK_BUTTON_CLASS.to_string(),
        ] {
            assert!(class.contains("tw:bg-"), "no background in `{class}`");
            assert!(class.contains("tw:border"), "no border in `{class}`");
        }
    }
}
