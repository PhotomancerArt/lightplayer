//! The header **session·project control**: one shell, three segments, three
//! popovers — `[ device | project | changes ]`.
//!
//! `❖ Sim · ESP32-C6 │ ◈ small dome · 🔒 Private │ ✎ ② [Save]` — one bordered
//! shell with internal hairlines, each segment an independently-clickable
//! [`DetailPopover`] trigger, and Save standing beside the shell as the one
//! direct act (relationship-control D7/D8, spike §2-E).
//!
//! **Three questions, one bar.** What is running · what document is open ·
//! what is in flight. Changes earn their own segment because pending edits
//! span BOTH stores — they are already live on the session, and Save banks
//! them to the library — so they belong to neither the device box nor the
//! document box alone.
//!
//! **Why per-segment hover now (the #432 rationale, inverted).** The fused
//! lockup washed on hover as ONE object, and deliberately: device and
//! project did the same thing — open the same panel — so lighting them
//! separately would have promised a distinction that did not exist. That
//! argument runs the other way here. Each segment opens a DIFFERENT panel,
//! so the hover wash rides the segment under the cursor and the shell stays
//! quiet: the wash is now the honest promise it once would have faked. The
//! shell keeps the outer border and radius, and drawn hairlines mark the
//! seams — still one control, with three doors.
//!
//! **Trigger subtrees stay stateless.** While a popover is open its trigger
//! renders TWICE (the in-flow placeholder holding layout and focus, plus the
//! top-layer copy above the merged outline), so nothing stateful and nothing
//! interactive may live inside a segment. Save is a sibling button outside
//! every trigger subtree and renders once; revert-all and the per-entry
//! reverts live in the changes POPUP, which is panel content, not trigger
//! content.
//!
//! **One ungated mount** (Q10 lesson, #426): the control is never wrapped in
//! a `tw:@min-*` container. A top-layer popover cannot answer a container
//! query, and mounting a trigger twice gives the header two popovers that
//! disagree about which one is open. The FOLDS ride the pieces instead — the
//! device name and the relationship WORD hide below the 900px cut (their
//! glyphs stay), the changes segment goes count-only and the padding tightens
//! below 560px.
//!
//! **Three popovers, three concepts.** The device segment states what is
//! running; the changes segment lists what is in flight; the project
//! segment opens [`ProjectRelationshipPanel`] — the fixed skeleton (Where /
//! Access / action row, with History as a tab) rendered for all five
//! [`ProjectRelationship`] states. The detail sections that used to hang
//! here are not homeless: they sit behind that panel's ⋯ menu.
//!
//! The control is presentational: everything it renders comes from
//! [`UiChromeSessionControl`] (core's projection of THE session), the open
//! project's [`ProjectDetailContent`] — the same value the pane's [i]
//! renders, so the chrome and the pane can never disagree about a project —
//! the derived [`ProjectRelationship`] the caller computes, and
//! [`ProjectPopoverInputs`] (the address, roster, publish ledger, and
//! dispatches only the web shell can gather).

use dioxus::prelude::*;
use lpa_studio_core::{
    DirtySummary, UiAction, UiAffordance, UiChromeSessionControl, UiChromeSessionStatus,
    UiPaneAction,
};
use lpc_cloud_api::Access;

use crate::app::affordance::affordance_trigger_style;
use crate::app::home::package_export::ExportTarget;
use crate::app::project::pending_edit_section::{
    PendingEditBucket, PendingEditList, bucket_section_tint, entries_in,
};
use crate::app::project::project_pane::{ProjectDetailRow, state_label};
use crate::app::project::{ProjectChanges, ProjectDetailContent};
use crate::app::share::{
    ProjectRelationship, ProjectRelationshipPanel, PublishStatus, RosterFacts, ShareUrl,
    relationship_face,
};
use crate::base::{
    DetailPopover, DetailSection, DetailSectionTint, IconMenuTone, InlineButton, InlineButtonTone,
    PopoverPlacement, StudioIcon, StudioIconName,
};

/// Which segment's popover a story wants open on mount. The three popovers
/// are independent, so "open" is a choice, not a flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlSegment {
    /// The device segment — what is running.
    Device,
    /// The project segment — what document is open.
    Project,
    /// The changes segment — what is in flight.
    Changes,
}

/// Everything the header control renders: THE session (core's control
/// projection), the open project's detail content if a project is open at
/// all — a connected board with nothing loaded is a real state, and the
/// project segment says so rather than inventing a name — and the derived
/// relationship the project segment wears as its face.
#[derive(Clone, PartialEq)]
pub struct ChromeSessionControl {
    /// THE session this tab runs.
    pub session: UiChromeSessionControl,
    /// The open project's detail content; `None` with no project open.
    pub project: Option<ProjectDetailContent>,
    /// The user's relationship to the open project (vision D1): the project
    /// segment's face — Example / Private / Shared / Member / Viewing. It
    /// replaces the retired accent "example" pill, which could only say one
    /// of those five things.
    pub relationship: ProjectRelationship,
    /// The live halves of the PROJECT segment's popover — the address, the
    /// roster, the publish ledger, and the fork/copy dispatches. Gathered
    /// by the web shell (only it holds the route, the `CloudSession`, and
    /// the command channel); default-empty everywhere else, which the
    /// panel renders honestly rather than blankly.
    pub project_popover: ProjectPopoverInputs,
    /// Dispatch for the popovers' rows and the Save sibling.
    pub on_action: EventHandler<UiAction>,
    /// Open one segment's popover immediately (stories only).
    pub initially_open: Option<ControlSegment>,
}

