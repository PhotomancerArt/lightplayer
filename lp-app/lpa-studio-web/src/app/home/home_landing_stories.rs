//! Home page stories, first visit: the page's frame with nothing on it yet.
//!
//! (The file keeps its old name: a story's id is its file path plus its
//! function name, so a rename would turn a before/after into a delete and
//! an add. The page itself lives in `app/home/page/`.)
//!
//! Stories lease no preview slot (the story book provides
//! `StaticThumbPreviews`, which clears every preview source), so the example
//! cards render their poster/seeded thumbs deterministically. The Connect a
//! board section is pinned to Chrome on a computer so a capture never reads
//! the story server's own browser or address.

use dioxus::prelude::*;
use lpa_studio_core::{
    BluetoothReach, ControllerId, DeviceRosterView, HOME_NODE_ID, HomeOp, UiAction, UiHomeView,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::connect_board::connect_board_section::ConnectStoryPins;
use crate::app::home::device_offer_story_fixtures::StoryHomePage;
use crate::app::home::project_opening_frame::OpenFailureNotice;

#[story(
    description = "The home page for a first visit: nothing to filter, so no tab strip and no cards/list switch. Connect a board comes first — USB, Bluetooth and Network squares and \"start a board here\" under them, with one hint line after the squares (\"No board? Try an example ↓\") — then Other projects, which is only its add row (New · Import · Paste) with nothing in the library yet, then the examples (Example projects, Example patterns; cards render their poster/seeded thumbs, stories lease no previews) and the quiet footer. No hero, no doors, no chat box: the top bar's chat button and ⌘K are how the assistant is reached. The shared-`/p/` line and the sign-in line render nothing without their context."
)]
fn landing() -> Element {
    rsx! {
        section { class: "tw:p-4",
            StoryHomePage {
                home: newcomer_home(),
                connect_pins: pins(),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "A `/p/…` View link that reached Home and failed to open (a sim that would not boot, say): the same `OpenFailureNotice` Explore shows for its own failed opens, with Retry and a way back, above the page. Home has no `state` seam like the opening frame's, since this reads the core's own open-stage signal directly, so the story poses the notice with the page below it."
)]
fn landing_failed_view_link() -> Element {
    rsx! {
        section { class: "tw:grid tw:content-start tw:gap-7 tw:p-4",
            OpenFailureNotice {
                message: "engine wasm fetch/compile failed: NetworkError when attempting to fetch resource"
                    .to_string(),
                retry: UiAction::from_op(
                    ControllerId::new(HOME_NODE_ID),
                    HomeOp::OpenExample { id: "catalog/fyeah-sign".to_string() },
                ),
                on_action: None,
            }
            StoryHomePage {
                home: newcomer_home(),
                connect_pins: pins(),
                on_action: |_| {},
            }
        }
    }
}

/// A first visit: a browser that can reach a port, no board, no project.
///
/// The sections are core's: `StoryHomePage` builds them from this roster and
/// library, and a first visit is what core makes of nothing.
pub(crate) fn newcomer_home() -> UiHomeView {
    UiHomeView {
        projects: Vec::new(),
        examples: Vec::new(),
        devices: DeviceRosterView {
            transport_available: true,
            usb_available: true,
            ..DeviceRosterView::default()
        },
        sections: Default::default(),
        library_available: true,
        opening: None,
        issue: None,
    }
}

/// Chrome on a computer, with the product's own address in any copy line.
pub(crate) fn pins() -> ConnectStoryPins {
    ConnectStoryPins {
        ble_reach: Some(BluetoothReach::Ready),
        page_url: Some("https://lightplayer.app/".to_string()),
        ..ConnectStoryPins::default()
    }
}
