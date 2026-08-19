//! The header **session·project control** and its panel — the one piece of
//! session UI the chrome carries under the single-session web policy.
//!
//! `❖ Sim · ESP32-C6 │ ✎ mini dome ②   [Save] [↺]` — one segmented lockup
//! (spike `spikes/studio-chrome/` concept **B**, ruled round 7) with the
//! save moment materializing BESIDE it the instant persisted edits exist
//! (G1 round-2, 2026-08-19: inspect and act are different surfaces — the
//! actions left the box).
//!
//! **Why a lockup and not two chips.** With one session per tab the device
//! and the project are not two things you switch between — they are the two
//! halves of one answer to "what is this tab?". The single-session ruling
//! deleted the switcher, the strip, and the washed off-editor chip; what is
//! left is a control, so it reads as one object with internal seams rather
//! than a row of independent pills.
//!
//! **Click grammar (R8-2, amended at G1).** Every segment of the lockup
//! opens the panel — with one session per tab the device segment has
//! nothing to navigate *to*, you are always here. Save/↺ are direct
//! actions and live OUTSIDE the lockup as ordinary sibling buttons: the
//! first cut carried them as trailing segments, and the gate found a box
//! that half-inspects and half-acts reads as odd (it also forced invalid
//! button-in-button nesting through the popover trigger). The lockup is
//! now purely the popover trigger.
//!
//! **One ungated mount** (Q10 lesson, #426): the control is never wrapped in
//! a `tw:@min-*` container. A top-layer popover cannot answer a container
//! query, and mounting the trigger twice gives the header two popovers that
//! disagree about which one is open. The FOLDS ride the pieces instead — the
//! device name hides below the 900px cut, ↺ below 680, matching the cuts the
//! tab row already uses.
//!
//! The control is presentational: everything it renders comes from
//! [`UiChromeSessionControl`] (core's projection of THE session) and the
//! open project's [`ProjectDetailContent`] — the same value the pane's [i]
//! renders, so the chrome and the pane can never disagree about a project.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiAffordance, UiChromeSessionControl, UiChromeSessionStatus, UiPaneAction,
};

use crate::app::affordance::affordance_trigger_style;
use crate::app::project::project_pane::trigger_label;
use crate::app::project::{ProjectDetailContent, ProjectDetailSections};
use crate::base::{DetailPopover, IconMenuTone, PopoverPlacement, StudioIcon, StudioIconName};

/// Everything the header control renders: THE session (core's control
/// projection) and the open project's detail content, if a project is open
/// at all — a connected board with nothing loaded is a real state, and the
/// project segment says so rather than inventing a name.
///
/// This is the header project chip's successor (P4-retired): same
/// `content` + `on_action` pair, now paired with the session the project
/// runs on.
#[derive(Clone, PartialEq)]
pub struct ChromeSessionControl {
    /// THE session this tab runs.
    pub session: UiChromeSessionControl,
    /// The open project's detail content; `None` with no project open.
    pub project: Option<ProjectDetailContent>,
    /// Dispatch for the panel's rows and the lockup's Save/↺ segments.
    pub on_action: EventHandler<UiAction>,
    /// Open the panel immediately (stories only).
    pub initially_open: bool,
}

