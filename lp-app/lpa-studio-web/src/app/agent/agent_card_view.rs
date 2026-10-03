//! [`AgentCardView`]: one card the app agent put in the chat — an action
//! only the user may press, handed over as the real control.
//!
//! A card that names an offer which takes values (flash *which board*) is
//! drawn with that offer's own controls from the live tree, set to the
//! agent's values: the board pick a Flash wears on the device card, or the
//! generic parameter form for any other offer. The user may change a value
//! before pressing, and the press is the offer's own binding of what they
//! settled on (`UiOffer::press`) — the same arming button the device card
//! shows, never an op built here. Core recognizes any press of that offer as
//! the card's answer and tells the agent what the user changed.
//!
//! A card for a one-click verb draws the action it carries, verbatim.
//! Dismiss is the card's own action from core.

use dioxus::prelude::*;
use lpa_studio_core::{FLASH_BOARD_PARAM, UiAction, UiAgentCard, UiAgentCardState, UiOffer};

use crate::app::home::device_pick_popover::{BoardPickPanel, ChipSource};
use crate::core::action::{ActionButton, ActionButtonVariant};
use crate::core::{OfferParamsForm, OfferPressButton, use_offers};

/// One app-agent card: what it does and why the assistant asks, then —
/// while it waits — the control and the press, or what came of it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentCardView(
    card: UiAgentCard,
    /// The chip a board pick is filtered by, and which source named it:
    /// the filter line's words, as the device card's picker says them.
    #[props(default)]
    chip: Option<(String, ChipSource)>,
    /// Stories only: start with the press armed.
    #[props(default)]
    armed_preview: bool,
    on_action: EventHandler<UiAction>,
) -> Element {
    let tree = use_offers();
    let args = use_signal(|| card.args.clone());
    let live: Option<UiOffer> = card
        .offer
        .as_ref()
        .and_then(|path| tree.read().get(path).cloned());
    let current = args.read().clone();

    let body = match &card.state {
        UiAgentCardState::Pending => match (&card.offer, live) {
            // The offer takes values: its own controls, pre-filled.
            (Some(_), Some(offer)) if !offer.params().is_empty() => {
                let board_pick = offer
                    .params()
                    .iter()
                    .any(|param| param.name == FLASH_BOARD_PARAM);
                rsx! {
                    div { class: CONTROL_CLASS,
                        if board_pick {
                            BoardPickPanel { offer: offer.clone(), args, chip, on_action }
                        } else {
                            div { class: "tw:p-2.5",
                                OfferParamsForm { offer: offer.clone(), args }
                            }
                        }
                    }
                    div { class: ROW_CLASS,
                        OfferPressButton {
                            offer,
                            args: current,
                            variant: ActionButtonVariant::Outline,
                            armed_preview,
                            on_action,
                        }
                        DismissButton { card: card.clone(), on_action }
                    }
                }
            }
            // It named an offer the tree no longer publishes (the board
            // went away): nothing to press.
            (Some(_), None) => rsx! {
                p { class: NOTE_CLASS, "This is not offered any more." }
                div { class: ROW_CLASS,
                    DismissButton { card: card.clone(), on_action }
                }
            },
            // A one-click verb: the action the card carries, verbatim.
            _ => rsx! {
                div { class: ROW_CLASS,
                    ActionButton {
                        action: card.press.clone(),
                        running: false,
                        variant: ActionButtonVariant::Outline,
                        armed_preview,
                        on_action,
                    }
                    DismissButton { card: card.clone(), on_action }
                }
            },
        },
        UiAgentCardState::Done { outcome } if outcome.is_empty() => rsx! {
            p { class: NOTE_CLASS, "Done." }
        },
        UiAgentCardState::Done { outcome } => rsx! {
            p { class: NOTE_CLASS, "Done: {outcome}" }
        },
        UiAgentCardState::Failed { error } => rsx! {
            p { class: FAILED_CLASS, "It failed: {error}" }
        },
        UiAgentCardState::Dismissed => rsx! {
            p { class: NOTE_CLASS, "Dismissed." }
        },
    };

    rsx! {
        section { class: card_class(card.destructive && card.is_pending()),
            div { class: "tw:grid tw:min-w-0 tw:gap-1",
                p { class: "tw:m-0 tw:text-sm tw:font-semibold tw:text-strong-foreground",
                    "{card.title}"
                }
                if !card.message.is_empty() {
                    p { class: "tw:m-0 tw:text-xs tw:leading-relaxed tw:text-muted-foreground",
                        "{card.message}"
                    }
                }
            }
            // The assistant's own line: model text, drawn as plain text.
            if !card.why.is_empty() {
                p { class: WHY_CLASS, "{card.why}" }
            }
            {body}
        }
    }
}

/// The card's Dismiss: core's action for it.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn DismissButton(card: UiAgentCard, on_action: EventHandler<UiAction>) -> Element {
    rsx! {
        ActionButton {
            action: card.dismiss_action(),
            running: false,
            variant: ActionButtonVariant::Quiet,
            on_action,
        }
    }
}

/// The card's frame: the error tint's border while a press that takes
/// something away waits.
fn card_class(destructive: bool) -> &'static str {
    if destructive {
        "tw:grid tw:min-w-0 tw:gap-2.5 tw:rounded-md tw:border tw:border-status-error-border tw:bg-card tw:p-3"
    } else {
        "tw:grid tw:min-w-0 tw:gap-2.5 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-3"
    }
}

/// The control's well: the picker sits in it the way it sits in the device
/// card's popover.
const CONTROL_CLASS: &str = "tw:min-w-0 tw:overflow-hidden tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-subtle";

const ROW_CLASS: &str = "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2";

const WHY_CLASS: &str = "tw:m-0 tw:border-l-2 tw:border-border-strong tw:pl-2 tw:text-xs tw:italic tw:text-muted-foreground";

const NOTE_CLASS: &str = "tw:m-0 tw:text-xs tw:text-dim-foreground";

const FAILED_CLASS: &str = "tw:m-0 tw:text-xs tw:text-status-error-foreground";
