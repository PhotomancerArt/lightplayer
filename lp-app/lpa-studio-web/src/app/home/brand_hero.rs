//! The landing hero: the brand triangle as a window onto a live shader.
//!
//! The mark's play triangle, blown up to hero scale, is not drawn here —
//! it is a *clip path*. Behind it runs a real engine preview (a
//! `PreviewHost` lease of the same page-wide host the gallery thumbs and
//! the docs hero use), so the first thing the landing says about
//! LightPlayer is said by the engine itself rather than by a picture of
//! it.
//!
//! The stage holds ONE engine canvas behind TWO windows — the triangle and
//! the wordmark glyphs, cut by a single SVG clipPath. One visual signal,
//! multiple mapped objects: the product's mapping story, told by the
//! landing page itself. The light that fills the triangle flows on
//! through the letters below it.
//!
//! Layers, back to front (spike `spikes/logo-triangle-chip/index.html` §4,
//! treatment **Spill**, extended by the shader-lit wordmark):
//!
//! 1. **Spill** — the identity gradient, blurred and slightly enlarged
//!    behind the triangle: colored light escaping past its edge. A still
//!    approximation, on purpose: the engine owns its canvas and there is no
//!    frame we could blur in step with it (plan D1). It paints instantly,
//!    which is the point — the hero is lit before the engine has warmed.
//! 2. **Fallback** — the identity gradient clipped to the triangle. The
//!    reveal base, and the whole face wherever no preview runs (stories,
//!    host builds, a failed slot). Exactly the docs-hero pattern.
//! 3. **Pre-reveal word** — the brand rainbow sweep, and the whole word
//!    wherever no engine runs. Crossfades out when the canvas lights the
//!    same glyphs.
//! 4. **Canvas** — the leased engine surface behind both windows, revealed
//!    once a frame has landed so the gradient covers the boot.
//! 5. **Badge** — failures only (issue-only policy, fidelity-tiers ADR
//!    decision-4 note). The landing never announces its renderer; a broken
//!    preview still says so out loud.
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
use lpa_studio_core::PreviewSource;

use crate::app::home::card_thumb::thumb_swatch_style;
use crate::app::home::gallery_preview::{ThumbPreviewBadge, use_preview_lease_raster};
use crate::base::logo_mark::{BrandWord, fillet_tri_path};

/// The landing hero's example and cadence. A constant on purpose: the
/// future fixture-hero plan swaps this surface's source, not its shape.
/// `pub(crate)` because the landing's "Edit the logo" pill opens it.
/// It is the brand's own artwork (`examples/logo-sign`): the triangle and
/// the letters the hero cuts its windows from are the same objects that
/// example maps to lamps, so the pill hands the visitor exactly the piece
/// they were just watching.
pub(crate) const HERO_EXAMPLE: &str = "examples/logo-sign";
/// Present cadence for the hero — the visitor is watching this one.
const HERO_FPS: f32 = 30.0;

/// The triangle window in CSS pixels, and the triangle inside it:
/// circumradius and center, from the spike's landing mock (250×232,
/// r = h/2, cx = 0.46w).
/// `pub(crate)` from here down: the `examples/logo-sign` map2d generator
/// (`app::home::logo_sign_gen`) lays its canvas out on exactly this stage,
/// so the artwork the pencil opens is the artwork the hero shows.
pub(crate) const HERO_BOX: (f32, f32) = (250.0, 232.0);
pub(crate) const HERO_TRI: (f32, f32, f32) = (115.0, 116.0, 116.0);
/// Hero-specific fillet ratio (the mark keeps 0.16 — spike gate-4).
pub(crate) const HERO_CORNER_RATIO: f32 = 0.10;
/// The stage: ONE canvas behind BOTH brand objects. The triangle and the
/// wordmark are two windows onto the same running shader — one visual
/// signal, multiple mapped objects, which is the product's mapping story
/// told by the landing page (gate follow-up, 2026-08-24).
pub(crate) const STAGE: (f32, f32) = (300.0, 308.0);
/// Wordmark inside the stage: size, and the SVG text baseline the clip
/// glyphs sit on. The HTML word (the pre-reveal rainbow sweep) is placed
/// to land its baseline on the same line, so the crossfade doesn't jump.
pub(crate) const WORD_PX: f32 = 40.0;
pub(crate) const WORD_BASELINE_Y: f32 = 292.0;
/// Canvas backing store: the stage at 2× device pixels, in the stage's
/// own aspect.
const HERO_CANVAS: (u32, u32) = (600, 616);

