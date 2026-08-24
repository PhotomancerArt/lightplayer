//! The landing hero: the brand triangle as a window onto a live shader.
//!
//! The mark's play triangle, blown up to hero scale, is not drawn here —
//! it is a *clip path*. Behind it runs a real engine preview (a
//! `PreviewHost` lease of the same page-wide host the gallery thumbs and
//! the docs hero use), so the first thing the landing says about
//! LightPlayer is said by the engine itself rather than by a picture of
//! it.
//!
//! Layers, back to front (spike `spikes/logo-triangle-chip/index.html` §4,
//! treatment **Spill**):
//!
//! 1. **Spill** — the identity gradient, blurred and slightly enlarged:
//!    colored light escaping past the triangle's edge. It is a still
//!    approximation, on purpose: the engine owns its canvas and there is no
//!    frame we could blur in step with it (plan D1). It paints instantly,
//!    which is the point — the hero is lit before the engine has warmed.
//! 2. **Fallback** — the clipped identity gradient. The reveal base, and
//!    the whole face wherever no preview runs (stories, host builds, a
//!    failed slot). Exactly the docs-hero pattern.
//! 3. **Canvas** — the leased engine canvas, same clip, revealed once a
//!    frame has landed so the gradient covers the boot.
//! 4. **Badge** — the granted tier, quieted. Small, but never silent
//!    (fidelity-tiers ADR).
//!
//! The hero corner ratio is **tighter than the mark's** (0.10 vs 0.16):
//! the fillet that reads as friendly at 22px reads balloon-y at 250px
//! (spike gate-4).
//!
//! This component is the **seed of the fixture-hero** direction — a module
//! panel under the hero, touch and sound driving it (plan
//! `2026-08-24-1100-logo-triangle-chip`, D1). That future swaps this
//! surface's *source*, not its shape, which is why the example id is a
//! constant and why the hero lives in its own module with no control path
//! into the slot.

use dioxus::prelude::*;
use lpa_studio_core::{HomeOp, PreviewSource, UiAction};

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::gallery_preview::{ThumbPreviewBadge, use_preview_lease_raster};
use crate::app::home::package_card::home_action;
use crate::base::logo_mark::{BrandWord, fillet_tri_path};

/// The landing hero's example and cadence. A constant on purpose: the
/// future fixture-hero plan swaps this surface's source, not its shape.
const HERO_EXAMPLE: &str = "examples/plasma";
/// Present cadence for the hero — the visitor is watching this one.
const HERO_FPS: f32 = 30.0;

/// The triangle box in CSS pixels, and the triangle inside it: circumradius
/// and center, from the spike's landing mock (250×232, r = h/2, cx = 0.46w).
const HERO_BOX: (f32, f32) = (250.0, 232.0);
const HERO_TRI: (f32, f32, f32) = (115.0, 116.0, 116.0);
/// Hero-specific fillet ratio (the mark keeps 0.16 — spike gate-4).
const HERO_CORNER_RATIO: f32 = 0.10;
/// Canvas backing store: the box at ~2× device pixels, in the box's own
/// aspect. A 16:9 buffer behind a square-ish clip would waste the pixels
/// the clip throws away and stretch the ones it keeps.
const HERO_CANVAS: (u32, u32) = (512, 476);

