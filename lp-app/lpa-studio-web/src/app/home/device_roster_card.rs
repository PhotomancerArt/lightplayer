//! Device cards, rendered straight off the model's projection.
//!
//! There is no view model between `lpa-devices` and this file: a
//! [`DeviceView`] already IS the card (title, state line, detail, freshness,
//! activity, escapes), computed as a pure function of intent + evidence +
//! activity. So this renderer makes no decisions about devices — it lays out
//! what the fold concluded, and every affordance it draws comes from the DTO.
//!
//! # Four zones, in this order, always (Yona's ruling, G1 2026-08-31)
//!
//! ```text
//!   header      title + status chip + identity
//!   ────────────
//!   state       what it is, what is happening, and the state's ONE verb
//!   ────────────
//!   terminal    what the board actually said, and what Studio did to it
//!   ────────────
//!   actions     every escape, plus the always-actions
//! ```
//!
//! The zones are **stable**: on a linked card all four are present in every
//! state, separators and all. That is the point of them. Faces and labels
//! appearing and vanishing made the card jump around the gallery while a
//! flash ran, and a fixed-height terminal panel is what absorbs that churn —
//! a box that is always the same size cannot resize the card.
//!
//! Two rules it also has to keep:
//!
//! 1. **Every escape the DTO carries is rendered, in every state.** Invariant
//!    I3 lives in the model, but a renderer that dropped an escape would
//!    defeat it from outside, which is exactly how the shipped card lost its
//!    danger zone in the states that needed it.
//! 2. **Nothing is offered that cannot happen.** The needs-firmware face
//!    (round 2, the card ruling: no wizard — the card is the whole flow)
//!    offers a board pick + one primary verb, and only when the model's
//!    fold says the board wants firmware AND a served build resolves;
//!    otherwise the face says honestly why not.
//!
//! Plain on purpose: dress-up belongs to the design spike.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceAction, DeviceActivityView, DeviceEscape, DeviceId, DeviceLoadedProject, DevicePushOp,
    DeviceView, DevicesOp, PendingLinkView, PushSourceGroup, UiAction, UiExampleCard,
    UiPackageCard, UiStatus, device_escape_action, device_status_kind, flash_offer,
    pending_escape_action, push_offer,
};

use crate::base::icon::{NodeKindIcon, StudioIconName};
use crate::base::option_cards::{OptionCard, OptionCards};
use crate::core::{ActionButton, ActionButtonVariant, StatusChip};

