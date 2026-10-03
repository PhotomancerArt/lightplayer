//! The "Reconnecting…" card in all three of its causes. The card in place,
//! on its curtain over the project page, is `workbench_link_reconnecting`.

use dioxus::prelude::*;
use lpa_studio_core::{LinkTrouble, UiLensReconnecting};
use lpa_studio_web_story_macros::story;

use crate::app::layout::LinkReconnectingStrip;

#[story(
    description = "The Reconnecting card, all three causes. Top: the board stopped answering on an established link (an lp-link stall). Middle: the link's session was reset and the board has not said hello yet. Bottom: the link went away altogether (a Bluetooth drop, a USB cable re-seated) and the editor is held until the same board is back on a new link (defect 2026-10-02). In the app the card floats over the page on a dim curtain that holds the page still (`workbench_link_reconnecting`), fading in after a short beat and out when the board is back. No buttons: there is nothing to do but wait a moment, and a board that does not come back within the grace closes the editor the old way."
)]
fn link_reconnecting_strip() -> Element {
    rsx! {
        section { class: "tw:grid tw:w-[760px] tw:gap-3 tw:p-4",
            LinkReconnectingStrip {
                reconnecting: UiLensReconnecting::new("Porch sign", LinkTrouble::Quiet),
            }
            LinkReconnectingStrip {
                reconnecting: UiLensReconnecting::new("Porch sign", LinkTrouble::Restarted),
            }
            LinkReconnectingStrip { reconnecting: UiLensReconnecting::link_lost("Porch sign") }
        }
    }
}