/// The B lockup (device segment · project segment — one popover trigger)
/// with Save/↺ standing beside it while the project is dirty.
///
/// **Trigger shape.** The lockup alone is the [`DetailPopover`] trigger;
/// Save/↺ are ordinary sibling buttons in the same flex row (G1 round-2
/// ruling). While the popover is open the trigger's subtree renders TWICE
/// (the in-flow placeholder plus the top-layer copy above the merged
/// outline), so nothing stateful may ever live inside the lockup — but the
/// action buttons sit outside that subtree and render once.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SessionProjectControl(control: ChromeSessionControl) -> Element {
    let ChromeSessionControl {
        session,
        project,
        on_action,
        initially_open,
    } = control;
    let affordance = project.as_ref().map(ProjectDetailContent::affordance);
    let style = affordance.map(affordance_trigger_style);
    // The popover's chrome tone follows the PROJECT's state (the thing with
    // states worth announcing); a session with no project keeps the quiet
    // treatment rather than borrowing a status it does not have.
    let tone = style.map_or(IconMenuTone::Quiet, |style| style.tone);
    let icon = style.map_or(kind_icon(session.sim), |style| style.icon);
    let label = control_label(&session, project.as_ref());
    // The save moment: the controller publishes Save/Revert on the editor's
    // `header_actions` exactly while persisted edits are pending, so their
    // presence IS the dirty test — the control never recomputes dirtiness.
    let (save, revert) = project
        .as_ref()
        .map(|project| save_and_revert(project.header_actions()))
        .unwrap_or_default();
    let board = board_suffix(&session);
    let name = session.name.clone();
    let kind_title = if session.sim {
        "This tab's session — the simulator"
    } else {
        "This tab's session — the connected device"
    };

    let trigger = rsx! {
        // Device segment: kind glyph, D16 status dot, name, board suffix.
        span { class: SEGMENT_CLASS, title: "{kind_title}",
            span { class: kind_glyph_class(session.sim),
                StudioIcon { name: kind_icon(session.sim), size: 12 }
            }
            span { class: dot_class(session.status) }
            // The md fold: below the 900px cut the glyph and the dot carry
            // kind and health alone — the two facts that survive a squeeze.
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
        // Project segment: the state glyph + name + amber count, wearing
        // the state's own tint (G1 ruling: the affordance colors the whole
        // chip, not just its glyph).
        span { class: project_segment_class(affordance),
            if let (Some(project), Some(style)) = (project.as_ref(), style) {
                span { class: state_glyph_class(affordance),
                    StudioIcon { name: style.icon, size: 13 }
                }
                span { class: "tw:min-w-0 tw:overflow-hidden tw:text-ellipsis tw:whitespace-nowrap tw:text-[11px] tw:font-semibold tw:text-muted-foreground",
                    "{project.project_name()}"
                }
                if project.unsaved_count() > 0 {
                    span { class: COUNT_PILL_CLASS, "{project.unsaved_count()}" }
                }
            } else {
                // Connected with nothing loaded (spike §5): honest-empty,
                // no fake name and no state glyph to read a state off.
                span { class: "tw:text-[11px] tw:italic tw:text-dim-foreground", "no project" }
            }
        }
    };

    rsx! {
        // The lockup and its actions are SIBLINGS (G1 round-2 ruling,
        // 2026-08-19): one box that half-opens a panel and half-acts read
        // as odd, so the trigger is purely "inspect" and Save/↺ stand
        // apart as plain buttons. This also retires the nested-button
        // fallback the first cut shipped — the buttons are ordinary
        // top-level controls now, honest to assistive tech.
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
            DetailPopover {
                icon,
                label: label.clone(),
                title: label,
                tone,
                placement: PopoverPlacement::BottomStart,
                trigger,
                trigger_class: LOCKUP_CLASS.to_string(),
                trigger_open_class: LOCKUP_OPEN_CLASS.to_string(),
                layer_keeps_layout: true,
                initially_open,
                SessionPanel { session, project, on_action }
            }
            // Save / ↺: DIRECT actions (R8-2), appearing exactly while the
            // editor publishes them (the dirty window).
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
                // The sm fold: ↺ retreats into the panel's per-entry
                // reverts — the destructive half of the pair is the one
                // to lose first.
                button {
                    class: "{REVERT_BUTTON_CLASS} tw:hidden tw:@min-[680px]:inline-flex",
                    r#type: "button",
                    title: "{revert.meta().summary}",
                    onclick: move |_| on_action.call(revert.clone()),
                    span { class: "tw:inline-flex tw:items-center tw:text-dim-foreground",
                        StudioIcon { name: StudioIconName::Revert, size: 13 }
                    }
                }
            }
        }
    }
}