/// One device card.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceRosterCard(
    card: DeviceView,
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    /// Story-only: render the Forget escape already ARMED, so captures can
    /// show the 2K+ armed dress and the card's `:has()` marking. Real
    /// surfaces never set this.
    #[props(default)]
    armed_preview: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let device = card.id;
    let status = UiStatus {
        label: card.state_label.clone(),
        kind: device_status_kind(card.status),
    };
    // The flash face appears on a settled needs-firmware verdict, never
    // while an activity runs (the activity row is the face then).
    let offer_flash = card.needs_firmware && card.activity.is_none();
    // The empty face: a LightPlayer that has REPORTED nothing loaded. A
    // board that simply has not said yet gets neither face — see
    // `DeviceLoadedProject::Unknown`.
    let offer_push = card.can_receive_project && card.loaded_project == DeviceLoadedProject::Empty;
    let running = match &card.loaded_project {
        DeviceLoadedProject::Running { label } => Some(label.clone()),
        DeviceLoadedProject::Empty | DeviceLoadedProject::Unknown => None,
    };
    // "Linked" in projection terms: Disconnect is offered exactly when the
    // model has a link for this device. It gates the terminal zone (an
    // offline card has no wire to show) and the always-actions that need a
    // port, which is the same condition the model's own spawns check.
    let linked = card.escapes.contains(&DeviceEscape::Disconnect);
    let idle = card.activity.is_none();

    rsx! {
        article { class: card_class(),
            // The armed-confirm scope (design spike, main): while a footer
            // chip is ARMED, the card previews its own removal — this
            // wrapper dims and the scope grows a red inset ring, all via
            // `:has()` (style.css), so no armed state ever reaches this
            // renderer. Zones 1-3 live inside; the actions footer stays
            // OUTSIDE so the asking chip keeps full contrast.
            div { class: "ux-armed-dim tw:grid tw:gap-3",
                // ── zone 1: header ──────────────────────────────────────
                header { class: "tw:grid tw:gap-1.5",
                    div { class: "tw:flex tw:items-start tw:justify-between tw:gap-3",
                        h3 { class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                            "{card.title}"
                        }
                        StatusChip { status }
                    }
                    if let Some(identity) = card.identity_label.clone() {
                        p { class: mono_line_class(), "{identity}" }
                    }
                }

                // ── zone 2: state — what it is, what happens, one verb ──
                section { class: zone_class(),
                    div { class: "tw:grid tw:gap-1",
                        // The running face's headline: what the board says
                        // it is running, above the firmware detail.
                        if let Some(running) = running.clone() {
                            p { class: detail_class(), "Running {running}" }
                        }
                        if let Some(detail) = card.detail.clone() {
                            p { class: detail_class(), "{detail}" }
                        }
                        // Honest staleness instead of a meaningless spinner.
                        if let Some(freshness) = card.freshness_label.clone() {
                            p { class: quiet_line_class(), "{freshness}" }
                        }
                        if let Some(outcome) = card.last_outcome.clone() {
                            p {
                                class: if outcome.ok { detail_class() } else { failure_line_class() },
                                "{outcome.summary}"
                            }
                        }
                    }

                    // The bar and its label stay HERE, with the state they
                    // describe; the terminal carries the narration.
                    if let Some(activity) = card.activity.clone() {
                        ActivityRow { activity }
                    }

                    if offer_flash {
                        FlashFace {
                            device,
                            detected_chip: card.detected_chip.clone(),
                            on_action,
                        }
                    }

                    if offer_push {
                        EmptyFace { card: card.clone(), projects, examples, on_action }
                    }
                }

                // ── zone 3: terminal ────────────────────────────────────
                // Always present on a linked card, empty or not: a panel
                // that comes and goes is a panel that resizes the card,
                // which is the churn the four zones exist to stop.
                if linked {
                    TerminalPanel { lines: card.terminal_lines.clone() }
                }
            }

            // ── zone 4: actions (outside the armed-dim scope) ───────────
            // Every escape the projection carries, in every state — including
            // Forget mid-activity, which the shipped system could not do.
            // The always-actions join when the board is linked and idle (the
            // same condition the model's spawns check); their armed inline
            // confirms ride the action meta.
            footer { class: actions_zone_class(),
                for escape in card.escapes.iter().copied() {
                    ActionButton {
                        key: "{escape:?}",
                        action: device_escape_action(escape, device),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        armed_preview: armed_preview && escape == DeviceEscape::Forget,
                        on_action,
                    }
                }
                // Only when the board has SAID it is running something: a
                // delete offered over a board that never reported one would
                // be a verb aimed at a guess.
                if card.can_remove_project {
                    ActionButton {
                        key: "{\"remove-project\"}",
                        action: DevicesOp::action_for(DeviceAction::RemoveProject { device }),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
                if idle && linked {
                    ActionButton {
                        key: "{\"reset-board\"}",
                        action: DevicesOp::action_for(DeviceAction::ResetBoard { device }),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
                if idle && linked {
                    ActionButton {
                        key: "{\"factory-reset\"}",
                        action: DevicesOp::action_for(DeviceAction::Erase { device }),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
            }
        }
    }
}

/// The terminal zone: what the board said and what Studio did to it.
///
/// Deliberately dumb and deliberately FIXED-HEIGHT. It renders the fold's
/// own tail verbatim, and its height never depends on how much there is —
/// an empty board and a mid-flash board occupy exactly the same space,
/// which is what keeps the card still while an activity runs.
///
/// The lines are rendered NEWEST FIRST into a `column-reverse` box, which
/// paints them oldest-top / newest-bottom and pins the scroll to the bottom
/// on its own. That is the whole reason for the reversal: a live log that
/// showed its first ten lines forever would answer the wrong question, and
/// scrolling it from here would mean owning scroll state the card has no
/// business keeping.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TerminalPanel(lines: Vec<String>) -> Element {
    rsx! {
        section { class: zone_class(),
            div { class: terminal_class(),
                if lines.is_empty() {
                    p { class: "tw:m-0 tw:opacity-60", "Nothing from this board yet." }
                } else {
                    for (index , line) in lines.iter().enumerate().rev() {
                        p { key: "{index}", class: "tw:m-0 tw:whitespace-pre-wrap tw:break-all", "{line}" }
                    }
                }
            }
        }
    }
}

/// The roster's "new device found, identifying…" entry.
///
/// It is deliberately not a device card: nothing about it is known yet, and
/// promoting it to one before the fold settles is how the shipped system ended
/// up with two cards for one board. Once the verdict settles at
/// needs-firmware, the SAME flash face appears here — the gesture adopts the
/// link, so flashing IS the "keep this one" decision.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn PendingLinkCard(
    pending: PendingLinkView,
    on_action: EventHandler<UiAction>,
) -> Element {
    let link = pending.link;
    let status = UiStatus {
        label: "Identifying".to_string(),
        kind: lpa_studio_core::UiStatusKind::Working,
    };

    rsx! {
        article { class: card_class(),
            // Same armed-confirm scope as the device card: Dismiss armed =
            // this entry previews its removal; the footer keeps contrast.
            div { class: "ux-armed-dim tw:grid tw:gap-3",
                header { class: "tw:flex tw:items-start tw:justify-between tw:gap-3",
                    h3 { class: "tw:m-0 tw:min-w-0 tw:truncate tw:text-sm tw:font-bold tw:text-strong-foreground",
                        "{pending.title}"
                    }
                    StatusChip { status }
                }
                p { class: detail_class(), "{pending.state_label}" }
                if let Some(detail) = pending.detail.clone() {
                    p { class: quiet_line_class(), "{detail}" }
                }

                if pending.needs_firmware {
                    FlashFace {
                        device: pending.device,
                        detected_chip: pending.detected_chip.clone(),
                        on_action,
                    }
                } else if pending.can_adopt {
                    // A blank chip may never identify itself, so a user
                    // gesture must be able to keep it. On a needs-firmware
                    // verdict the Flash verb IS that gesture; here the plain
                    // adopt is live (a disabled stub through round 1).
                    ActionButton {
                        action: DevicesOp::action_for(DeviceAction::AdoptLink { link }),
                        running: false,
                        variant: ActionButtonVariant::Outline,
                        on_action,
                    }
                }
            }

            footer { class: "tw:mt-auto tw:flex tw:flex-wrap tw:gap-2",
                for escape in pending.escapes.iter().copied() {
                    ActionButton {
                        key: "{escape:?}",
                        action: pending_escape_action(escape, link),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
                    }
                }
                // The silent-board recovery: a chip parked in ROM
                // download-wait prints nothing, so identify can never
                // settle — a hardware reset reboots it into honest boot
                // output (G1 2026-08-31, the erased C6).
                ActionButton {
                    key: "{\"reset-board\"}",
                    action: DevicesOp::action_for(DeviceAction::ResetBoard {
                        device: pending.device,
                    }),
                    running: false,
                    variant: ActionButtonVariant::Quiet,
                    on_action,
                }
            }
        }
    }
}

/// The needs-firmware face: the board pick (the EXPLAINING selector, filtered
/// to the detected chip's compatible boards) + ONE primary verb.
///
/// The board choice is ephemeral UI state; picking journals nothing. The
/// decision is journaled by the Flash ACTION it parameterizes — board id and
/// resolved build id ride the gesture into the model, and there is no wizard
/// state anywhere (the card ruling).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FlashFace(
    device: DeviceId,
    detected_chip: Option<String>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let offer = flash_offer(detected_chip.as_deref());
    let mut picked = use_signal(|| offer.preselect.clone());
    // A late-arriving boot banner can grow the offer after first render;
    // a stale pick that no longer exists must not survive it.
    let pick_exists = picked
        .read()
        .as_deref()
        .is_some_and(|id| offer.candidates.iter().any(|card| card.board_id == id));
    let effective = match pick_exists {
        true => picked.read().clone(),
        false => offer.preselect.clone(),
    };

    if let Some(unavailable) = offer.unavailable {
        return rsx! {
            div { class: note_class(),
                p { class: "tw:m-0", "{unavailable}" }
            }
        };
    }

    let choice = effective.as_deref().and_then(|id| {
        offer
            .candidates
            .iter()
            .find(|candidate| candidate.board_id == id)
            .cloned()
    });
    let options: Vec<OptionCard> = offer
        .candidates
        .iter()
        .map(|candidate| {
            OptionCard::new(
                &candidate.board_id,
                StudioIconName::Usb,
                &candidate.title,
                &candidate.blurb,
            )
        })
        .collect();

    rsx! {
        div { class: "tw:grid tw:gap-2",
            OptionCards {
                label: Some("Which board is this?".to_string()),
                options,
                selected: effective.clone(),
                on_pick: move |id: String| picked.set(Some(id)),
            }
            if detected_chip.is_none() {
                // Mid-stream attach: no boot output ever named the chip, so
                // the pick is unfiltered — say what keeps that safe.
                p { class: "tw:m-0 tw:text-xs tw:opacity-70",
                    "Studio hasn't seen this board name its chip — your pick is checked \
                     against the actual chip before anything is written."
                }
            }
            if let Some(choice) = choice {
                ActionButton {
                    action: DevicesOp::action_for(DeviceAction::Flash {
                        device,
                        board_id: choice.board_id.clone(),
                        build_id: choice.build_id.clone(),
                        park_first: choice.park_first,
                    }),
                    running: false,
                    variant: ActionButtonVariant::Solid,
                    on_action,
                }
            } else {
                // No pick yet (several candidates): the verb waits, honestly
                // disabled, rather than guessing a board.
                button {
                    class: disabled_button_class(),
                    r#type: "button",
                    disabled: true,
                    title: "Pick the board first — the pin map is written to the device.",
                    "Flash firmware"
                }
            }
        }
    }
}

/// The empty face: ONE inline picker with three sources, and one primary
/// verb (the card ruling — no wizard, no dialog flow).
///
/// The pick is ephemeral UI state, exactly like the board pick above it:
/// nothing is journaled until the verb is pressed, and the op it dispatches
/// carries the chosen source. A retry after a failure is the same face,
/// still here, still picked — which is what "in place" means.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn EmptyFace(
    card: DeviceView,
    projects: Vec<UiPackageCard>,
    examples: Vec<UiExampleCard>,
    on_action: EventHandler<UiAction>,
) -> Element {
    let device = card.id;
    let offer = push_offer(&card, &projects, &examples);
    let mut picked = use_signal(|| offer.preselect.clone());
    // The library and the example list can both grow under a card that is
    // already on screen; a pick that no longer exists must not survive it.
    let pick_exists = picked
        .read()
        .as_deref()
        .is_some_and(|key| offer.choices.iter().any(|choice| choice.key == key));
    let effective = match pick_exists {
        true => picked.read().clone(),
        false => offer.preselect.clone(),
    };

    if let Some(unavailable) = offer.unavailable.clone() {
        return rsx! {
            div { class: note_class(),
                p { class: "tw:m-0", "{unavailable}" }
            }
        };
    }

    let chosen = effective.as_deref().and_then(|key| {
        offer
            .choices
            .iter()
            .find(|choice| choice.key == key)
            .cloned()
    });
    let options: Vec<OptionCard> = offer
        .choices
        .iter()
        .map(|choice| {
            OptionCard::new(
                &choice.key,
                // A project is a project: the group label already says
                // where it comes from, so only the one being CREATED wears
                // a different glyph.
                match choice.group {
                    PushSourceGroup::New => StudioIconName::Add,
                    PushSourceGroup::Example | PushSourceGroup::Library => {
                        StudioIconName::NodeKind(NodeKindIcon::Module)
                    }
                },
                &format!("{} · {}", choice.group.label(), choice.title),
                &choice.blurb,
            )
        })
        .collect();

    rsx! {
        div { class: "tw:grid tw:gap-2",
            OptionCards {
                label: Some("Put something on it".to_string()),
                options,
                selected: effective.clone(),
                on_pick: move |key: String| picked.set(Some(key)),
            }
            // Said out loud rather than silently omitted: a board that
            // cannot have a starter generated for it has a reason.
            if let Some(reason) = offer.new_project_unavailable.clone() {
                p { class: quiet_line_class(), "{reason}" }
            }
            if let Some(chosen) = chosen {
                ActionButton {
                    action: DevicePushOp::action_for(device, chosen.source.clone()),
                    running: false,
                    variant: ActionButtonVariant::Solid,
                    on_action,
                }
            } else {
                button {
                    class: disabled_button_class(),
                    r#type: "button",
                    disabled: true,
                    title: "Pick what to put on the board first.",
                    "Put it on the board"
                }
            }
        }
    }
}

/// The running activity: what it is, how far along, and — from the card's
/// escapes, not from here — how to stop it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ActivityRow(activity: DeviceActivityView) -> Element {
    let percent = activity.percent.map(u32::from);
    let bar_style = match percent {
        Some(percent) => format!("width: {}%;", percent.min(100)),
        None => String::new(),
    };
    let bar_class = match percent {
        Some(_) => "tw:h-full tw:rounded-pill tw:bg-accent",
        None => {
            "tw:h-full tw:w-[35%] tw:rounded-pill tw:bg-accent [animation:ux-progress-sweep_1.2s_ease-in-out_infinite]"
        }
    };
    // A requested cancel is a STATE, not the absence of one: the activity is
    // winding down and will be evicted if it does not. Saying so beats a
    // button that has stopped responding. A flash's cancel can hold for the
    // rest of the write window — esptool cannot stop mid-image cleanly.
    let label = match activity.cancel_requested {
        true => format!(
            "{} — cancelling (finishing the current write)",
            activity.label
        ),
        false => activity.label.clone(),
    };

    rsx! {
        div { class: "tw:grid tw:gap-1",
            p { class: quiet_line_class(), "{label}" }
            div { class: "tw:h-1.5 tw:overflow-hidden tw:rounded-pill tw:bg-subtle-bg",
                div { class: bar_class, style: bar_style }
            }
        }
    }
}

