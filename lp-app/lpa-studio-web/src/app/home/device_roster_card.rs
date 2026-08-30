//! Device cards, rendered straight off the model's projection.
//!
//! There is no view model between `lpa-devices` and this file: a
//! [`DeviceView`] already IS the card (title, state line, detail, freshness,
//! activity, escapes), computed as a pure function of intent + evidence +
//! activity. So this renderer makes no decisions about devices — it lays out
//! what the fold concluded, and every affordance it draws comes from the DTO.
//!
//! Two rules it does have to keep:
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
    DeviceAction, DeviceActivityView, DeviceId, DeviceView, DevicesOp, PendingLinkView, UiAction,
    UiStatus, device_escape_action, device_status_kind, flash_offer, pending_escape_action,
};

use crate::base::icon::StudioIconName;
use crate::base::option_cards::{OptionCard, OptionCards};
use crate::core::{ActionButton, ActionButtonVariant, StatusChip};

/// One device card.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceRosterCard(card: DeviceView, on_action: EventHandler<UiAction>) -> Element {
    let device = card.id;
    let status = UiStatus {
        label: card.state_label.clone(),
        kind: device_status_kind(card.status),
    };
    // The flash face appears on a settled needs-firmware verdict, never
    // while an activity runs (the activity row is the face then).
    let offer_flash = card.needs_firmware && card.activity.is_none();

    rsx! {
        article { class: card_class(),
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

            div { class: "tw:grid tw:gap-1",
                if let Some(detail) = card.detail.clone() {
                    p { class: detail_class(), "{detail}" }
                }
                // Honest staleness instead of a spinner that means nothing.
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

            // Every escape the projection carries, in every state — including
            // Forget mid-activity, which the shipped system could not do.
            footer { class: "tw:mt-auto tw:flex tw:flex-wrap tw:gap-2",
                for escape in card.escapes.iter().copied() {
                    ActionButton {
                        key: "{escape:?}",
                        action: device_escape_action(escape, device),
                        running: false,
                        variant: ActionButtonVariant::Quiet,
                        on_action,
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
                // A blank chip may never identify itself, so a user gesture
                // must be able to keep it. On a needs-firmware verdict the
                // Flash verb IS that gesture; here the plain adopt is live
                // (it was a disabled "coming back soon" stub through round
                // 1).
                ActionButton {
                    action: DevicesOp::action_for(DeviceAction::AdoptLink { link }),
                    running: false,
                    variant: ActionButtonVariant::Outline,
                    on_action,
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
            if let Some(choice) = choice {
                ActionButton {
                    action: DevicesOp::action_for(DeviceAction::Flash {
                        device,
                        board_id: choice.board_id.clone(),
                        build_id: choice.build_id.clone(),
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
    "tw:grid tw:content-start tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-panel tw:p-4"
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
