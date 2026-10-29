//! [`AgentCardView`]: the app agent handing the user the install's one
//! question — replacing the firmware a board runs — as a card, at rest and
//! armed (plan `lp2025/2026-10-28-0025-install-without-board-pick`).

use dioxus::prelude::*;
use lpa_studio_core::{
    BoardRef, DeviceActivityKind, DeviceActivityView, DeviceBoardFs, DeviceFlashReplaceView,
    DeviceResident, FilesEvidence, OfferArgs, OfferPath, UiAgentCard, UiOffer, UiOfferTree,
    device_layout_view,
};
use lpa_studio_web_story_macros::story;

use super::AgentCardView;
use crate::app::board_card::board_card_stories::porch;
use crate::core::OffersProvider;

#[story(
    description = "The app agent's Replace card, at rest. The user asked the assistant to put LightPlayer on their WLED controller. Install is one press the agent may make — nothing is written before the board is read — and the install stopped on its one question: the board runs WLED 0.15.0, and installing replaces it. That answer is Lasting, so the agent's `act` on `confirm-replace` became this card. It names what is lost in the board's own words for itself; there is nothing to choose. The italic line is the assistant's own reason, drawn as plain text. Dismiss is the card's own action from core."
)]
fn replace_card_at_rest() -> Element {
    rsx! {
        section { class: "tw:w-full tw:max-w-[452px] tw:p-4",
            StoryReplaceCard { armed: false }
        }
    }
}

#[story(
    description = "The same Replace card with its press armed: the first click on a Lasting press arms it in place, in the error tint, exactly as the board card's own question would, and only the second click lets the install write. The card then settles, and the assistant hears the user answered."
)]
fn replace_card_armed() -> Element {
    rsx! {
        section { class: "tw:w-full tw:max-w-[452px] tw:p-4",
            StoryReplaceCard { armed: true }
        }
    }
}

/// The agent's Replace card, under the tree that publishes its offer.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryReplaceCard(armed: bool) -> Element {
    let (offer, offers) = wled_replace();
    let card = agent_card(&offer);
    rsx! {
        OffersProvider { offers,
            AgentCardView {
                card,
                chip: None,
                armed_preview: armed,
                on_action: |_| {},
            }
        }
    }
}

/// The card the agent's `act` makes: the press of `offer`, handed over at
/// the offer's path.
pub(crate) fn agent_card(offer: &UiOffer) -> UiAgentCard {
    let press = offer.press(&OfferArgs::new()).expect("Replace binds");
    UiAgentCard::new(
        "c1",
        press,
        "This board runs WLED; installing LightPlayer replaces it, and its settings and presets.",
    )
    .for_offer(offer.path.clone(), OfferArgs::new())
}

/// `devices/new-3/confirm-replace` on a board whose install stopped on the
/// question — it runs WLED 0.15.0 — and the tree core published it in.
pub(crate) fn wled_replace() -> (UiOffer, UiOfferTree) {
    let mut view = porch();
    view.activity = Some(DeviceActivityView {
        kind: DeviceActivityKind::Flash,
        label: "Waiting for your answer…".to_string(),
        percent: None,
        cancellable: true,
        cancel_requested: false,
        layout: None,
        replace: Some(DeviceFlashReplaceView {
            resident: DeviceResident::Foreign {
                project_name: Some("WLED".to_string()),
                version: Some("0.15.0".to_string()),
            },
        }),
        flash_step: None,
        update: None,
    });
    let mut offers = UiOfferTree::new();
    let layout = device_layout_view(
        &view,
        OfferPath::board(&BoardRef::New(3)),
        DeviceBoardFs::Unknown,
        false,
        FilesEvidence::default(),
        None,
        None,
        None,
        &mut offers,
    )
    .expect("the question is a layout panel");
    let path = layout
        .panel
        .and_then(|panel| panel.continue_action)
        .expect("Replace");
    let offer = offers.get(&path).cloned().expect("published");
    (offer, offers)
}
