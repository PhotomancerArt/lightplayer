//! Opening-frame stories: every state a click can wait in (P6, Q5).
//!
//! The frame reads live page signals, so each story hands it an explicit
//! `state` instead — the same enum the poll loop produces. That is the
//! whole point of the seam: these five pages ARE the state matrix, and a
//! state that cannot be posed here is a state nobody can review.

use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, HOME_NODE_ID, HomeOp, UiAction};
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
                id: "examples/fyeah-sign".to_string(),
            },
        ),
    })
}

/// The frame on the canvas the shell gives it.
fn frame(state: OpeningState) -> Element {
    rsx! {
        section { class: "tw:p-4",
            ProjectOpeningFrame { state }
        }
    }
}
