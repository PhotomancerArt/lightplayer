//! Card thumbnails: a layered stack under the live GPU gallery.
//!
//! Layers, top to bottom (gpu-live-gallery P4 + the M6 primary-visual
//! coordination):
//!
//! 1. **Live canvas** — mounted when the card has a preview source,
//!    revealed once the `PreviewHost` slot presents its first frame. The
//!    presented channel is bus `visual.out`, which IS the M6 "primary
//!    visual" contract (the engine resolves the highest-priority
//!    provider), so cards never re-derive which product is a project's
//!    face.
//! 2. **Snapshot seam** — a structurally present `<img>` for M6's
//!    save-time capture; sourceless (and hidden) until that lands.
//! 3. **Gradient base** — the deterministic identity gradient with the
//!    name's initial: the placeholder before the first present, the
//!    stories' whole face, and the fallback when previews fail.
//!
//! A corner badge surfaces the granted tier and failures (fidelity-tiers
//! ADR: never silent).

use dioxus::prelude::*;
use lpa_studio_core::PreviewSource;

use crate::app::home::gallery_preview::{ThumbPreviewBadge, use_thumb_preview};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardThumb(
    seed: String,
    label: String,
    #[props(default = false)] muted: bool,
    /// Live preview content for this thumb. `None` (stories, device-less
    /// contexts) renders the static gradient stack — no host, no canvas.
    #[props(default)]
    source: Option<PreviewSource>,
    /// Story/test injection: render this badge statically, without any
    /// PreviewHost. Overrides the live badge when both exist.
    #[props(default)]
    static_badge: Option<ThumbPreviewBadge>,
) -> Element {
    let preview = use_thumb_preview(source);
    let badge = static_badge.or(preview.badge);
    let (hue_a, hue_b) = thumb_hues(&seed);
    let (saturation, lightness) = if muted { (12, 16) } else { (42, 22) };
    let style = format!(
        "background: linear-gradient(135deg, hsl({hue_a} {saturation}% {lightness}%), hsl({hue_b} {}% {}%));",
        saturation + 12,
        lightness + 10,
    );
    // dated slugs (2026-07-09-1421-basic) take their initial from the
    // label part, not the stamp
    let initial = label
        .chars()
        .find(|c| c.is_alphabetic())
        .or_else(|| label.chars().next())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let initial_class = if muted {
        "tw:text-2xl tw:font-extrabold tw:text-white/20"
    } else {
        "tw:text-2xl tw:font-extrabold tw:text-white/40"
    };

    rsx! {
        div {
            id: "{preview.frame_id}",
            class: "tw:relative tw:h-24 tw:w-full tw:overflow-hidden tw:rounded-t-md",
            // base layer: identity gradient + initial
            div {
                class: "tw:absolute tw:inset-0 tw:flex tw:items-center tw:justify-center",
                style: "{style}",
                span { class: initial_class, "{initial}" }
            }
            // Snapshot seam (M6 coordination): the save-time capture layer.
            // Structurally present so M6 only has to hand it a source
            // (LibraryStore package metadata, never the deploy-synced file
            // tree); sourceless and hidden until that capture lands.
            img {
                class: "tw:absolute tw:inset-0 tw:hidden tw:h-full tw:w-full tw:object-cover",
                alt: "",
            }
            // live layer: the PreviewHost canvas, revealed after the first
            // presented frame; keyed so a bumped generation mounts a FRESH
            // element (a GPU-tier canvas is consumed by its transfer)
            if let Some(canvas) = preview.canvas {
                canvas {
                    key: "{canvas.id}",
                    id: "{canvas.id}",
                    width: "256",
                    height: "96",
                    class: thumb_canvas_class(canvas.revealed),
                }
            }
            if let Some(badge) = badge {
                span {
                    class: "tw:absolute tw:right-1.5 tw:top-1.5 tw:rounded-sm tw:border tw:bg-background/70 tw:px-1 tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-4 {thumb_badge_class(&badge)}",
                    title: thumb_badge_title(&badge),
                    {thumb_badge_text(&badge)}
                }
            }
        }
    }
}

/// The live canvas layer: hidden (gradient shows) until the first frame
/// reaches it, then revealed with a short fade.
fn thumb_canvas_class(revealed: bool) -> &'static str {
    if revealed {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-100 tw:transition-opacity tw:duration-200"
    } else {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-0"
    }
}

/// Badge chip styling per state — preview-lab's tier vocabulary (GPU wears
/// the accent border, CPU the muted one) in gallery-sized clothes; errors
/// read as errors.
fn thumb_badge_class(badge: &ThumbPreviewBadge) -> &'static str {
    match badge {
        ThumbPreviewBadge::Gpu => "tw:border-accent-border tw:text-strong-foreground",
        ThumbPreviewBadge::Cpu { .. } => "tw:border-border-strong tw:text-muted-foreground",
        ThumbPreviewBadge::Error { .. } => "tw:border-border-strong tw:text-error-foreground",
    }
}

/// Badge chip text (compact: the tier name, or `!` for failures).
fn thumb_badge_text(badge: &ThumbPreviewBadge) -> &'static str {
    match badge {
        ThumbPreviewBadge::Gpu => "GPU",
        ThumbPreviewBadge::Cpu { .. } => "CPU",
        ThumbPreviewBadge::Error { .. } => "!",
    }
}

/// Badge tooltip: the fallback / failure reason when there is one.
fn thumb_badge_title(badge: &ThumbPreviewBadge) -> String {
    match badge {
        ThumbPreviewBadge::Gpu => "Live preview on the GPU tier".to_string(),
        ThumbPreviewBadge::Cpu { reason: None } => "Live preview on the CPU tier".to_string(),
        ThumbPreviewBadge::Cpu {
            reason: Some(reason),
        } => format!("CPU tier (GPU unavailable: {reason})"),
        ThumbPreviewBadge::Error { reason } => format!("Preview failed: {reason}"),
    }
}

/// Two stable hues from the seed (uid or name): FNV-1a, split into two
/// angles far enough apart to read as a gradient.
fn thumb_hues(seed: &str) -> (u16, u16) {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in seed.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let hue_a = (hash % 360) as u16;
    let hue_b = ((hash >> 16) % 360) as u16;
    (hue_a, hue_b)
}

#[cfg(test)]
mod tests {
    use super::thumb_hues;

    #[test]
    fn hues_are_stable_and_seed_dependent() {
        assert_eq!(thumb_hues("prj_a"), thumb_hues("prj_a"));
        assert_ne!(thumb_hues("prj_a"), thumb_hues("prj_b"));
        let (a, b) = thumb_hues("prj_a");
        assert!(a < 360 && b < 360);
    }
}