/// What the project popover needs that [`ProjectDetailContent`] does not
/// carry: the things only the web shell can answer.
///
/// The bar stays presentational — it forwards this straight through to
/// [`ProjectRelationshipPanel`]. Every field is optional because every one
/// of them can honestly be unknown: a session with no addressable project,
/// a service that has not answered, a publish driver that has run no trip.
#[derive(Clone, Default, PartialEq)]
pub struct ProjectPopoverInputs {
    /// The project's canonical address — the Where section's hero.
    pub url: Option<ShareUrl>,
    /// The Access section's live half, when the service answered.
    pub roster: Option<RosterFacts>,
    /// The auto-publish ledger's last word (the `MineLocal` line).
    pub publish: Option<PublishStatus>,
    /// The fork-family verb's dispatch; `None` renders it disabled with
    /// [`Self::fork_blocked`] as the explanation.
    pub on_fork: Option<EventHandler<()>>,
    /// Why the fork verb cannot act, when it cannot.
    pub fork_blocked: String,
    /// The core view's monotonic fork counter — the panel compares it to
    /// the value it mounted with to announce a fork that happened under
    /// the open popover (G1: the flip alone was too quiet).
    pub fork_generation: u64,
    pub on_copy: Option<EventHandler<()>>,
    pub on_access: Option<EventHandler<Access>>,
    pub on_add: Option<EventHandler<String>>,
    pub on_remove: Option<EventHandler<String>>,
}

