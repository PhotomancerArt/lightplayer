//! Landing-page stories: the brand hero in its FALLBACK state.
//!
//! Stories lease no preview slot (the story book provides
//! `StaticThumbPreviews`, which clears every preview source), so the hero
//! captures deterministically: spill bloom + clipped identity gradient,
//! no canvas, no badge. That is deliberate and load-bearing — a live
//! canvas here would race capture, and the fit-reconciliation ready-gate
//! would time out waiting for a frame that story mode never produces.
//!
//! The capture harness freezes CSS animations before mount, so the
//! wordmark lands on its canonical rest frame (rainbow at the left edge),
//! same as the `logo_mark_stories` lockups.

use dioxus::prelude::*;
use lpa_studio_core::{ControllerId, HOME_NODE_ID, HomeOp, UiAction};
use lpa_studio_web_story_macros::story;

use crate::app::home::HomePage;
use crate::app::home::brand_hero::BrandHero;
use crate::app::home::project_opening_frame::OpenFailureNotice;

#[story(
    description = "The landing page: brand hero (the mark's triangle as a window onto a live shader — here the fallback identity gradient with its Spill bloom, since stories run no engine), wordmark, tagline, the \"Edit the logo\" pill (inert here — a story has no dispatcher), the three dive-in cards, and the example row with its Explore-all link (cards render their poster/seeded thumbs — stories lease no previews). The shared-`/p/` line renders nothing without context."
)]
fn landing() -> Element {
    rsx! {
        section { class: "tw:p-4",
            HomePage {}
        }
    }
}

#[story(
    description = "A `/p/…` View link that reached Home and failed to open (a sim that would not boot, say): the same `OpenFailureNotice` Explore shows for its own failed opens, with Retry and a way back. Before this it just vanished — Home has no `state` seam like the opening frame's, since this reads the core's own open-stage signal directly, so the story poses the notice with the layout around it instead."
)]
fn landing_failed_view_link() -> Element {
    rsx! {
        section { class: "tw:flex tw:min-h-[60vh] tw:flex-col tw:items-center tw:justify-center tw:gap-8 tw:p-4 tw:text-center",
            OpenFailureNotice {
                message: "engine wasm fetch/compile failed: NetworkError when attempting to fetch resource"
                    .to_string(),
                retry: Some(
                    UiAction::from_op(
                        ControllerId::new(HOME_NODE_ID),
                        HomeOp::OpenExample { id: "catalog/fyeah-sign".to_string() },
                    ),
                ),
                on_action: None,
            }
            BrandHero {}
        }
    }
}

#[story(
    description = "The hero alone, on the dark stage the brand assets use: triangle silhouette at the hero fillet ratio (0.10, tighter than the mark's 0.16), the Spill bloom escaping past its edges, and the 40px rainbow wordmark. The hero alone carries no controls — the way into the editor is the pill under the tagline on the landing page."
)]
fn brand_hero() -> Element {
    rsx! {
        div { class: "tw:flex tw:items-center tw:justify-center tw:bg-terminal tw:p-12",
            BrandHero {}
        }
    }
}