fn card_class() -> &'static str {
    // `ux-armed-scope`: the card is the blast radius of its own armed
    // destructive chips — `:has(.ux-armed)` marks it (style.css).
    "ux-armed-scope tw:grid tw:content-start tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-panel tw:p-4"
}

/// One zone below the header: a rule above it, and room to breathe. The
/// separators are what make the four zones legible as four zones.
fn zone_class() -> &'static str {
    "tw:grid tw:gap-2 tw:border-t tw:border-border tw:pt-3"
}

/// The actions zone: the same separator, but the buttons wrap in a row
/// rather than stacking, and it is pushed to the bottom so cards of
/// different heights still line their action rows up.
fn actions_zone_class() -> &'static str {
    "tw:mt-auto tw:flex tw:flex-wrap tw:gap-2 tw:border-t tw:border-border tw:pt-3"
}

/// The terminal panel itself: ten lines tall, always. A fixed height with
/// `overflow-y-auto` is what makes the box immune to its own contents, and
/// `flex-col-reverse` is what keeps it showing the newest line — see
/// [`TerminalPanel`] for why the rows are fed in reverse.
fn terminal_class() -> &'static str {
    "tw:flex tw:h-40 tw:flex-col-reverse tw:overflow-y-auto tw:overflow-x-hidden tw:rounded-md tw:border tw:border-border tw:bg-subtle-bg tw:px-2 tw:py-1.5 tw:font-mono tw:text-[0.68rem] tw:leading-[1.35] tw:text-subtle-foreground"
}

fn detail_class() -> &'static str {
    "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-muted-foreground"
}

fn quiet_line_class() -> &'static str {
    "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-subtle-foreground"
}

fn failure_line_class() -> &'static str {
    "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-status-error-foreground"
}

fn mono_line_class() -> &'static str {
    "tw:m-0 tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground"
}

fn note_class() -> &'static str {
    "tw:grid tw:gap-2 tw:rounded-md tw:border tw:border-dashed tw:border-border tw:px-3 tw:py-2.5 tw:text-xs tw:leading-relaxed tw:text-subtle-foreground"
}

fn disabled_button_class() -> &'static str {
    "tw:inline-flex tw:w-fit tw:cursor-not-allowed tw:items-center tw:rounded-md tw:border tw:border-border tw:px-2.5 tw:py-1 tw:text-xs tw:font-semibold tw:text-subtle-foreground tw:opacity-60"
}
