//! The header **session·project control**: one shell, three segments, ONE
//! panel — `[ device | project | changes ]`, segments as tabs.
//!
//! `❖ Sim · ESP32-C6 │ ◈ small dome · 🔒 Private │ ✎ ② [Save]` — one bordered
//! shell with internal hairlines, each segment a tab into the shared panel
//! anchored on the whole shell, and Save standing beside the shell as the
//! one direct act (relationship-control D7/D8 as amended by D15, spike
//! §2-E).
//!
//! **Three questions, one bar.** What is running · what document is open ·
//! what is in flight. Changes earn their own segment because pending edits
//! span BOTH stores — they are already live on the session, and Save banks
//! them to the library — so they belong to neither the device box nor the
//! document box alone.
//!
//! **One panel, segments as tabs (D15, amending D7/D9).** Round 1 gave
//! each segment its own popover, and moving between them meant a full
//! close-reopen animation for what reads as switching tabs on one control.
//! Now the segments drive a LIFTED open-state: clicking a segment opens
//! the shared panel at its section, clicking another segment while open
//! switches the content IN PLACE (the popover's retarget guard and panel
//! ResizeObserver absorb the resize), and clicking the open segment closes
//! it. The merged outline anchors on the SHELL, so the panel visibly hangs
//! off the whole bar — the bar IS the tab row, and no inner tab row is
//! needed anywhere in the panel.
//!
//! **Segments are plain buttons now.** They are not popover triggers: the
//! popover's own trigger is a hidden button (the segments own opening),
//! and while the panel is open the segments' interactive copies render in
//! the top layer as the popover's ANCHOR VISUAL — anchored-mode visuals
//! host real controls, so the tab clicks keep working above the merged
//! outline. Save stays a sibling outside the shell and renders once.
//!
//! **One ungated mount** (Q10 lesson, #426): the control is never wrapped in
//! a `tw:@min-*` container. A top-layer popover cannot answer a container
//! query. The FOLDS ride the pieces instead — the
//! device name and the relationship WORD hide below the 900px cut (their
//! glyphs stay), the changes segment goes count-only and the padding tightens
//! below 560px.
//!
//! **Three sections, three concepts.** The device section states what is
//! running; the changes section lists what is in flight AND carries the
//! document's banked history below it (D14 — changes and history are one
//! temporal axis); the project section is [`ProjectRelationshipPanel`] —
//! the identity skeleton (Where / Access / action row) rendered for all
//! five [`ProjectRelationship`] states. The detail sections that used to
//! hang here are not homeless: they sit behind that panel's ⋯ menu.
//!
//! The control is presentational: everything it renders comes from
//! [`UiChromeSessionControl`] (core's projection of THE session), the open
//! project's [`ProjectDetailContent`] — the same value the pane's [i]
//! renders, so the chrome and the pane can never disagree about a project —
//! the derived [`ProjectRelationship`] the caller computes, and
//! [`ProjectPopoverInputs`] (the address, roster, publish ledger, and
//! dispatches only the web shell can gather).

use std::sync::atomic::{AtomicUsize, Ordering};

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
use crate::app::project::{HistoryList, ProjectChanges, ProjectDetailContent};
use crate::app::share::{
    ProjectRelationship, ProjectRelationshipPanel, PublishStatus, RosterFacts, ShareUrl,
    relationship_face,
};
use crate::base::{
    DetailPopover, DetailSection, DetailSectionTint, IconMenuTone, InlineButton, InlineButtonTone,
    PopoverPlacement, StudioIcon, StudioIconName,
};

static NEXT_SESSION_CONTROL_ID: AtomicUsize = AtomicUsize::new(1);

