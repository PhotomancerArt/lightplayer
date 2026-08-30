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
//! 2. **Nothing is offered that cannot happen.** Setup is round 2, so a blank
//!    or foreign board shows its honest classification with the setup verb
//!    DISABLED and a note saying so — never a button that does nothing.
//!
//! Plain on purpose: this milestone is the wiring, not the visual design.

use dioxus::prelude::*;
use lpa_studio_core::{
    DeviceActivityView, DeviceStatus, DeviceView, PendingLinkView, UiAction, UiStatus,
    device_escape_action, device_status_kind, pending_escape_action,
};

use crate::core::{ActionButton, ActionButtonVariant, StatusChip};

/// What a card says where a Setup button will go in round 2.
const SETUP_COMING_BACK: &str =
    "Setting a board up (firmware, naming, a first project) is coming back soon.";

/// One device card.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceRosterCard(card: DeviceView, on_action: EventHandler<UiAction>) -> Element {
    let device = card.id;
    let needs_setup = card.status == DeviceStatus::NeedsAttention;
    let status = UiStatus {
        label: card.state_label.clone(),
        kind: device_status_kind(card.status),
    };

    rsx! {
        article { class: card_class(),
            // The armed-confirm scope: while a footer chip is ARMED, the
            // card previews its own removal — this wrapper dims and the
            // scope grows a red inset ring, all via `:has()` (style.css),
            // so no armed state ever reaches this renderer. The footer
            // stays outside: the asking chip keeps full contrast.
            div { class: "ux-armed-dim tw:grid tw:gap-3",
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

                if needs_setup {
                    div { class: note_class(),
                        p { class: "tw:m-0", "{SETUP_COMING_BACK}" }
                        button {
                            class: disabled_button_class(),
                            r#type: "button",
                            disabled: true,
                            title: "{SETUP_COMING_BACK}",
                            "Set up this device"
                        }
                    }
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
/// up with two cards for one board.
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

                // `can_adopt` is the MODEL saying a user gesture may create a
                // device here — a blank chip may never identify itself. The
                // gesture it creates is Setup's, and Setup is round 2, so the
                // affordance is shown honestly disabled rather than hidden.
                if pending.can_adopt {
                    div { class: note_class(),
                        p { class: "tw:m-0", "{SETUP_COMING_BACK}" }
                        button {
                            class: disabled_button_class(),
                            r#type: "button",
                            disabled: true,
                            title: "{SETUP_COMING_BACK}",
                            "Set up this device"
                        }
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
    // button that has stopped responding.
    let label = match activity.cancel_requested {
        true => format!("{} — cancelling", activity.label),
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
