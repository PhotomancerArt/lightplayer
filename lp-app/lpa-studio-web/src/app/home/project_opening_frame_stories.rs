//! Opening-frame stories: every state a click can wait in (P6, Q5).
//!
//! The frame reads live page signals, so each story hands it an explicit
//! `state` instead — the same enum the poll loop produces. That is the
//! whole point of the seam: these five pages ARE the state matrix, and a
//! state that cannot be posed here is a state nobody can review.

use dioxus::prelude::*;
use lpa_studio_core::{
    ControllerId, DeviceId, DeviceOpenProgress, DeviceOpenStep, DeviceWait, DeviceWaitReason,
    HOME_NODE_ID, HomeOp, OpenDevice, UiAction,
};
use lpa_studio_web_story_macros::story;

use crate::app::home::ProjectOpeningFrame;
use crate::app::home::project_opening_frame::{EnginePhase, OpeningState};

// Named `default`, not `overview`: `<component>/overview` is reserved for the
// generated composite page, which shadowed this story outright — the id
// resolved to the composite, so the authored page was unreachable and its
// baseline was a composite capture. See `OVERVIEW_ID_SUFFIX` in
// `stories/story_book.rs`.
#[story(
    description = "The calm skeleton: a project reload, or a fast open whose phases all passed inside the ~150 ms label debounce. Most opens never show anything else, which is why this state has to look like progress rather than like a stall."
)]
fn default() -> Element {
    frame(OpeningState::Opening)
}

#[story(
    description = "A cold, throttled load: the engine binary is the multi-MB wait, and it is the one phase with a real quantity, so it is the one phase with a bar. The percentage is the actual byte progress from the page-side fetch — no spinner standing in for a number we have."
)]
fn downloading_engine() -> Element {
    frame(OpeningState::DownloadingEngine {
        received_bytes: 4_089_446.0,
        total_bytes: Some(9_437_184.0),
    })
}

#[story(
    description = "The engine is in hand and coming up. The phase word is the boot protocol's own (`gpu-init` here) rendered as work rather than as a wire token — the bar is gone because nothing here is a quantity, and inventing one would be worse than the pulsing dot."
)]
fn starting_engine() -> Element {
    frame(OpeningState::StartingEngine {
        phase: EnginePhase::GpuInit,
    })
}

#[story(
    description = "The runtime is up; the remaining work is the project itself — read, migrate if it is an older format, deploy. Usually a blink; visible on a large project or a cold OPFS."
)]
fn preparing_project() -> Element {
    frame(OpeningState::PreparingProject)
}

#[story(
    description = "The rare one worth naming: this tab's own cloud sync holds the project lock for a local snapshot, and the open is waiting it out. This used to surface as \"this project is open in another tab\" with one tab open — the state exists so that lie cannot come back."
)]
fn waiting_for_sync() -> Element {
    frame(OpeningState::WaitingForSync)
}

#[story(
    description = "The dead end, with both ways out. This state is what replaced the eternal skeleton: the error is the mapped message the console shows, Retry re-runs exactly the open that failed (no reload), and Back to Explore is the exit for someone who would rather click something else."
)]
fn failed() -> Element {
    frame(OpeningState::Failed {
        message: "engine wasm fetch/compile failed: NetworkError when attempting to fetch resource"
            .to_string(),
        retry: UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenExample {
                id: "catalog/fyeah-sign".to_string(),
            },
        ),
        device: None,
        needs_unlock: false,
    })
}

#[story(
    description = "A `/p/…?on=mac:` address loaded fresh in a browser that forgets Web Serial grants on reload (Brave): the board is remembered but this page has no port for it. Only a click can fix that — `requestPort()` needs a user gesture — so the page offers exactly that click instead of waiting silently (Yona, 2026-09-24)."
)]
fn board_not_connected() -> Element {
    frame(OpeningState::WaitingForDevice(DeviceWait {
        device: choker(),
        reason: DeviceWaitReason::NotConnected,
    }))
}

#[story(
    description = "Connected, but the board has not said hello — it just reset, or it stopped answering. The stall note starts counting after a few seconds, and Reset the board is the way out that used to need a browser refresh."
)]
fn board_not_answering() -> Element {
    stalled(
        OpeningState::WaitingForDevice(DeviceWait {
            device: choker(),
            reason: DeviceWaitReason::Identifying,
        }),
        23,
    )
}

#[story(
    description = "The project going onto the board: the bar is the real byte count of acknowledged writes, the one quantity a board open has."
)]
fn board_uploading() -> Element {
    frame(OpeningState::OnDevice(DeviceOpenProgress {
        device: choker(),
        step: DeviceOpenStep::Uploading {
            sent_bytes: 23_552,
            total_bytes: 43_741,
        },
    }))
}

#[story(
    description = "The long step on a C6: the board compiles every shader on its own JIT, and there is no quantity to show. After a few seconds the frame says which step and for how long, so a slow compile does not read as a crash."
)]
fn board_loading_stalled() -> Element {
    stalled(
        OpeningState::OnDevice(DeviceOpenProgress {
            device: choker(),
            step: DeviceOpenStep::Loading,
        }),
        14,
    )
}

#[story(
    description = "An open that failed on a board names the step it was on (here the load timed out), offers Retry and Reset the board, and goes back to Devices rather than Explore."
)]
fn failed_on_board() -> Element {
    frame(OpeningState::Failed {
        message: "XIAO ESP32-C6 · Sep 24 stopped while loading the project on the board \
                  (compiling its shaders): transport error: Transport error: device did not \
                  respond within 20.0s"
            .to_string(),
        retry: UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenExample {
                id: "catalog/fyeah-sign".to_string(),
            },
        ),
        device: Some(choker()),
        needs_unlock: false,
    })
}

#[story(
    description = "An open the board refused because this Bluetooth link is unlocked for play only. The board is fine, so the way on is Unlock (the sheet asks for an edit password), not Reset the board; Retry once it is unlocked."
)]
fn failed_on_board_needs_unlock() -> Element {
    frame(OpeningState::Failed {
        message: lpa_studio_core::not_permitted_sentence(lpa_studio_core::AccessTier::Edit)
            .to_string(),
        retry: UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenExample {
                id: "catalog/fyeah-sign".to_string(),
            },
        ),
        device: Some(choker()),
        needs_unlock: true,
    })
}

fn choker() -> OpenDevice {
    OpenDevice {
        id: Some(DeviceId(1)),
        uid: "devstory".to_string(),
        name: "XIAO ESP32-C6 · Sep 24".to_string(),
    }
}

/// The frame on the canvas the shell gives it.
fn frame(state: OpeningState) -> Element {
    rsx! {
        section { class: "tw:p-4",
            ProjectOpeningFrame { state }
        }
    }
}

/// The frame with its step held for `secs`.
fn stalled(state: OpeningState, secs: u64) -> Element {
    rsx! {
        section { class: "tw:p-4",
            ProjectOpeningFrame { state, stalled_secs: secs }
        }
    }
}
