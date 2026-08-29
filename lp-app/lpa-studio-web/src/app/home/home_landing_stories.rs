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
use lpa_studio_web_story_macros::story;

use crate::app::home::HomePage;
use crate::app::home::brand_hero::BrandHero;

#[story(
    description = "The landing page: brand hero (the mark's triangle as a window onto a live shader — here the fallback identity gradient with its Spill bloom, since stories run no engine), wordmark, tagline, the three dive-in cards, and the curated example row with its Explore-all link (cards render their poster/seeded thumbs — stories lease no previews). The shared-`/p/` line renders nothing without context."
)]
fn landing() -> Element {
    rsx! {
        section { class: "tw:p-4",
            HomePage {}
        }
    }
}

#[story(
    description = "The hero alone, on the dark stage the brand assets use: triangle silhouette at the hero fillet ratio (0.10, tighter than the mark's 0.16), the Spill bloom escaping past its edges, the 40px rainbow wordmark, and the quiet edit affordance — inert here because a story has no dispatcher."
)]
fn brand_hero() -> Element {
    rsx! {
        div { class: "tw:flex tw:items-center tw:justify-center tw:bg-terminal tw:p-12",
            BrandHero {}
        }
    }
}