/// The landing hero: brand triangle as a live shader window, wordmark
/// under it, and a quiet way into the editor.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BrandHero(#[props(default)] on_action: Option<EventHandler<UiAction>>) -> Element {
    // Raster-first on purpose: examples drive LEDs, so their products are
    // control-first and a plain lease would keep the raster hidden behind a
    // lamp view (G1 finding: the triangle sat dark on the gradient). The
    // hero's triangle is a *screen* — it shows the shader surface itself.
    let preview = use_preview_lease_raster(
        Some(PreviewSource::Example(HERO_EXAMPLE.to_string())),
        Some(HERO_FPS),
    );
    // Both clipped layers must carry the IDENTICAL path string, or the
    // canvas would reveal into a subtly different silhouette than the
    // gradient it replaces. Built once per mount: it never varies.
    let clip = use_hook(|| {
        let (cx, cy, r) = HERO_TRI;
        format!(
            "clip-path:path('{}');",
            fillet_tri_path(cx, cy, r, r * HERO_CORNER_RATIO)
        )
    });
    let gradient = use_hook(|| thumb_swatch_style(HERO_EXAMPLE, false));
    let (box_w, box_h) = HERO_BOX;
    let (canvas_w, canvas_h) = HERO_CANVAS;

    rsx! {
        div { class: "tw:flex tw:flex-col tw:items-center tw:gap-5",
            div {
                id: "{preview.frame_id}",
                class: "tw:relative",
                style: "width:{box_w}px;height:{box_h}px",
                // 1 · Spill. The blur lives on a WRAPPER and the clip on the
                // child: CSS applies filter before clip-path on one element,
                // so a clipped-and-blurred div would have its own bloom
                // clipped away — exactly the light that is supposed to escape.
                div {
                    class: "tw:pointer-events-none tw:absolute tw:inset-0",
                    style: "filter:blur(22px);opacity:0.55;transform:scale(1.06)",
                    "aria-hidden": "true",
                    div { class: "tw:absolute tw:inset-0", style: "{gradient}{clip}" }
                }
                // 2 · Fallback: the identity gradient, clipped — the base
                // layer, and the whole picture until (or unless) a frame lands.
                div { class: "tw:absolute tw:inset-0", style: "{gradient}{clip}" }
                // 3 · Canvas: keyed, so a bumped generation mounts a fresh
                // element (an offscreen-transferred canvas is consumed).
                if let Some(canvas) = preview.canvas {
                    canvas {
                        key: "{canvas.id}",
                        id: "{canvas.id}",
                        width: "{canvas_w}",
                        height: "{canvas_h}",
                        class: hero_canvas_class(canvas.revealed),
                        style: "{clip}",
                    }
                }
                // 4 · Badge: the tier, at hero volume — quiet, never absent.
                if let Some(badge) = preview.badge {
                    span {
                        class: "tw:absolute tw:bottom-0 tw:right-0 tw:rounded-sm tw:border tw:bg-background/60 tw:px-1 tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-4 {hero_badge_class(&badge)}",
                        title: hero_badge_title(&badge),
                        {hero_badge_text(&badge)}
                    }
                }
            }
            span { class: "tw:flex tw:text-strong",
                BrandWord { word_px: 40 }
            }
            HeroEditLink { on_action }
        }
    }
}

/// The way out of the hero and into the editor: the Explore card's open
/// path, worn quietly. Without a dispatcher (stories, host builds) it
/// renders inert with a tooltip that says why — the `OpenInStudioButton`
/// precedent.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HeroEditLink(#[props(default)] on_action: Option<EventHandler<UiAction>>) -> Element {
    let live = on_action.is_some();
    let title = if live {
        "Opens this shader in the editor and keeps it in your projects"
    } else {
        "Only available in the running app"
    };
    rsx! {
        button {
            class: hero_edit_class(live),
            r#type: "button",
            disabled: !live,
            title: "{title}",
            onclick: move |_| {
                if let Some(on_action) = on_action {
                    on_action
                        .call(
                            home_action(HomeOp::OpenExample {
                                id: HERO_EXAMPLE.to_string(),
                            }),
                        );
                }
            },
            "edit this shader"
        }
    }
}

/// Quiet by design: text weight, no chrome until hover. The hero's own
/// picture is the call to action; this is the door beside it.
fn hero_edit_class(live: bool) -> &'static str {
    if live {
        "tw:cursor-pointer tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2 tw:py-1 tw:text-xs tw:text-muted-foreground tw:transition-colors tw:hover:border-border tw:hover:text-strong-foreground"
    } else {
        "tw:cursor-not-allowed tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-2 tw:py-1 tw:text-xs tw:text-dim-foreground"
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

/// The gallery's tier vocabulary, at landing volume.
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