/// The control's panel: the device zone over the project zone, with the
/// "this tab is the session" hint under both. It replaces the header
/// chip's project-only popup — the old one answered half the question.
///
/// The project zone is [`ProjectDetailSections`] mounted AS-IS: identity +
/// status word, project settings, share, the pending-edit lists with
/// per-entry revert, stats. One content value, two homes (here and the
/// pane's [i]) — forking it here would be a second spelling of the same
/// panel.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SessionPanel(
    session: UiChromeSessionControl,
    /// The open project's sections; absent for a session with no project.
    #[props(default)]
    project: Option<ProjectDetailContent>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let run = run_word(&session);
    let stat_line = device_stat_line(&session);
    let hint = session_hint(session.sim);
    rsx! {
        // Device zone: the muted band, so the panel's first read is "what
        // is running", not "what is edited".
        section { class: "tw:grid tw:gap-0.5 tw:bg-card-muted tw:px-3 tw:py-2",
            div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                span { class: kind_glyph_class(session.sim),
                    StudioIcon { name: kind_icon(session.sim), size: 13 }
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
        if let Some(project) = project {
            ProjectDetailSections { content: project, on_action }
        }
        section { class: "tw:border-t tw:border-border-muted tw:px-3 tw:py-1.5",
            p { class: "tw:m-0 tw:text-[10px] tw:italic tw:leading-snug tw:text-dim-foreground",
                "{hint}"
            }
        }
    }
}

/// The session's kind glyph: the violet sim mark (the bound-family
/// convention the sim card wears), the transport icon for hardware — USB
/// today; the slot grows network/BT glyphs with those transports.
fn kind_icon(sim: bool) -> StudioIconName {
    if sim {
        StudioIconName::Simulator
    } else {
        StudioIconName::Usb
    }
}

fn kind_glyph_class(sim: bool) -> &'static str {
    if sim {
        "tw:flex tw:flex-none tw:items-center tw:text-status-bound-foreground"
    } else {
        "tw:flex tw:flex-none tw:items-center tw:text-subtle-foreground"
    }
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

/// The project segment's own background: the state tint the spike gives it
/// (amber wash unsaved, error bed failed), so the save moment is visible in
/// the bar without reading the glyph. Silent states keep the lockup's
/// surface — a clean project is not an announcement.
fn project_segment_class(affordance: Option<UiAffordance>) -> String {
    let tint = match affordance {
        Some(UiAffordance::Unsaved) => " tw:bg-status-warning-bg",
        Some(UiAffordance::Error) => " tw:bg-status-error-bg",
        _ => "",
    };
    format!("{SEGMENT_CLASS}{tint}")
}

/// The state glyph's color: the affordance family's foreground, quiet while
/// there is nothing to announce.
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

/// The sim's board suffix (`· ESP32-C6`, ruling 8.1: the sim names the board
/// it simulates). Hardware never wears one — a board's own name IS the
/// device name, and repeating it would read as two devices.
fn board_suffix(session: &UiChromeSessionControl) -> Option<String> {
    session
        .sim
        .then(|| session.board.clone())
        .flatten()
        .filter(|board| !board.is_empty())
}

/// Save and Revert, picked out of the editor's `header_actions` by their
/// icon tokens. The control dispatches the SAME actions the pane header and
/// the Tree row dispatch — one save verb in the app, three surfaces at most
/// (and after the retirements, one).
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

/// An operation in flight OUTRANKS the run state: it is the thing that will
/// refuse a nav-away, so the panel names it where the eye lands first.
fn run_word(session: &UiChromeSessionControl) -> RunWord {
    if let Some(busy) = session.busy.as_ref() {
        return RunWord {
            text: busy.clone(),
            class: "tw:text-status-working-foreground",
        };
    }
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

/// The panel's footer line: the single-session policy said plainly, because
/// the consequence of navigating away is otherwise invisible. The document
/// is durable (the draft overlay persists) — the SESSION is what ends.
fn session_hint(sim: bool) -> &'static str {
    if sim {
        "This tab is the session — close it or navigate away to stop the simulator."
    } else {
        "This tab is the session — close it or navigate away to stop and disconnect."
    }
}

/// The trigger's accessible name and tooltip: both halves of what the
/// lockup shows, because a screen reader gets one string for the whole
/// trigger.
fn control_label(
    session: &UiChromeSessionControl,
    project: Option<&ProjectDetailContent>,
) -> String {
    let device = match board_suffix(session) {
        Some(board) => format!("{} · {board}", session.name),
        None => session.name.clone(),
    };
    match project {
        Some(project) => format!(
            "{device} — {} ({})",
            project.project_name(),
            trigger_label(project.affordance())
        ),
        None => format!("{device} — no project open"),
    }
}

/// The lockup at rest: ONE rounded bordered container with internal
/// hairlines, not a row of chips. `p-0` because the segments own the
/// padding, and an explicit background/border because this build ships
/// Tailwind WITHOUT preflight — a bare `<button>` paints UA `buttonface`.
///
/// The hover wash rides the WHOLE lockup rather than the individual
/// segments: device and project do the same thing (open the panel), so
/// lighting them separately would promise a distinction that does not
/// exist. The Save/↺ buttons light on their own — those really are
/// different clicks.
const LOCKUP_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:items-stretch tw:overflow-hidden tw:rounded-[7px] tw:border tw:border-border-strong tw:bg-card-subtle tw:p-0 tw:text-left tw:transition-colors tw:hover:border-subtle-foreground";
/// Open: the same geometry with a brighter rim, so the group reads as the
/// thing the panel grew out of (the merged outline draws the rest).
const LOCKUP_OPEN_CLASS: &str = "tw:inline-flex tw:min-w-0 tw:cursor-pointer tw:items-stretch tw:overflow-hidden tw:rounded-[7px] tw:border tw:border-subtle-foreground tw:bg-card-subtle tw:p-0 tw:text-left";
/// One segment of the lockup.
const SEGMENT_CLASS: &str =
    "tw:inline-flex tw:min-w-0 tw:items-center tw:gap-1.5 tw:px-2.5 tw:py-1";
/// The internal hairline between segments — a drawn divider rather than a
/// per-segment border, so no segment has to fight the UA button border.
const DIVIDER_CLASS: &str = "tw:w-px tw:flex-none tw:self-stretch tw:bg-border-subtle";
/// The standalone amber Save button (G1 round-2: apart from the lockup —
/// the inspect surface and the act surface are different things). No
/// preflight, so border and background are named explicitly.
const SAVE_BUTTON_CLASS: &str = "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:rounded-md tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-2.5 tw:py-[3px] tw:transition-colors tw:hover:border-status-warning-foreground";
/// The quiet ↺ button beside it: revert never competes with save.
const REVERT_BUTTON_CLASS: &str = "tw:flex-none tw:cursor-pointer tw:items-center tw:rounded-md tw:border tw:border-border-subtle tw:bg-transparent tw:px-2 tw:py-[3px] tw:transition-colors tw:hover:border-border-strong tw:hover:bg-background-wash";
/// The unsaved count, the header chip's pill verbatim (D8): mono, amber,
/// pill-shaped — the same badge the pane's own affordances wear.
const COUNT_PILL_CLASS: &str = "tw:flex-none tw:rounded-full tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:px-1.5 tw:font-mono tw:text-[9.5px] tw:font-semibold tw:text-status-warning-foreground";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ControllerId, ProjectController, ProjectOp};

    use super::*;

    fn session(sim: bool, board: Option<&str>) -> UiChromeSessionControl {
        UiChromeSessionControl {
            key: "runtime-1".to_string(),
            sim,
            name: if sim { "Sim" } else { "Garage dome" }.to_string(),
            board: board.map(str::to_string),
            status: UiChromeSessionStatus::Run,
            busy: None,
            stat_line: None,
        }
    }

    fn pane_action(icon: &str, op: ProjectOp) -> UiPaneAction {
        UiPaneAction::new(
            icon,
            UiAction::from_op(ControllerId::new(ProjectController::NODE_ID), op),
        )
    }

    /// The lockup's Save/↺ are the controller's own header actions — not a
    /// second pair minted here — so the control, the pane header, and the
    /// popup can never save different things.
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
    /// dirty test: no actions, no trailing segments.
    #[test]
    fn a_clean_project_offers_no_save_segments() {
        assert_eq!(save_and_revert(&[]), (None, None));
    }

    /// Ruling 8.1: the sim names the board it simulates. Hardware does not —
    /// the device name already IS the board.
    #[test]
    fn only_the_sim_wears_a_board_suffix() {
        assert_eq!(
            board_suffix(&session(true, Some("ESP32-C6"))),
            Some("ESP32-C6".to_string())
        );
        assert_eq!(board_suffix(&session(true, None)), None);
        assert_eq!(board_suffix(&session(false, Some("ESP32-C6"))), None);
    }

    #[test]
    fn the_stat_line_leads_with_what_the_sim_simulates() {
        let mut sim = session(true, Some("ESP32-C6"));
        sim.stat_line = Some("60 fps · 217 lamps".to_string());
        assert_eq!(
            device_stat_line(&sim),
            Some("simulating ESP32-C6 · 60 fps · 217 lamps".to_string())
        );

        let mut hardware = session(false, None);
        hardware.stat_line = Some("USB · 217 lamps".to_string());
        assert_eq!(
            device_stat_line(&hardware),
            Some("USB · 217 lamps".to_string())
        );

        // Nothing published, nothing said — no empty mono line.
        assert_eq!(device_stat_line(&session(false, None)), None);
    }

    /// The in-flight label outranks the run state: it is what a nav-away
    /// would be refused for, so it is what the panel says.
    #[test]
    fn the_busy_label_wins_over_the_run_state() {
        let mut busy = session(true, None);
        busy.busy = Some("Deploying…".to_string());
        assert_eq!(run_word(&busy).text, "Deploying…");

        assert_eq!(run_word(&session(true, None)).text, "running");

        let mut empty = session(false, None);
        empty.status = UiChromeSessionStatus::Empty;
        assert_eq!(run_word(&empty).text, "idle");
    }

    /// Leaving costs a stop for the sim and a stop PLUS a disconnect for
    /// hardware; the hint has to say which.
    #[test]
    fn the_hint_names_what_leaving_actually_ends() {
        assert!(session_hint(true).ends_with("stop the simulator."));
        assert!(session_hint(false).ends_with("stop and disconnect."));
    }

    /// The trigger is one button, so its accessible name carries both
    /// halves — including the honest-empty state.
    #[test]
    fn the_trigger_label_carries_both_halves() {
        let label = control_label(&session(true, Some("ESP32-C6")), None);
        assert_eq!(label, "Sim · ESP32-C6 — no project open");
    }
}
