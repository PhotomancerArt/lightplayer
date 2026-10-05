//! [`OfferParamsForm`]: any offer's parameters as the generic controls,
//! with [`OfferPressButton`] under them — the renderer the app agent's chat
//! card hands the user a parameterised verb with.

use dioxus::prelude::*;
use lpa_studio_core::{
    BoardRef, DeviceEscape, DeviceFirmwareFace, DeviceId, DeviceLinkId, OfferArgs, OfferPath,
    PendingLinkView, UiOffer, flash_pending_offer, new_sim_offer,
};
use lpa_studio_web_story_macros::story;

use crate::core::{OfferParamsForm, OfferPressButton};

#[story(
    description = "The generic parameter form over three offers core publishes. FLASH A BLANK CHIP (`devices/new-3/flash`): the `board` choice as option rows — the chip-narrowed boards only, because `all_boards` (the toggle at the bottom) is off — the optional `name` field with core's placeholder, and the press, Routine on a blank chip. Nothing is preselected (two C6 boards fit), so the press waits, saying why in core's words. START A BOARD HERE (`devices/new-sim`): two choices, `board` and `runtime`, pre-filled here the way the app agent would hand them over (a XIAO, emulated) — so the press is live. Every value is an `OfferArgs` the form writes; the press is the offer's own binding, never an op the web builds."
)]
fn generic_form() -> Element {
    let flash = flash_pending_offer(&blank_chip(), OfferPath::board(&BoardRef::New(3)))
        .expect("a blank chip flashes");
    let sim = new_sim_offer();
    let sim_args = OfferArgs::new()
        .with("board", "seeed/xiao-esp32-c6")
        .with("backing", "emu");
    rsx! {
        div { class: "tw:grid tw:grid-cols-[repeat(2,360px)] tw:items-start tw:gap-6 tw:p-4",
            FormCard { offer: flash, args: OfferArgs::new() }
            FormCard { offer: sim, args: sim_args }
        }
    }
}

/// One offer's form and press, in a card, over its own args.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn FormCard(offer: UiOffer, args: OfferArgs) -> Element {
    let args = use_signal(|| args);
    let current = args.read().clone();
    rsx! {
        section { class: "tw:grid tw:gap-3 tw:rounded-md tw:border tw:border-border tw:bg-card tw:p-3",
            p { class: "tw:m-0 tw:font-mono tw:text-[0.68rem] tw:text-subtle-foreground", "{offer.path}" }
            OfferParamsForm { offer: offer.clone(), args }
            OfferPressButton { offer, args: current, on_action: |_| {} }
        }
    }
}

fn blank_chip() -> PendingLinkView {
    PendingLinkView {
        link: DeviceLinkId(4),
        device: DeviceId(3),
        title: "New device".to_string(),
        state_label: "Blank flash — needs firmware".to_string(),
        detail: None,
        can_adopt: true,
        firmware_face: DeviceFirmwareFace::Blank,
        detected_chip: Some("esp32c6".to_string()),
        mac: None,
        firmware_blocked: None,
        escapes: vec![DeviceEscape::Forget],
    }
}
