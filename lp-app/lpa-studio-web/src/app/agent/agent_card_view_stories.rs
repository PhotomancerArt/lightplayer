//! [`AgentCardView`]: the app agent handing the user a Flash over somebody
//! else's firmware — the real board pick, pre-filled with the agent's
//! board — at rest, armed, and beside the device card's own picker.

use dioxus::prelude::*;
use lpa_studio_core::{
    BoardRef, DeviceEscape, DeviceFirmwareFace, DeviceId, DeviceLinkId, FLASH_BOARD_PARAM,
    FLASH_NAME_PARAM, OfferArgs, OfferPath, PendingLinkView, UiAgentCard, UiOffer, UiOfferTree,
    flash_pending_offer,
};
use lpa_studio_web_story_macros::story;

use super::AgentCardView;
use crate::app::home::device_pick_popover::{BoardPickMode, BoardPickPopover, ChipSource};
use crate::core::OffersProvider;

#[story(
    description = "The app agent's Flash card, at rest (M3 P4). The user asked the assistant to put LightPlayer on a board still running its factory demo; flashing over that firmware loses it, so the press is Lasting and the agent cannot make it — its `act` became this card. The card is NOT a frozen button: it draws the Flash offer's own board pick from the live tree (`devices/new-3/flash`), the same panel the device card opens, pre-filled with what the agent passed: the board it named (the XIAO ESP32-C6 tile wears the selection) and the name the user asked for (\"Shelf lamp\"). The user may pick another board, name it, or widen the list with \"show all\" before pressing; the press under it is the offer's own binding of what they settled on, with the device card's arming Flash button. The italic line is the assistant's own reason, drawn as plain text. Dismiss is the card's own action from core."
)]
fn flash_card_pre_filled() -> Element {
    rsx! {
        section { class: "tw:w-full tw:max-w-[452px] tw:p-4",
            StoryFlashCard { armed: false }
        }
    }
}

#[story(
    description = "The same Flash card with its press armed: the first click on a Lasting press arms it in place (\"Confirm flash\" in the error tint) exactly as the device card's Flash does, and only the second click flashes. Core treats that press — whatever board the user left picked — as the card's answer: the card settles, and the assistant hears which board was flashed and whether the user changed the one it suggested."
)]
fn flash_card_armed() -> Element {
    rsx! {
        section { class: "tw:w-full tw:max-w-[452px] tw:p-4",
            StoryFlashCard { armed: true }
        }
    }
}

#[story(
    description = "Side by side, the same `devices/new-3/flash` offer handed over two ways. LEFT: the device card's board pick, opened from the pending card's verb row — a fresh pick, so nothing is selected (two C6 boards fit and the card never guesses) and the Flash CTA waits for one. RIGHT: the assistant's chat card — the same panel, the same tiles, the same filter line and \"show all\" escape, the same name field, but pre-filled with the agent's board and name, so the press is ready to arm. One renderer, two starting points: the offer's parameters are drawn the same way wherever they are handed to the user."
)]
fn flash_card_beside_device_picker() -> Element {
    let offer = foreign_flash();
    rsx! {
        section { class: "tw:flex tw:min-h-[620px] tw:flex-wrap tw:items-start tw:gap-6 tw:p-4",
            div { class: "tw:grid tw:w-[560px] tw:max-w-full tw:gap-2",
                p { class: LABEL_CLASS, "Device card · board pick" }
                div { class: "tw:flex tw:h-[30px] tw:min-w-0 tw:items-center tw:gap-1.5 tw:overflow-hidden tw:whitespace-nowrap",
                    BoardPickPopover {
                        offer,
                        chip: Some(("esp32c6".to_string(), ChipSource::BootBanner)),
                        mode: BoardPickMode::Row,
                        initially_open: true,
                        on_action: |_| {},
                    }
                }
            }
            div { class: "tw:grid tw:w-[420px] tw:max-w-full tw:gap-2",
                p { class: LABEL_CLASS, "Assistant · chat card" }
                StoryFlashCard { armed: false }
            }
        }
    }
}

/// The agent's Flash card over the foreign board, under the tree that
/// publishes its offer.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn StoryFlashCard(armed: bool) -> Element {
    let offer = foreign_flash();
    let card = agent_card(&offer);
    let mut offers = UiOfferTree::new();
    offers.publish(offer);
    rsx! {
        OffersProvider { offers,
            AgentCardView {
                card,
                chip: Some(("esp32c6".to_string(), ChipSource::BootBanner)),
                armed_preview: armed,
                on_action: |_| {},
            }
        }
    }
}

/// The card the agent's `act` makes: the press of `offer` with its board
/// and a name, handed over at the offer's path with those values as the
/// pre-selection. (A typed name also keeps the capture free of the date the
/// name field's placeholder would show.)
pub(crate) fn agent_card(offer: &UiOffer) -> UiAgentCard {
    let args = OfferArgs::new()
        .with(FLASH_BOARD_PARAM, AGENT_BOARD)
        .with(FLASH_NAME_PARAM, "Shelf lamp");
    let press = offer.press(&args).expect("the agent's board binds");
    UiAgentCard::new(
        "c1",
        press,
        "This board still runs its factory demo; flashing LightPlayer replaces it.",
    )
    .for_offer(offer.path.clone(), args)
}

/// `devices/new-3/flash` on a XIAO still running its factory demo: Lasting,
/// the C6 boards first, every other served board behind "show all".
pub(crate) fn foreign_flash() -> UiOffer {
    flash_pending_offer(&foreign_board(), OfferPath::board(&BoardRef::New(3)))
        .expect("a board with somebody else's firmware flashes")
}

fn foreign_board() -> PendingLinkView {
    PendingLinkView {
        link: DeviceLinkId(4),
        device: DeviceId(3),
        title: "New device".to_string(),
        state_label: "Running other firmware".to_string(),
        detail: None,
        can_adopt: true,
        firmware_face: DeviceFirmwareFace::Foreign {
            label: Some("Seeed factory demo".to_string()),
        },
        detected_chip: Some("esp32c6".to_string()),
        mac: None,
        firmware_blocked: None,
        escapes: vec![DeviceEscape::Forget],
    }
}

/// The board the agent named (the user said "my XIAO").
const AGENT_BOARD: &str = "seeed/xiao-esp32-c6";

const LABEL_CLASS: &str = "tw:m-0 tw:text-[11px] tw:font-semibold tw:uppercase tw:tracking-wide tw:text-subtle-foreground";