/// The landing hero: brand triangle as a live shader window, the wordmark
/// lit by the same surface. The way into the editor — the "Edit this
/// artwork" pill — lives under the slogan in `home_landing`, not on this
/// stage: the hero is a window, and furniture inside it fought the mark.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn BrandHero() -> Element {
    // Raster-first on purpose: examples drive LEDs, so their products are
    // control-first and a plain lease would keep the raster hidden behind a
    // lamp view (G1 finding: the triangle sat dark on the gradient). The
    // hero's triangle is a *screen* — it shows the shader surface itself.
    let preview = use_preview_lease_raster(
        Some(PreviewSource::Example(HERO_EXAMPLE.to_string())),
        Some(HERO_FPS),
    );
    // The fallback gradient's clip must match the canvas mask's triangle
    // exactly, or the canvas would reveal into a subtly different
    // silhouette than the gradient it replaces. Built once per mount.
    let clip = use_hook(|| {
        let (cx, cy, r) = HERO_TRI;
        format!(
            "clip-path:path('{}');",
            fillet_tri_path(cx, cy, r, r * HERO_CORNER_RATIO)
        )
    });
    // The combined stage mask: the same triangle (in stage coordinates)
    // plus the wordmark's glyphs, as an SVG clipPath applied to the one
    // canvas. Injected as a 0×0 defs block — clipPath with a <text> child
    // is plain SVG 1.1, and `userSpaceOnUse` keeps the coordinates in the
    // stage's own pixels.
    let stage_defs = use_hook(|| {
        let (stage_w, _) = STAGE;
        let (box_w, _) = HERO_BOX;
        let (cx, cy, r) = HERO_TRI;
        let tri = fillet_tri_path((stage_w - box_w) / 2.0 + cx, cy, r, r * HERO_CORNER_RATIO);
        format!(
            "<svg width=\"0\" height=\"0\" style=\"position:absolute\" aria-hidden=\"true\"><defs>\
             <clipPath id=\"brand-hero-clip\" clipPathUnits=\"userSpaceOnUse\">\
             <path d=\"{tri}\"/>\
             <text x=\"{mid}\" y=\"{WORD_BASELINE_Y}\" text-anchor=\"middle\" \
              font-family=\"Inter, ui-sans-serif, system-ui, sans-serif\" \
              font-weight=\"800\" font-size=\"{WORD_PX}\" letter-spacing=\"-0.6\"\
             >LightPlayer</text>\
             </clipPath></defs></svg>",
            mid = stage_w / 2.0,
        )
    });
    let gradient = use_hook(|| thumb_swatch_style(HERO_EXAMPLE, false));
    let (stage_w, stage_h) = STAGE;
    let (box_w, box_h) = HERO_BOX;
    let box_x = (stage_w - box_w) / 2.0;
    let (canvas_w, canvas_h) = HERO_CANVAS;
    // Once the canvas is lit, the HTML word (the rainbow-sweep pre-reveal
    // state, and the whole word wherever no engine runs) yields to the
    // shader-lit glyphs above it.
    let word_lit = preview
        .canvas
        .as_ref()
        .is_some_and(|canvas| canvas.revealed);

    rsx! {
        div { class: "tw:flex tw:flex-col tw:items-center",
            div {
                id: "{preview.frame_id}",
                class: "tw:relative",
                style: "width:{stage_w}px;height:{stage_h}px",
                // 0 · The combined clip (triangle + wordmark glyphs).
                div { dangerous_inner_html: "{stage_defs}" }
                // 1 · Spill. The blur lives on a WRAPPER and the clip on the
                // child: CSS applies filter before clip-path on one element,
                // so a clipped-and-blurred div would have its own bloom
                // clipped away — exactly the light that is supposed to escape.
                div {
                    class: "tw:pointer-events-none tw:absolute",
                    style: "left:{box_x}px;top:0;width:{box_w}px;height:{box_h}px;filter:blur(22px);opacity:0.55;transform:scale(1.06)",
                    "aria-hidden": "true",
                    div { class: "tw:absolute tw:inset-0", style: "{gradient}{clip}" }
                }
                // 2 · Fallback: the identity gradient, clipped to the
                // triangle — the base layer, and the whole picture until
                // (or unless) a frame lands.
                div {
                    class: "tw:absolute",
                    style: "left:{box_x}px;top:0;width:{box_w}px;height:{box_h}px;{gradient}{clip}",
                }
                // 3 · The pre-reveal word: the brand rainbow sweep, and the
                // whole word wherever no engine runs (stories, failures).
                // Fades out as the canvas lights the same glyphs from the
                // shader — the crossfade, not an overlap (SVG and HTML text
                // metrics differ subtly; painting both would fringe).
                div {
                    class: hero_word_class(word_lit),
                    style: "top:258px",
                    span { class: "tw:flex tw:text-strong",
                        BrandWord { word_px: WORD_PX as u32 }
                    }
                }
                // 4 · Canvas: one engine surface behind both windows;
                // keyed, so a bumped generation mounts a fresh element (an
                // offscreen-transferred canvas is consumed).
                if let Some(canvas) = preview.canvas {
                    canvas {
                        key: "{canvas.id}",
                        id: "{canvas.id}",
                        width: "{canvas_w}",
                        height: "{canvas_h}",
                        class: hero_canvas_class(canvas.revealed),
                        style: "clip-path:url(#brand-hero-clip)",
                    }
                }
                // 5 · Badge: failures only — the landing never announces
                // its renderer, but a broken preview must not just sit dark.
                if let Some(badge) = preview.badge.and_then(ThumbPreviewBadge::issue) {
                    span {
                        class: "tw:absolute tw:bottom-0 tw:right-0 tw:rounded-sm tw:border tw:bg-background/60 tw:px-1 tw:text-[0.6rem] tw:font-bold tw:uppercase tw:leading-4 {hero_badge_class(&badge)}",
                        title: hero_badge_title(&badge),
                        {hero_badge_text(&badge)}
                    }
                }
            }
        }
    }
}

/// The pre-reveal word wrapper: absolutely placed so its baseline sits on
/// [`WORD_BASELINE_Y`] (the clip glyphs' line), fading out once the shader
/// lights the glyphs. Same duration as the canvas reveal — the stage
/// breathes as one surface, not two exchanges.
fn hero_word_class(lit: bool) -> &'static str {
    if lit {
        "tw:absolute tw:inset-x-0 tw:flex tw:justify-center tw:opacity-0 tw:transition-opacity tw:duration-700 tw:ease-out"
    } else {
        "tw:absolute tw:inset-x-0 tw:flex tw:justify-center tw:opacity-100 tw:transition-opacity tw:duration-700 tw:ease-out"
    }
}

/// Revealed only once a frame has landed, so the gradient covers the boot.
/// The transition classes ride BOTH states so the fade is symmetric and
/// slow enough to read as a reveal, not a swap (polish round: the 300ms
/// one-sided fade registered as a jump).
fn hero_canvas_class(revealed: bool) -> &'static str {
    if revealed {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-100 tw:transition-opacity tw:duration-700 tw:ease-out"
    } else {
        "tw:absolute tw:inset-0 tw:h-full tw:w-full tw:opacity-0 tw:transition-opacity tw:duration-700 tw:ease-out"
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
