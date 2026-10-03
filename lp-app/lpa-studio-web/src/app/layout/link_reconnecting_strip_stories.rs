//! The "Reconnecting…" strip (plan D13) in all three of its causes. The strip in
//! place, over the project page, is `workbench_link_reconnecting`.

use dioxus::prelude::*;
use lpa_studio_core::{LinkTrouble, UiLensReconnecting};
use lpa_studio_web_story_macros::story;

use crate::app::layout::LinkReconnectingStrip;

#[story(
    description = "The Reconnecting strip, all three causes. Top: the board went quiet on an established link (an lp-link stall). Middle: the link's session restarted and the board has not said hello yet. Bottom: the link went away altogether — a Bluetooth drop, a USB cable re-seated — and the editor is held until the same board is back on a new link (defect 2026-10-02). Neutral tint, a slowly turning icon, no buttons — a blip the link recovers from on its own is not an error and asks nothing of the user. It sits over the project page or Play, which stays as it was; it goes when the board is heard again, and a board that does not come back within the grace (20 s for a stall or reset, 45 s of awake time for a dropped link) closes the editor the old way."
)]
fn link_reconnecting_strip() -> Element {
    rsx! {
        section { class: "tw:grid tw:w-[760px] tw:gap-1 tw:p-4",
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
