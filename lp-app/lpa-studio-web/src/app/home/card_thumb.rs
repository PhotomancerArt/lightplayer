//! Card thumbnails: a layered stack under the live GPU gallery.
//!
//! Layers, top to bottom (gpu-live-gallery P4 + the M6 primary-visual
//! coordination):
//!
//! 1. **Lamp field** — for a control-first project (its root scope resolves
//!    `control.out`, answered by the engine), the fixture's own lamps drawn
//!    with `LampView`, over the raster it replaces. Same rule as the
//!    editor's module-face hero: a project that drives lamps leads with
//!    them. Present only once a drawable output frame has landed, which is
//!    also its reveal.
//! 2. **Live canvas** — mounted when the card has a preview source,
//!    revealed once the `PreviewHost` slot presents its first frame. The
//!    presented channel is bus `visual.out`, which IS the M6 "primary
//!    visual" contract (the engine resolves the highest-priority
//!    provider), so cards never re-derive which product is a project's
//!    face.
//! 3. **Poster** — this session's captured frame for the card's preview
//!    source, shown as soon as one exists ([`ThumbMode::PosterFirst`]).
//!    It is what makes a gallery card stable: the picture arrives without
//!    a running slot, and on a revisit it is there on the first render.
//!    Every quadrant has a capture path, including the shader-only GPU
//!    tier (worker-side texture readback; see
//!    `docs/adr/2026-08-14-poster-first-gallery-previews.md`). Sourceless
//!    (and hidden) only on a card that genuinely has none yet — including
//!    every story. The same `<img>` is M6's save-time snapshot seam.
//! 4. **Gradient base** — the deterministic identity gradient with the
//!    name's initial: the placeholder before the first present, the
//!    stories' whole face, and the fallback when previews fail.
//!
//! A corner badge surfaces failures only ([`ThumbPreviewBadge::issue`]):
//! the granted tier stays log/wire-visible per the fidelity-tiers ADR's
//! decision-4 note, but a browsing surface doesn't announce the normal.

use dioxus::prelude::*;
use lpa_studio_core::{PreviewSource, UiControlProductPreview};

use crate::app::home::gallery_preview::{ThumbMode, ThumbPreviewBadge, use_thumb_preview};
use crate::app::node::lamp_view::LampView;

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
    /// How long this card is willing to render. Gallery cards opt into
    /// [`ThumbMode::PosterFirst`] — a picture, then nothing; the default
    /// keeps the always-live behavior for any consumer that has not.
    #[props(default = ThumbMode::Live)]
    mode: ThumbMode,
    /// Story/test injection: render this badge statically, without any
    /// PreviewHost. Overrides the live badge when both exist.
    #[props(default)]
    static_badge: Option<ThumbPreviewBadge>,
    /// Story/test injection: draw this lamp field, without any PreviewHost
    /// (stories lease no slot, so a control-first thumb has no other way to
    /// pose). Overrides the live one when both exist.
    #[props(default)]
    static_lamps: Option<UiControlProductPreview>,
    /// Story/test injection: show this poster image (a data URL), without
    /// any PreviewHost or capture — the poster-first states are otherwise
    /// unposable statically, since a real poster is always a captured
    /// frame. Overrides the live one when both exist.
    #[props(default)]
    static_poster: Option<String>,
) -> Element {
    let preview = use_thumb_preview(source, mode);
    let badge = static_badge
        .or(preview.badge)
        .and_then(ThumbPreviewBadge::issue);
    let lamps = static_lamps.or(preview.lamps);
    let poster = static_poster.or(preview.poster);
    let style = thumb_swatch_style(&seed, muted);
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
            class: "tw:relative tw:aspect-[4/3] tw:w-full tw:overflow-hidden",
            // base layer: identity gradient + the name's initial
            div {
                class: "tw:absolute tw:inset-0 tw:flex tw:items-center tw:justify-center",
                style: "{style}",
                span { class: initial_class, "{initial}" }
            }
            // Poster layer (and M6's snapshot seam): this session's
            // captured frame, shown as soon as there is one. It sits
            // UNDER the live canvas on purpose — hover-to-play (P4) swaps
            // motion in over a picture that is already there, so the
            // exchange can never blank the card.
            img {
                class: thumb_poster_class(poster.is_some()),
                src: poster,
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
                    height: "192",
                    class: thumb_canvas_class(canvas.revealed),
                }
            }
            // lamp layer: a control-first project's own fixture, over the
            // raster it replaces. Black behind it (like the device card's ▶
            // frame) because the lamp canvas is transparent where nothing is
            // lit, and lamps must screen-blend against dark rather than
            // against the identity gradient. Inset so an edge lamp is not
            // clipped by the thumb's rounded corner.
            if let Some(lamps) = lamps {
                div { class: "tw:absolute tw:inset-0 tw:bg-black",
                    div { class: "tw:absolute tw:inset-[6%]",
                        LampView { preview: lamps }
                    }
                }
            }
            // top-LEFT: the ⋯ menu owns the top-right corner on the
            // full-art card face (package_card.rs), and the two must
            // never collide
            if let Some(badge) = badge {
                span {
                    class: "tw:absolute tw:left-1.5 tw:top-1.5 tw:rounded-sm tw:border tw:bg-background/70 tw:px-1 tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-4 {thumb_badge_class(&badge)}",
                    title: thumb_badge_title(&badge),
                    {thumb_badge_text(&badge)}
                }
            }
        }
    }
}