/// The E bar: the three-segment shell with Save standing beside it while the
/// project is dirty.
///
/// **Trigger shape.** Each segment is its own [`DetailPopover`] trigger,
/// styled through `trigger_class` / `trigger_open_class` with
/// `layer_keeps_layout` on (the segments hold icon PLUS label, so the
/// top-layer copy must keep the trigger's own box). The shell around them is
/// a plain `div` carrying the border and radius — never a button, so no
/// button ever nests inside another.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SessionProjectControl(control: ChromeSessionControl) -> Element {
    let ChromeSessionControl {
        session,
        project,
        relationship,
        project_popover,
        on_action,
        initially_open,
    } = control;
    let affordance = project.as_ref().map(ProjectDetailContent::affordance);
    let style = affordance.map(affordance_trigger_style);
    // The save moment: the controller publishes Save/Revert on the editor's
    // `header_actions` exactly while persisted edits are pending, so their
    // presence IS the dirty test — the control never recomputes dirtiness.
    // Only Save rides the bar; revert-all retired into the changes popup.
    let save = project
        .as_ref()
        .map(|project| save_and_revert(project.header_actions()).0)
        .unwrap_or_default();
    let changes = project.as_ref().map(ProjectDetailContent::changes);
    let dirty = changes.as_ref().map(|changes| changes.dirty);

    let board = board_suffix(&session);
    let name = session.name.clone();
    // Sim-only while the legacy device system is torn down (device-model
    // M2 on main); hardware wording returns with the rebuilt device model.
    let device_title = "This tab's session — the simulator";
    let device_label = device_label(&session);
    let device_panel_session = session.clone();

    // The device segment: kind glyph, D16 status dot, name, board suffix.
    let device_trigger = rsx! {
        span { class: kind_glyph_class(),
            StudioIcon { name: kind_icon(), size: 12 }
        }
        span { class: dot_class(session.status) }
        // The md fold: below the 900px cut the glyph and the dot carry kind
        // and health alone — the two facts that survive a squeeze.
        span { class: "tw:hidden tw:min-w-0 tw:items-baseline tw:gap-1 tw:@min-[900px]:flex",
            // The unlayered `font: inherit` reset beats tw:font-* on the
            // button itself, so every text utility rides a span.
            span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-[11px] tw:font-semibold tw:text-muted-foreground",
                "{name}"
            }
            if let Some(board) = board.as_ref() {
                span { class: "tw:flex-none tw:text-[10.5px] tw:text-dim-foreground", "· {board}" }
            }
        }
    };

    // The project segment: state glyph, name, relationship face. NEUTRAL —
    // the dirty wash belongs to the changes segment now, so the only color
    // here is the state glyph's own.
    let face = relationship_face(relationship);
    let project_name = project
        .as_ref()
        .map(|project| project.project_name().to_string());
    let project_trigger = rsx! {
        if let (Some(project_name), Some(style)) = (project_name.as_ref(), style) {
            span { class: state_glyph_class(affordance),
                StudioIcon { name: style.icon, size: 13 }
            }
            span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-[11px] tw:font-semibold tw:text-muted-foreground",
                "{project_name}"
            }
            span { class: FACE_CLASS,
                span { class: "tw:flex tw:flex-none tw:items-center tw:text-dim-foreground",
                    StudioIcon { name: face.glyph, size: 11 }
                }
                // The word folds with the device name at the same 900px cut;
                // the glyph is the narrow form (spike §3 vocabulary V3).
                span { class: "tw:hidden tw:flex-none tw:text-[9.5px] tw:font-semibold tw:text-dim-foreground tw:@min-[900px]:inline",
                    "{face.word}"
                }
            }
        } else {
            // Connected with nothing loaded (spike §5): honest-empty, no
            // fake name and no state glyph to read a state off.
            span { class: "tw:text-[11px] tw:italic tw:text-dim-foreground", "no project" }
        }
    };

    // The changes segment: quiet ✓ clean, ✎ plus the amber count dirty.
    let changes_trigger = rsx! {
        if let Some(dirty) = dirty {
            if dirty.persisted == 0 && dirty.failed == 0 {
                span { class: "tw:flex tw:flex-none tw:items-center tw:text-dim-foreground",
                    StudioIcon { name: StudioIconName::StepComplete, size: 12 }
                }
            } else {
                // The glyph is what the 560px fold spends — a count with no
                // glyph still counts; a glyph with no count says nothing.
                span { class: "{changes_glyph_class(dirty)} tw:hidden tw:@min-[560px]:flex",
                    StudioIcon { name: changes_glyph(dirty), size: 12 }
                }
                if dirty.persisted > 0 {
                    span { class: COUNT_PILL_CLASS, "{dirty.persisted}" }
                }
                if dirty.failed > 0 {
                    span { class: FAILED_PILL_CLASS, "{dirty.failed}" }
                }
            }
        }
    };

    let project_label = project_label(project_name.as_deref(), relationship);
    // With nothing open there is no standing to state, so the segment's
    // tooltip says the honest thing rather than a face's sentence.
    let project_title = if project_name.is_some() {
        face.title
    } else {
        "No project open on this session"
    };
    let changes_label = changes_label(dirty.unwrap_or_default());
    let changes_tint = dirty.map(changes_tint).unwrap_or(SegmentTint::None);
    // The project segment is the last one only while no project is open —
    // with one open, changes closes the shell.
    let project_edge = if changes.is_some() {
        SegmentEdge::Middle
    } else {
        SegmentEdge::Last
    };

    rsx! {
        // The shell and Save are SIBLINGS (G1 round-2 ruling, 2026-08-19,
        // upheld): a box that half-inspects and half-acts reads as odd, so
        // every segment inspects and the one direct act stands apart.
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
            div { class: SHELL_CLASS,
                DetailPopover {
                    icon: kind_icon(),
                    label: device_label,
                    title: device_title.to_string(),
                    tone: IconMenuTone::Quiet,
                    placement: PopoverPlacement::BottomStart,
                    trigger: device_trigger,
                    trigger_class: segment_class(SegmentEdge::First, SegmentTint::None, false),
                    trigger_open_class: segment_class(SegmentEdge::First, SegmentTint::None, true),
                    layer_keeps_layout: true,
                    initially_open: initially_open == Some(ControlSegment::Device),
                    SessionDevicePanel { session: device_panel_session }
                }
                span { class: DIVIDER_CLASS }
                DetailPopover {
                    icon: style.map_or(StudioIconName::Info, |style| style.icon),
                    label: project_label,
                    title: project_title.to_string(),
                    tone: style.map_or(IconMenuTone::Quiet, |style| style.tone),
                    placement: PopoverPlacement::BottomStart,
                    trigger: project_trigger,
                    trigger_class: segment_class(project_edge, SegmentTint::None, false),
                    trigger_open_class: segment_class(project_edge, SegmentTint::None, true),
                    layer_keeps_layout: true,
                    initially_open: initially_open == Some(ControlSegment::Project),
                    // THE relationship skeleton (D9): Where / Access /
                    // action row, one shape for all five states. The
                    // settings/identity/stats sections it replaced are not
                    // homeless — they live behind its ⋯ menu's Details row.
                    if let Some(project) = project.clone() {
                        ProjectRelationshipPanel {
                            name: project.project_name().to_string(),
                            relationship,
                            url: project_popover.url.clone(),
                            roster: project_popover.roster.clone(),
                            publish: project_popover.publish.clone(),
                            export: project.library_identity().map(|(uid, slug)| ExportTarget {
                                uid: uid.clone(),
                                slug: slug.clone(),
                            }),
                            unsaved: project.unsaved_count(),
                            // The History tab's rows (D10): core's capped
                            // projection of the open handle's own events,
                            // riding the same gather as everything else
                            // here — no fetch, no second source of truth.
                            history: project.history().clone(),
                            created: project.created().map(str::to_string),
                            fork_generation: project_popover.fork_generation,
                            details: Some(project.clone()),
                            on_fork: project_popover.on_fork,
                            fork_blocked: project_popover.fork_blocked.clone(),
                            on_copy: project_popover.on_copy,
                            on_access: project_popover.on_access,
                            on_add: project_popover.on_add,
                            on_remove: project_popover.on_remove,
                        }
                    } else {
                        DetailSection {
                            p { class: "tw:m-0 tw:text-xs tw:italic tw:leading-snug tw:text-dim-foreground",
                                "No project open on this session."
                            }
                        }
                    }
                }
                if let Some(changes) = changes {
                    span { class: DIVIDER_CLASS }
                    DetailPopover {
                        icon: changes_glyph(changes.dirty),
                        label: changes_label,
                        title: CHANGES_TITLE.to_string(),
                        tone: changes_tone(changes.dirty),
                        placement: PopoverPlacement::BottomStart,
                        trigger: changes_trigger,
                        trigger_class: segment_class(SegmentEdge::Last, changes_tint, false),
                        trigger_open_class: segment_class(SegmentEdge::Last, changes_tint, true),
                        layer_keeps_layout: true,
                        initially_open: initially_open == Some(ControlSegment::Changes),
                        SessionChangesPanel { changes, on_action }
                    }
                }
            }
            // Save: the one DIRECT act (R8-2), appearing exactly while the
            // editor publishes it (the dirty window). ↺ retired into the
            // changes popup — a destructive verb belongs where the thing it
            // destroys is listed.
            if let Some(save) = save.clone() {
                button {
                    class: SAVE_BUTTON_CLASS,
                    r#type: "button",
                    title: "{save.meta().summary}",
                    onclick: move |_| on_action.call(save.clone()),
                    span { class: "tw:text-[11px] tw:font-semibold tw:text-status-warning-foreground",
                        "Save"
                    }
                }
            }
        }
    }
}