/// One section of the shared panel — and therefore one segment of the
/// shell, since the segments ARE the panel's tabs (D15). Stories name a
/// section here to mount with the panel open on it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ControlSegment {
    /// The device segment — what is running.
    Device,
    /// The project segment — what document is open.
    Project,
    /// The changes segment — what is in flight, over the banked history.
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
    /// Mount with the shared panel open on this section (stories only).
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
/// **Segment shape (D15).** Each segment is a plain button driving the
/// LIFTED open-state; the shared panel is ONE [`DetailPopover`] whose
/// hidden trigger cedes opening to the segments, whose merged outline
/// anchors on the shell (`anchor_id`), and whose top-layer anchor visual
/// re-renders the same segments interactively so the tabs keep working
/// while the panel is open. The shell around them is a plain `div`
/// carrying the border and radius — never a button, so no button ever
/// nests inside another.
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
    // The D15 lifted state: which section the shared panel shows, and
    // whether it is open. The segments write both; the popover reads and
    // writes the open half through its controlled `open_signal` (the
    // backdrop closes it, the segments toggle it).
    let section = use_signal(|| initially_open.unwrap_or(ControlSegment::Device));
    let panel_open = use_signal(|| initially_open.is_some());
    // The anchor id: the merged outline welds the panel to the WHOLE
    // shell — the bar is the tab row (D15), so the panel hangs off the
    // bar, not off one segment.
    let shell_id = use_hook(|| {
        let id = NEXT_SESSION_CONTROL_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-session-control-shell-{id}")
    });

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

    // The segments render TWICE while the panel is open: once in flow
    // (under the outline fill) and once as the popover's top-layer anchor
    // visual. Same builder, same signals, so both copies show the same
    // selected tab and both sets of clicks drive the same state.
    let segments = segments_rsx(
        &session,
        project.as_ref(),
        relationship,
        dirty,
        section,
        panel_open,
    );
    let visual_segments = segments_rsx(
        &session,
        project.as_ref(),
        relationship,
        dirty,
        section,
        panel_open,
    );

    // The panel chrome (tone stroke) follows the OPEN section, so the one
    // panel still speaks each section's status language.
    let tone = match section() {
        ControlSegment::Device => IconMenuTone::Quiet,
        ControlSegment::Project => style.map_or(IconMenuTone::Quiet, |style| style.tone),
        ControlSegment::Changes => dirty.map(changes_tone).unwrap_or(IconMenuTone::Quiet),
    };
    let device_panel_session = session.clone();
    let panel_relationship = relationship;
    let panel_changes = changes.clone();
    let panel_project = project.clone();

    rsx! {
        // The shell and Save are SIBLINGS (G1 round-2 ruling, 2026-08-19,
        // upheld): a box that half-inspects and half-acts reads as odd, so
        // every segment inspects and the one direct act stands apart.
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
            div { id: "{shell_id}", class: SHELL_CLASS,
                {segments}
                // The ONE panel (D15). Its own trigger is hidden — the
                // segments above own opening — and it contributes no
                // width to the shell. The top layer escapes the shell's
                // overflow clip, so hosting it inside the shell is safe.
                DetailPopover {
                    icon: StudioIconName::Info,
                    label: "Session and project details".to_string(),
                    title: "Session and project details".to_string(),
                    tone,
                    placement: PopoverPlacement::BottomStart,
                    trigger: rsx! {},
                    trigger_class: HIDDEN_TRIGGER_CLASS.to_string(),
                    trigger_open_class: HIDDEN_TRIGGER_CLASS.to_string(),
                    open_signal: Some(panel_open),
                    initially_open: initially_open.is_some(),
                    anchor_id: Some(shell_id.clone()),
                    anchor_visual: rsx! {
                        // The interactive top-layer copy of the shell's
                        // interior: no border and no background — the
                        // merged outline owns the chrome while open.
                        div { class: ANCHOR_VISUAL_CLASS, {visual_segments} }
                    },
                    match section() {
                        ControlSegment::Device => rsx! {
                            SessionDevicePanel { session: device_panel_session.clone() }
                        },
                        ControlSegment::Project => rsx! {
                            // THE relationship skeleton (D9, amended by D14:
                            // identity axis only — history lives with
                            // changes now). The settings/identity/stats
                            // sections it replaced are not homeless — they
                            // live behind its ⋯ menu's Details row.
                            if let Some(project) = panel_project.clone() {
                                ProjectRelationshipPanel {
                                    name: project.project_name().to_string(),
                                    relationship: panel_relationship,
                                    url: project_popover.url.clone(),
                                    roster: project_popover.roster.clone(),
                                    publish: project_popover.publish.clone(),
                                    export: project.library_identity().map(|(uid, slug)| ExportTarget {
                                        uid: uid.clone(),
                                        slug: slug.clone(),
                                    }),
                                    unsaved: project.unsaved_count(),
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
                        },
                        ControlSegment::Changes => rsx! {
                            if let Some(changes) = panel_changes.clone() {
                                SessionChangesPanel {
                                    changes,
                                    relationship: panel_relationship,
                                    on_action,
                                }
                            } else {
                                DetailSection {
                                    p { class: "tw:m-0 tw:text-xs tw:italic tw:leading-snug tw:text-dim-foreground",
                                        "No project open on this session."
                                    }
                                }
                            }
                        },
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

/// The three segment buttons with their dividers — the shell's interior,
/// built once for the in-flow shell and once for the popover's top-layer
/// anchor visual. Plain buttons: each click runs [`next_panel_state`]
/// over the lifted signals, which is the whole tab protocol.
fn segments_rsx(
    session: &UiChromeSessionControl,
    project: Option<&ProjectDetailContent>,
    relationship: ProjectRelationship,
    dirty: Option<DirtySummary>,
    mut section: Signal<ControlSegment>,
    mut panel_open: Signal<bool>,
) -> Element {
    let mut press = move |segment: ControlSegment| {
        let (open, next) = next_panel_state(panel_open(), section(), segment);
        // Section first: when the open flip lands, the panel content is
        // already the clicked section — no one-frame flash of the old tab.
        section.set(next);
        panel_open.set(open);
    };
    let segment_open = |segment: ControlSegment| panel_open() && section() == segment;

    let affordance = project.map(ProjectDetailContent::affordance);
    let style = affordance.map(affordance_trigger_style);
    let board = board_suffix(session);
    let name = session.name.clone();
    // Sim-only while the legacy device system is torn down (device-model
    // M2 on main); hardware wording returns with the rebuilt device model.
    let device_title = "This tab's session — the simulator";
    let device_label = device_label(session);
    let status = session.status;

    let face = relationship_face(relationship);
    let project_name = project.map(|project| project.project_name().to_string());
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
    let has_changes = dirty.is_some();
    // The project segment is the last one only while no project is open —
    // with one open, changes closes the shell.
    let project_edge = if has_changes {
        SegmentEdge::Middle
    } else {
        SegmentEdge::Last
    };

    rsx! {
        // The device segment: kind glyph, D16 status dot, name, board suffix.
        button {
            class: segment_class(SegmentEdge::First, SegmentTint::None, segment_open(ControlSegment::Device)),
            r#type: "button",
            aria_label: "{device_label}",
            aria_expanded: "{segment_open(ControlSegment::Device)}",
            aria_haspopup: "dialog",
            title: "{device_title}",
            onclick: move |event| {
                event.stop_propagation();
                press(ControlSegment::Device);
            },
            span { class: kind_glyph_class(),
                StudioIcon { name: kind_icon(), size: 12 }
            }
            span { class: dot_class(status) }
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
        }
        span { class: DIVIDER_CLASS }
        // The project segment: state glyph, name, relationship face. NEUTRAL —
        // the dirty wash belongs to the changes segment now, so the only color
        // here is the state glyph's own.
        button {
            class: segment_class(project_edge, SegmentTint::None, segment_open(ControlSegment::Project)),
            r#type: "button",
            aria_label: "{project_label}",
            aria_expanded: "{segment_open(ControlSegment::Project)}",
            aria_haspopup: "dialog",
            title: "{project_title}",
            onclick: move |event| {
                event.stop_propagation();
                press(ControlSegment::Project);
            },
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
        }
        // The changes segment: quiet ✓ clean, ✎ plus the amber count dirty.
        if let Some(dirty) = dirty {
            span { class: DIVIDER_CLASS }
            button {
                class: segment_class(SegmentEdge::Last, changes_tint, segment_open(ControlSegment::Changes)),
                r#type: "button",
                aria_label: "{changes_label}",
                aria_expanded: "{segment_open(ControlSegment::Changes)}",
                aria_haspopup: "dialog",
                title: CHANGES_TITLE,
                onclick: move |event| {
                    event.stop_propagation();
                    press(ControlSegment::Changes);
                },
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
        }
    }
}

/// The tab protocol (D15), pure: clicking the OPEN section closes the
/// panel; clicking anything else opens it there — switching sections while
/// open is therefore an in-place content swap, never a close-reopen.
fn next_panel_state(
    open: bool,
    section: ControlSegment,
    clicked: ControlSegment,
) -> (bool, ControlSegment) {
    if open && section == clicked {
        (false, section)
    } else {
        (true, clicked)
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

/// The CHANGES section: what is in flight, the verbs that end it, and the
/// banked history below it (D14).
///
/// Dirty: the labeled pending edits with their per-entry reverts (the SAME
/// [`PendingEditList`] the detail sections used to host — re-homed, not
/// re-spelled), the failed bucket, the pending facts, an honest receipt of
/// what Save actually does — naming the version it will bank when the
/// projection knows it — and the controller's own Save / Revert-all pair.
/// Clean: one quiet line. The popup never claims cloud state it cannot see —
/// it says the edits are live on THIS session and that Save banks them to
/// the library, which is exactly what it knows.
///
/// **The banked timeline rides underneath** (D14, amending D10): changes
/// and history are one temporal axis — the receipt's "Save banks v13" and
/// the timeline's "v12 saved" are the same ledger read from opposite ends
/// — so the pending block sits on top and the document's history sits
/// below it, in one panel, with no tab between them. No synthetic
/// "editing" row in the timeline: the pending block IS the in-flight
/// statement.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SessionChangesPanel(
    changes: ProjectChanges,
    /// Selects the history empty-line honesty rule (an Example's seeded
    /// rows are bookkeeping, not the person's history).
    relationship: ProjectRelationship,
    /// Fixed clock for the history rows in stories; `None` uses the
    /// platform clock.
    #[props(default)]
    now_secs: Option<f64>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let ProjectChanges {
        affordance,
        dirty,
        overlay_revision,
        edits_in_flight,
        pending_edits,
        header_actions,
        history,
    } = changes;
    let unsaved_entries = entries_in(&pending_edits, PendingEditBucket::Persisted);
    let failed_entries = entries_in(&pending_edits, PendingEditBucket::Failed);
    let (save, revert) = save_and_revert(&header_actions);
    let receipt = save_receipt_line(history.next_version);
    let anything_pending = dirty.persisted > 0
        || dirty.failed > 0
        || !unsaved_entries.is_empty()
        || !failed_entries.is_empty();
    let timeline = rsx! {
        DetailSection { title: "History",
            HistoryList { relationship, history: history.clone(), now_secs }
        }
    };

    if !anything_pending {
        return rsx! {
            DetailSection {
                p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-muted-foreground",
                    "All saved \u{2014} nothing pending."
                }
            }
            {timeline}
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
                    "{receipt}"
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
        {timeline}
    }
}

/// The receipt's one sentence: what Save actually does, naming the version
/// it will bank when the projection knows the next one — the same number
/// the timeline's newest row will wear after the save, which is the D14
/// point: two ends of one ledger.
fn save_receipt_line(next_version: Option<u64>) -> String {
    match next_version {
        Some(next) => {
            format!("Already live in this session \u{2014} Save banks v{next} to your library.")
        }
        None => "Already live in this session \u{2014} Save banks everything to your library."
            .to_string(),
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

/// One segment button. `open` selects the settled ACTIVE-TAB look the
/// popover's merged outline grows out of (D15: the bar is the tab row);
/// otherwise the segment washes on hover alone.
///
/// The open, untinted segment wears the you-are-here spectrum underline —
/// the same mark the site chrome's nav tabs wear, because "which section
/// is open" is the same question "which page am I on" is (#481 selection
/// grammar: nav you-are-here = spectrum line on the nav axis's edge). A
/// status-TINTED open segment keeps its semantic bed alone: semantics
/// beat decoration, the same rule the popover chrome follows.
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
        (SegmentTint::None, true) => {
            " tw:bg-card-raised tw:relative tw:after:absolute tw:after:inset-x-2 tw:after:bottom-0 tw:after:h-0.5 tw:after:rounded-full tw:after:bg-[linear-gradient(90deg,var(--studio-spectrum))] tw:after:content-['']"
        }
        (SegmentTint::Warning, false) => {
            " tw:bg-status-warning-bg tw:hover:bg-status-warning-border"
        }
        (SegmentTint::Warning, true) => " tw:bg-status-warning-border",
        (SegmentTint::Error, false) => " tw:bg-status-error-bg tw:hover:bg-status-error-border",
        (SegmentTint::Error, true) => " tw:bg-status-error-border",
    };
    format!("{SEGMENT_CLASS}{radius}{background}")
}

/// The shared panel's OWN trigger button, hidden: the segments own opening
/// (D15), so the popover's mandatory trigger contributes nothing — no box,
/// no focus stop, no accessible name of its own.
const HIDDEN_TRIGGER_CLASS: &str = "tw:hidden";
/// The top-layer copy of the shell's interior (the popover's anchor
/// visual): the same segment row, interactive, with NO border and NO
/// background of its own — the merged outline owns the chrome while open.
const ANCHOR_VISUAL_CLASS: &str = "tw:inline-flex tw:h-full tw:w-full tw:min-w-0 tw:items-stretch tw:overflow-hidden tw:rounded-[7px] tw:text-left";
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

    /// The active-tab mark (D15 + #481 grammar): the open untinted segment
    /// wears the spectrum you-are-here underline; a status-tinted open
    /// segment keeps its semantic bed alone (semantics beat decoration).
    #[test]
    fn only_the_untinted_open_segment_wears_the_spectrum_line() {
        let open_neutral = segment_class(SegmentEdge::First, SegmentTint::None, true);
        let closed_neutral = segment_class(SegmentEdge::First, SegmentTint::None, false);
        let open_warning = segment_class(SegmentEdge::Last, SegmentTint::Warning, true);
        let open_error = segment_class(SegmentEdge::Last, SegmentTint::Error, true);

        assert!(open_neutral.contains("--studio-spectrum"));
        assert!(!closed_neutral.contains("--studio-spectrum"));
        assert!(!open_warning.contains("--studio-spectrum"));
        assert!(!open_error.contains("--studio-spectrum"));
    }

    /// The tab protocol (D15): clicking the open section closes; clicking
    /// any other section opens there — including while already open, which
    /// is the in-place switch (open stays true, only the section moves).
    #[test]
    fn segments_act_as_tabs() {
        use ControlSegment::{Changes, Device, Project};

        // Closed: any click opens its own section.
        assert_eq!(next_panel_state(false, Device, Project), (true, Project));
        assert_eq!(next_panel_state(false, Device, Device), (true, Device));
        // Open, same section: closes (the section is irrelevant after).
        assert_eq!(next_panel_state(true, Project, Project), (false, Project));
        // Open, different section: switches IN PLACE — never a close.
        assert_eq!(next_panel_state(true, Project, Changes), (true, Changes));
        assert_eq!(next_panel_state(true, Changes, Device), (true, Device));
    }

    /// The receipt names the version Save will bank when the projection
    /// knows it (D14: the same number the timeline's newest row will wear),
    /// and stays honest when it does not.
    #[test]
    fn the_receipt_names_the_version_save_banks() {
        assert_eq!(
            save_receipt_line(Some(13)),
            "Already live in this session \u{2014} Save banks v13 to your library."
        );
        assert_eq!(
            save_receipt_line(None),
            "Already live in this session \u{2014} Save banks everything to your library."
        );
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