/// The poster layer: structurally present always (M6's snapshot seam), but
/// only displayed once it has an image — an `<img>` with no `src` would
/// otherwise paint the broken-image glyph over the gradient.
fn thumb_poster_class(has_poster: bool) -> &'static str {
    if has_poster {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:object-cover"
    } else {
        "tw:absolute tw:inset-0 tw:hidden tw:h-full tw:w-full tw:object-cover"
    }
}

/// The live canvas layer: hidden (poster or gradient shows) until the first
/// frame reaches it, then revealed with a short fade — and faded back out
/// the same way when a hover lease ends, so motion arrives and leaves over
/// the poster instead of cutting.
fn thumb_canvas_class(revealed: bool) -> &'static str {
    if revealed {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-100 tw:transition-opacity tw:duration-200"
    } else {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-0 tw:transition-opacity tw:duration-200"
    }
}

/// Badge chip styling per state — preview-lab's tier vocabulary (GPU wears
/// the strong border, CPU the muted one) in gallery-sized clothes; errors
/// read as errors.
fn thumb_badge_class(badge: &ThumbPreviewBadge) -> &'static str {
    match badge {
        ThumbPreviewBadge::Gpu => "tw:border-border-strong tw:text-strong-foreground",
        ThumbPreviewBadge::Cpu { .. } => "tw:border-border-strong tw:text-muted-foreground",
        ThumbPreviewBadge::Error { .. } => {
            "tw:border-border-strong tw:text-status-error-foreground"
        }
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

/// The identity-gradient style for a seed — the thumb's base layer, and
/// the small project-chip swatch on device cards (same identity, same
/// colors, chip-sized).
pub(crate) fn thumb_swatch_style(seed: &str, muted: bool) -> String {
    let (hue_a, hue_b) = thumb_hues(seed);
    let (saturation, lightness) = if muted { (12, 16) } else { (42, 22) };
    format!(
        "background: linear-gradient(135deg, hsl({hue_a} {saturation}% {lightness}%), hsl({hue_b} {}% {}%));",
        saturation + 12,
        lightness + 10,
    )
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
        assert_eq!(thumb_hues("prja"), thumb_hues("prja"));
        assert_ne!(thumb_hues("prja"), thumb_hues("prjb"));
        let (a, b) = thumb_hues("prja");
        assert!(a < 360 && b < 360);
    }
}