/// The DEVICE segment's popover: what is running. The muted device band
/// (kind glyph, name, run word, the mono stat line) over the "this tab is
/// the session" hint — the two facts that answer "what is this tab attached
/// to, and what does leaving cost".
///
/// This is the declared landing zone for the desktop device panel's facts
/// when it retires (D13); nothing new is plumbed for it here.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SessionDevicePanel(session: UiChromeSessionControl) -> Element {
    let run = run_word(&session);
    let stat_line = device_stat_line(&session);
    let hint = session_hint();
    rsx! {
        section { class: "tw:grid tw:gap-0.5 tw:bg-card-muted tw:px-3 tw:py-2",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                span { class: kind_glyph_class(),
                    StudioIcon { name: kind_icon(), size: 13 }
                }
                strong { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-sm tw:text-strong-foreground",
                    "{session.name}"
                }
                // The run word — or, while an operation is in flight, its
                // label: that is also what the nav guard refuses on, so the
                // panel and the refusal toast name the same work.
                span { class: "tw:ml-auto tw:flex-none tw:text-xs {run.class}", "{run.text}" }
            }
            if let Some(stat_line) = stat_line.as_ref() {
                p { class: "tw:m-0 tw:pl-[21px] tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                    "{stat_line}"
                }
            }
        }
        section { class: "tw:border-t tw:border-border-muted tw:px-3 tw:py-1.5",
            p { class: "tw:m-0 tw:text-[10px] tw:italic tw:leading-snug tw:text-dim-foreground",
                "{hint}"
            }
        }
    }
}

