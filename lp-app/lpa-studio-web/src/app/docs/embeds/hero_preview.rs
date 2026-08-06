//! `hero-preview`: the article's opening shot — a `PreviewHost` lease,
//! article-wide.
//!
//! Tier 1 of the embed ladder: no controller, no lens, no dispatch. It
//! leases a slot from the **same page-wide host the gallery thumbs use**
//! (`super::super::super::home::gallery_preview`), so a docs page costs
//! one more slot in an existing pool rather than a second preview
//! infrastructure. The lease handle `Drop`s cleanly with the component.
//!
//! It is sized as a hero (article width, 16:9) and asks for a smoother
//! present cadence than a thumb: this is the surface a reader is actually
//! watching, not a card in a grid.
//!
//! The tier badge is the gallery's, quieted: an article must still say out
//! loud when it fell back to the CPU tier or failed (fidelity-tiers ADR —
//! never silent), but it should not shout mid-sentence.

use dioxus::prelude::*;
use lpa_studio_core::PreviewSource;

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::gallery_preview::{ThumbPreviewBadge, use_preview_lease};

/// Present cadence for a hero: the reader is watching this one.
const HERO_FPS: f32 = 30.0;

/// The article's hero: a live example running on a leased preview slot.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DocsHeroPreview(
    /// Embedded example id (`examples/plasma`).
    example_id: String,
) -> Element {
    let preview = use_preview_lease(
        Some(PreviewSource::Example(example_id.clone())),
        Some(HERO_FPS),
    );
    let style = thumb_swatch_style(&example_id, false);

    rsx! {
        div {
            id: "{preview.frame_id}",
            class: "tw:relative tw:mb-1.5 tw:aspect-video tw:w-full tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:last:mb-0",
            // The identity gradient is the base layer and the fallback,
            // exactly as on a gallery card.
            div { class: "tw:absolute tw:inset-0", style: "{style}" }
            if let Some(canvas) = preview.canvas {
                canvas {
                    key: "{canvas.id}",
                    id: "{canvas.id}",
                    width: "640",
                    height: "360",
                    class: hero_canvas_class(canvas.revealed),
                }
            }
            if let Some(badge) = preview.badge {
                span {
                    class: "tw:absolute tw:bottom-1.5 tw:right-1.5 tw:rounded-sm tw:border tw:bg-background/60 tw:px-1 tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-4 {hero_badge_class(&badge)}",
                    title: hero_badge_title(&badge),
                    {hero_badge_text(&badge)}
                }
            }
        }
    }
}

/// Revealed only once a frame has landed, so the gradient covers the boot.
fn hero_canvas_class(revealed: bool) -> &'static str {
    if revealed {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-100 tw:transition-opacity tw:duration-300"
    } else {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-0"
    }
}

/// The gallery's tier vocabulary, at article volume.
fn hero_badge_class(badge: &ThumbPreviewBadge) -> &'static str {
    match badge {
        ThumbPreviewBadge::Gpu => "tw:border-border-muted tw:text-dim-foreground",
        ThumbPreviewBadge::Cpu { .. } => "tw:border-border-muted tw:text-muted-foreground",
        ThumbPreviewBadge::Error { .. } => "tw:border-border-strong tw:text-error-foreground",
    }
}

fn hero_badge_text(badge: &ThumbPreviewBadge) -> &'static str {
    match badge {
        ThumbPreviewBadge::Gpu => "GPU",
        ThumbPreviewBadge::Cpu { .. } => "CPU",
        ThumbPreviewBadge::Error { .. } => "!",
    }
}

fn hero_badge_title(badge: &ThumbPreviewBadge) -> String {
    match badge {
        ThumbPreviewBadge::Gpu => "Running live on your GPU".to_string(),
        ThumbPreviewBadge::Cpu { reason: None } => "Running live on the CPU tier".to_string(),
        ThumbPreviewBadge::Cpu {
            reason: Some(reason),
        } => format!("Running on the CPU tier (GPU unavailable: {reason})"),
        ThumbPreviewBadge::Error { reason } => format!("This preview stopped: {reason}"),
    }
}
