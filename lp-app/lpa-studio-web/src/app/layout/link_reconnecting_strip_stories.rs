//! The "Reconnecting…" strip (plan D13) in both of its causes. The strip in
//! place, over the project page, is `workbench_link_reconnecting`.

use dioxus::prelude::*;
use lpa_studio_core::{LinkTrouble, UiLensReconnecting};
use lpa_studio_web_story_macros::story;

use crate::app::layout::LinkReconnectingStrip;

#[story(
    description = "The Reconnecting strip, both causes (plan D13). Top: the board went quiet on an established link (an lp-link stall). Bottom: the link's session restarted and the board has not said hello yet. Neutral tint, a slowly turning icon, no buttons — a blip the link recovers from on its own is not an error and asks nothing of the user. It sits over the project page, which stays as it was; it goes when the board is heard again, and a board that does not come back within 20 s closes the editor the old way."
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
        }
    }
}