/// The CHANGES segment's popover: what is in flight, and the verbs that end
/// it.
///
/// Dirty: the labeled pending edits with their per-entry reverts (the SAME
/// [`PendingEditList`] the detail sections used to host — re-homed, not
/// re-spelled), the failed bucket, the pending facts, an honest receipt of
/// what Save actually does, and the controller's own Save / Revert-all pair.
/// Clean: one quiet line. The popup never claims cloud state it cannot see —
/// it says the edits are live on THIS session and that Save banks them to
/// the library, which is exactly what it knows.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SessionChangesPanel(changes: ProjectChanges, on_action: EventHandler<UiAction>) -> Element {
    let ProjectChanges {
        affordance,
        dirty,
        overlay_revision,
        edits_in_flight,
        pending_edits,
        header_actions,
    } = changes;
    let unsaved_entries = entries_in(&pending_edits, PendingEditBucket::Persisted);
    let failed_entries = entries_in(&pending_edits, PendingEditBucket::Failed);
    let (save, revert) = save_and_revert(&header_actions);
    let anything_pending = dirty.persisted > 0
        || dirty.failed > 0
        || !unsaved_entries.is_empty()
        || !failed_entries.is_empty();

    if !anything_pending {
        return rsx! {
            DetailSection {
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                    "All saved \u{2014} nothing pending."
                }
            }
        };
    }

    rsx! {
        if dirty.persisted > 0 || !unsaved_entries.is_empty() {
            DetailSection {
                title: "Unsaved (persisted)",
                meta: dirty.persisted.to_string(),
                tint: bucket_section_tint(PendingEditBucket::Persisted, dirty.persisted),
                PendingEditList { entries: unsaved_entries, on_action }
            }
        }
        if dirty.failed > 0 || !failed_entries.is_empty() {
            DetailSection {
                title: "Failed edits",
                meta: dirty.failed.to_string(),
                tint: bucket_section_tint(PendingEditBucket::Failed, dirty.failed),
                PendingEditList { entries: failed_entries, on_action }
            }
        }
        DetailSection { title: "Pending edits",
            ProjectDetailRow { label: "State", value: state_label(affordance).to_string() }
            ProjectDetailRow { label: "Overlay revision", value: overlay_revision.to_string() }
            if edits_in_flight > 0 {
                ProjectDetailRow { label: "Awaiting ack", value: edits_in_flight.to_string() }
            }
        }
        if save.is_some() || revert.is_some() {
            DetailSection { tint: DetailSectionTint::Warning,
                p { class: "tw:m-0 tw:text-[11px] tw:leading-snug tw:text-dim-foreground",
                    "Already live in this session \u{2014} Save banks everything to your library."
                }
                div { class: "tw:flex tw:items-center tw:gap-2 tw:pt-1.5",
                    if let Some(save) = save.clone() {
                        button {
                            class: SAVE_BUTTON_CLASS,
                            r#type: "button",
                            title: "{save.meta().summary}",
                            onclick: move |_| on_action.call(save.clone()),
                            span { class: "tw:text-[11px] tw:font-semibold tw:text-status-warning-foreground",
                                "Save"
                            }
                        }
                    }
                    if let Some(revert) = revert.clone() {
                        InlineButton {
                            label: "Revert all".to_string(),
                            title: revert.meta().summary.clone(),
                            text: "Revert all".to_string(),
                            icon: StudioIconName::Revert,
                            tone: InlineButtonTone::Warning,
                            class: "tw:ml-auto".to_string(),
                            on_press: move |_| on_action.call(revert.clone()),
                        }
                    }
                }
            }
        }
    }
}

/// The session's kind glyph: the violet sim mark (the bound-family
/// convention the sim card wears). The transport icons for hardware
/// return with the rebuilt device model.
fn kind_icon() -> StudioIconName {
    StudioIconName::Simulator
}

fn kind_glyph_class() -> &'static str {
    "tw:flex tw:flex-none tw:items-center tw:text-status-bound-foreground"
}

/// The D16 status dot, the same three-value vocabulary the strip collapses
/// to: accent run / amber attention / hollow connected-empty.
fn dot_class(status: UiChromeSessionStatus) -> &'static str {
    match status {
        UiChromeSessionStatus::Run => "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-accent",
        UiChromeSessionStatus::Attention => {
            "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:bg-status-attention-foreground"
        }
        UiChromeSessionStatus::Empty => {
            "tw:h-1.5 tw:w-1.5 tw:flex-none tw:rounded-full tw:border tw:border-border-strong tw:bg-transparent"
        }
    }
}

/// The state glyph's color: the affordance family's foreground, quiet while
/// there is nothing to announce. This is the ONLY color the project segment
/// carries — the dirty wash moved to the changes segment, so a project is
/// never announced twice in one bar.
fn state_glyph_class(affordance: Option<UiAffordance>) -> &'static str {
    match affordance {
        Some(UiAffordance::Unsaved) => {
            "tw:flex tw:flex-none tw:items-center tw:text-status-warning-foreground"
        }
        Some(UiAffordance::Error) => {
            "tw:flex tw:flex-none tw:items-center tw:text-status-error-foreground"
        }
        Some(UiAffordance::Busy) => {
            "tw:flex tw:flex-none tw:items-center tw:text-status-working-foreground"
        }
        Some(UiAffordance::Debug) => "tw:flex tw:flex-none tw:items-center lp-debug-indicator",
        _ => "tw:flex tw:flex-none tw:items-center tw:text-dim-foreground",
    }
}

/// The changes segment's glyph: the pencil while work is waiting to be
/// saved, the alert ring when the only news is failure, the check when
/// there is nothing pending at all.
fn changes_glyph(dirty: DirtySummary) -> StudioIconName {
    if dirty.persisted > 0 {
        StudioIconName::Edited
    } else if dirty.failed > 0 {
        StudioIconName::StatusError
    } else {
        StudioIconName::StepComplete
    }
}

fn changes_glyph_class(dirty: DirtySummary) -> &'static str {
    if dirty.persisted > 0 {
        "tw:flex tw:flex-none tw:items-center tw:text-status-warning-foreground"
    } else if dirty.failed > 0 {
        "tw:flex tw:flex-none tw:items-center tw:text-status-error-foreground"
    } else {
        "tw:flex tw:flex-none tw:items-center tw:text-dim-foreground"
    }
}

/// The popover chrome tone for the changes card — failure outranks unsaved,
/// and a clean project is not an announcement.
fn changes_tone(dirty: DirtySummary) -> IconMenuTone {
    if dirty.failed > 0 {
        IconMenuTone::Error
    } else if dirty.persisted > 0 {
        IconMenuTone::Warning
    } else {
        IconMenuTone::Quiet
    }
}

/// The changes segment's own wash. Failure outranks unsaved: a red bed is
/// the louder statement and the one that must not be masked.
fn changes_tint(dirty: DirtySummary) -> SegmentTint {
    if dirty.failed > 0 {
        SegmentTint::Error
    } else if dirty.persisted > 0 {
        SegmentTint::Warning
    } else {
        SegmentTint::None
    }
}

/// The sim's board suffix (`· ESP32-C6`, ruling 8.1: the sim names the board
/// it simulates). Every session is a sim while the legacy device system is
/// torn down; hardware (which never wears a suffix — a board's own name IS
/// the device name) returns with the rebuilt device model.
fn board_suffix(session: &UiChromeSessionControl) -> Option<String> {
    session.board.clone().filter(|board| !board.is_empty())
}

/// Save and Revert, picked out of the editor's `header_actions` by their
/// icon tokens. Every home dispatches the SAME actions the pane header and
/// the Tree row dispatch — one save verb in the app.
fn save_and_revert(actions: &[UiPaneAction]) -> (Option<UiAction>, Option<UiAction>) {
    let pick = |icon: &str| {
        actions
            .iter()
            .find(|action| action.icon == icon)
            .map(|action| action.action.clone())
    };
    (pick("save"), pick("revert"))
}

/// The device zone's mono stat line: what the sim is simulating (8.1) ahead
/// of core's own stat line ("60 fps · USB"). `None` when the session has
/// published nothing honest to say.
fn device_stat_line(session: &UiChromeSessionControl) -> Option<String> {
    let simulating = board_suffix(session).map(|board| format!("simulating {board}"));
    match (simulating, session.stat_line.clone()) {
        (Some(simulating), Some(stats)) => Some(format!("{simulating} · {stats}")),
        (Some(simulating), None) => Some(simulating),
        (None, stats) => stats,
    }
}

/// The device zone's right-aligned run word, with the tone it reads in.
struct RunWord {
    text: String,
    class: &'static str,
}

/// The run word is the three-dot vocabulary, spelled out.
///
/// ⚠️ The in-flight OPERATION label used to outrank it (a flash/deploy is
/// what a nav-away is refused for); it went with M2 of the device-model
/// rebuild along with the ops that set it.
fn run_word(session: &UiChromeSessionControl) -> RunWord {
    match session.status {
        UiChromeSessionStatus::Run => RunWord {
            text: "running".to_string(),
            class: "tw:text-status-good-foreground",
        },
        UiChromeSessionStatus::Attention => RunWord {
            text: "needs attention".to_string(),
            class: "tw:text-status-attention-foreground",
        },
        UiChromeSessionStatus::Empty => RunWord {
            text: "idle".to_string(),
            class: "tw:text-dim-foreground",
        },
    }
}

/// The device panel's footer line: the single-session policy said plainly,
/// because the consequence of navigating away is otherwise invisible. The
/// document is durable (the draft overlay persists) — the SESSION is what
/// ends.
fn session_hint() -> &'static str {
    "This tab is the session — close it or navigate away to stop the simulator."
}

/// The device segment's accessible name: the device and, for a sim, the
/// board it simulates.
fn device_label(session: &UiChromeSessionControl) -> String {
    match board_suffix(session) {
        Some(board) => format!("{} · {board}", session.name),
        None => session.name.clone(),
    }
}

/// The project segment's accessible name: the document and your standing on
/// it — the two things its face shows.
fn project_label(project_name: Option<&str>, relationship: ProjectRelationship) -> String {
    match project_name {
        Some(name) => format!("{name} — {}", relationship_face(relationship).word),
        None => "No project open".to_string(),
    }
}

/// The changes segment's accessible name: a screen reader gets one string
/// for the whole segment, so it has to carry both counts.
fn changes_label(dirty: DirtySummary) -> String {
    match (dirty.persisted, dirty.failed) {
        (0, 0) => "Changes — all saved".to_string(),
        (persisted, 0) => format!("Changes — {persisted} unsaved"),
        (0, failed) => format!("Changes — {failed} failed"),
        (persisted, failed) => format!("Changes — {persisted} unsaved, {failed} failed"),
    }
}

/// Where a segment sits in the shell — it owns its own corner rounding, so
/// the top-layer copy of an open trigger keeps the shape the in-flow
/// segment had inside the shell's clip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SegmentEdge {
    First,
    Middle,
    Last,
}

/// A segment's status wash. Only the changes segment ever wears one today
/// (D8: changes own the dirty concept).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SegmentTint {
    None,
    Warning,
    Error,
}

/// One segment button. `open` selects the settled look the popover's merged
/// outline grows out of; otherwise the segment washes on hover alone.
///
/// No preflight in this build, so a `<button>` names its border AND its
/// background explicitly — a bare one paints UA `buttonface` over the
/// shell.
fn segment_class(edge: SegmentEdge, tint: SegmentTint, open: bool) -> String {
    let radius = match edge {
        SegmentEdge::First => " tw:rounded-l-[6px]",
        SegmentEdge::Middle => "",
        SegmentEdge::Last => " tw:rounded-r-[6px]",
    };
    let background = match (tint, open) {
        (SegmentTint::None, false) => " tw:bg-transparent tw:hover:bg-card-raised",
        (SegmentTint::None, true) => " tw:bg-card-raised",
        (SegmentTint::Warning, false) => {
            " tw:bg-status-warning-bg tw:hover:bg-status-warning-border"
        }
        (SegmentTint::Warning, true) => " tw:bg-status-warning-border",
        (SegmentTint::Error, false) => " tw:bg-status-error-bg tw:hover:bg-status-error-border",
        (SegmentTint::Error, true) => " tw:bg-status-error-border",
    };
    format!("{SEGMENT_CLASS}{radius}{background}")
}

/// The shell at rest: ONE rounded bordered container with internal
/// hairlines, not a row of chips. A plain `div` — the segments inside it are
/// the buttons — and `p-0` because the segments own the padding.
const SHELL_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:items-stretch tw:overflow-hidden tw:rounded-[7px] tw:border tw:border-border-strong tw:bg-card-subtle tw:p-0 tw:text-left";
/// One segment's shared geometry. Below the phone cut the padding tightens —
/// on a 375px bar every horizontal pixel the chrome keeps is a pixel the
/// project name loses.
const SEGMENT_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-1.5 tw:border-0 tw:px-2.5 tw:py-1 tw:text-left tw:transition-colors tw:@max-[560px]:px-1.5";
/// The internal hairline between segments — a drawn divider rather than a
/// per-segment border, so no segment has to fight the UA button border.
const DIVIDER_CLASS: &str = "tw:w-px tw:flex-none tw:self-stretch tw:bg-border-subtle";
/// The relationship face: glyph plus word in a quiet outlined chip, neutral
/// family (D12 — the face states identity, not status, so it borrows no
/// status color; the outline is what separates it from the project NAME
/// beside it, since both are dim text).
const FACE_CLASS: &str = "tw:flex tw:flex-none tw:items-center tw:gap-1 tw:rounded-full tw:border tw:border-border-subtle tw:px-1.5 tw:py-px";
/// The standalone amber Save button (G1 round-2: apart from the shell — the
/// inspect surface and the act surface are different things). No preflight,
/// so border and background are named explicitly.
const SAVE_BUTTON_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:rounded-md tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2.5 tw:py-[3px] tw:transition-colors tw:hover:border-status-warning-foreground tw:@max-[560px]:px-2";
/// The unsaved count, the header chip's pill verbatim (D8): mono, amber,
/// pill-shaped — the same badge the pane's own affordances wear. It rides
/// the CHANGES segment now; the retired accent "example" pill's job went to
/// the relationship face.
const COUNT_PILL_CLASS: &str = "tw:flex-none tw:rounded-full tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-1.5 tw:font-mono tw:text-[9.5px] tw:font-semibold tw:text-status-warning-foreground";
/// The failed count beside it — the same pill in the error family, so a
/// failed-only project still counts something in the bar.
const FAILED_PILL_CLASS: &str = "tw:flex-none tw:rounded-full tw:border tw:border-status-error-border tw:bg-status-error-bg tw:px-1.5 tw:font-mono tw:text-[9.5px] tw:font-semibold tw:text-status-error-foreground";
/// The changes segment's tooltip: what the popup is about, in one line.
const CHANGES_TITLE: &str = "Changes — live on this session, banked to your library on Save";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ControllerId, ProjectController, ProjectOp};

    use super::*;

    fn session(board: Option<&str>) -> UiChromeSessionControl {
        UiChromeSessionControl {
            key: "runtime-1".to_string(),
            name: "Sim".to_string(),
            board: board.map(str::to_string),
            status: UiChromeSessionStatus::Run,
            stat_line: None,
        }
    }

    fn pane_action(icon: &str, op: ProjectOp) -> UiPaneAction {
        UiPaneAction::new(
            icon,
            UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op),
        )
    }

    fn dirty(persisted: usize, failed: usize) -> DirtySummary {
        DirtySummary { persisted, failed }
    }

    /// Save (bar) and Revert-all (changes popup) are the controller's own
    /// header actions — not a second pair minted here — so the bar, the
    /// popup, and the pane header can never save different things.
    #[test]
    fn save_and_revert_come_from_the_editors_header_actions() {
        let actions = vec![
            pane_action("save", ProjectOp::SaveOverlay),
            pane_action("revert", ProjectOp::RevertAllEdits),
        ];

        let (save, revert) = save_and_revert(&actions);

        assert_eq!(save, Some(actions[0].action.clone()));
        assert_eq!(revert, Some(actions[1].action.clone()));
    }

    /// A clean project publishes no header actions, which is exactly the
    /// dirty test: no actions, no Save sibling and no popup verbs.
    #[test]
    fn a_clean_project_offers_no_save_verbs() {
        assert_eq!(save_and_revert(&[]), (None, None));
    }

    /// Ruling 8.1: the sim names the board it simulates (hardware, which
    /// never wears one, returns with the rebuilt device model).
    #[test]
    fn the_sim_wears_its_board_as_a_suffix() {
        assert_eq!(
            board_suffix(&session(Some("ESP32-C6"))),
            Some("ESP32-C6".to_string())
        );
        assert_eq!(board_suffix(&session(None)), None);
    }

    #[test]
    fn the_stat_line_leads_with_what_the_sim_simulates() {
        let mut sim = session(Some("ESP32-C6"));
        sim.stat_line = Some("60 fps · 217 lamps".to_string());
        assert_eq!(
            device_stat_line(&sim),
            Some("simulating ESP32-C6 · 60 fps · 217 lamps".to_string())
        );

        // Nothing published, nothing said — no empty mono line.
        assert_eq!(device_stat_line(&session(None)), None);
    }

    #[test]
    fn the_run_word_spells_the_status_dot() {
        assert_eq!(run_word(&session(None)).text, "running");

        let mut empty = session(None);
        empty.status = UiChromeSessionStatus::Empty;
        assert_eq!(run_word(&empty).text, "idle");
    }

    /// Three segments, three accessible names: each one answers its OWN
    /// question rather than the whole bar's.
    #[test]
    fn each_segment_names_only_its_own_question() {
        assert_eq!(device_label(&session(Some("ESP32-C6"))), "Sim · ESP32-C6");
        assert_eq!(device_label(&session(None)), "Sim");

        assert_eq!(
            project_label(Some("small dome"), ProjectRelationship::Example),
            "small dome — Example"
        );
        // The honest-empty edge: no invented name anywhere.
        assert_eq!(
            project_label(None, ProjectRelationship::MineLocal),
            "No project open"
        );

        assert_eq!(changes_label(dirty(0, 0)), "Changes — all saved");
        assert_eq!(changes_label(dirty(3, 0)), "Changes — 3 unsaved");
        assert_eq!(changes_label(dirty(0, 2)), "Changes — 2 failed");
        assert_eq!(changes_label(dirty(3, 2)), "Changes — 3 unsaved, 2 failed");
    }

    /// The dirty wash lives on the CHANGES segment now, and failure
    /// outranks unsaved there — a red bed must never be masked by amber.
    #[test]
    fn the_changes_segment_wears_failure_over_unsaved() {
        assert_eq!(changes_tint(dirty(0, 0)), SegmentTint::None);
        assert_eq!(changes_tint(dirty(3, 0)), SegmentTint::Warning);
        assert_eq!(changes_tint(dirty(0, 1)), SegmentTint::Error);
        assert_eq!(changes_tint(dirty(3, 1)), SegmentTint::Error);

        assert_eq!(changes_tone(dirty(0, 0)), IconMenuTone::Quiet);
        assert_eq!(changes_tone(dirty(3, 0)), IconMenuTone::Warning);
        assert_eq!(changes_tone(dirty(3, 1)), IconMenuTone::Error);
    }

    /// Clean is a quiet ✓; the pencil only appears once Save has something
    /// to write, and the alert ring is the failed-only edge.
    #[test]
    fn the_changes_glyph_says_which_state_it_is() {
        assert_eq!(changes_glyph(dirty(0, 0)), StudioIconName::StepComplete);
        assert_eq!(changes_glyph(dirty(2, 0)), StudioIconName::Edited);
        assert_eq!(changes_glyph(dirty(0, 2)), StudioIconName::StatusError);
    }

    /// Every relationship has a face, and the words are the five ruled ones
    /// (D12) — the face is what replaced the accent "example" pill.
    #[test]
    fn every_relationship_has_one_of_the_five_faces() {
        let words = [
            ProjectRelationship::Example,
            ProjectRelationship::MineLocal,
            ProjectRelationship::MinePublished,
            ProjectRelationship::MemberOfSomeoneElses,
            ProjectRelationship::ViewingSomeoneElses,
        ]
        .map(|relationship| relationship_face(relationship).word);
        assert_eq!(words, ["Example", "Private", "Shared", "Member", "Viewing"]);
    }

    /// The first segment rounds the shell's left corners and the last its
    /// right ones, so an open trigger's top-layer copy keeps the shape it
    /// had inside the shell's clip.
    #[test]
    fn only_the_end_segments_round_their_corners() {
        let first = segment_class(SegmentEdge::First, SegmentTint::None, false);
        let middle = segment_class(SegmentEdge::Middle, SegmentTint::None, false);
        let last = segment_class(SegmentEdge::Last, SegmentTint::None, false);

        assert!(first.contains("tw:rounded-l-[6px]"));
        assert!(!middle.contains("tw:rounded-"));
        assert!(last.contains("tw:rounded-r-[6px]"));
        // No preflight: every segment names a background, tint or not.
        for class in [&first, &middle, &last] {
            assert!(class.contains("tw:bg-"));
            assert!(class.contains("tw:border-0"));
        }
    }
}
