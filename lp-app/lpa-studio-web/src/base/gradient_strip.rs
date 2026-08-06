//! `GradientStripCanvas`: a 256×1-texel preview of a [`Gradient`] (or a
//! `from` → `to` blend), painted onto a `<canvas>` and CSS-scaled to fill
//! its container.
//!
//! Same `putImageData`/`ImageData` pattern
//! [`ProductPreviewCanvas`](crate::app::node::ProductPreviewCanvas) uses for
//! visual product previews — paint on mount and whenever the sampled input
//! changes, skip repainting an unchanged frame.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_palettes::sample_gradient_as_srgb;
use lpc_model::{Gradient, InterpMethod};
use wasm_bindgen::{Clamped, JsCast};

/// Texels sampled across the strip's width. 256 is smooth at any CSS width
/// Studio renders this at; the canvas element stays exactly this wide in
/// device pixels and is scaled up by CSS.
const STRIP_TEXELS: u32 = 256;

/// Monotonic canvas element ids (one per mounted strip).
static NEXT_STRIP_CANVAS_ID: AtomicU64 = AtomicU64::new(0);

/// Renders `gradient` (or a `gradient` → `second` cross-fade, blended by
/// `mix`) as a 256×1-texel strip, CSS-scaled to fill its container.
/// `image-rendering: pixelated` kicks in only for [`InterpMethod::Step`]
/// gradients, so a discrete swatch list reads as hard bands rather than a
/// blur.
///
/// Reuses the `ux-produced-product-pixel-canvas` class so the story-capture
/// ready-wait in `scripts/studio-story-pngs.mjs` (which already gates on
/// `canvas.ux-produced-product-pixel-canvas:not([data-preview-painted])`)
/// covers this canvas too, with no script change. That class also carries an
/// unconditional `position: absolute; inset: 0` + `image-rendering:
/// pixelated`, so this component supplies its own positioned, sized wrapper
/// and overrides `image-rendering` inline per gradient method — inline style
/// always wins the cascade over the shared class rule.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn GradientStripCanvas(
    gradient: Gradient,
    /// A second gradient to cross-fade toward. `None` renders `gradient`
    /// alone.
    #[props(default)]
    second: Option<Gradient>,
    /// Blend factor toward `second`, `[0, 1]`. Ignored when `second` is
    /// `None`.
    #[props(default = 0.0)]
    mix: f32,
) -> Element {
    let canvas_id = use_hook(|| {
        let id = NEXT_STRIP_CANVAS_ID.fetch_add(1, Ordering::Relaxed);
        format!("gradient-strip-canvas-{id}")
    });
    // What the canvas currently shows: the last painted key. A parent
    // re-render with unchanged inputs skips the repaint entirely.
    let painted = use_hook(|| Rc::new(RefCell::new(None::<GradientStripPaintKey>)));

    let image_rendering = if gradient.method == InterpMethod::Step {
        "pixelated"
    } else {
        "auto"
    };
    let paint_key = GradientStripPaintKey {
        gradient,
        second,
        mix,
    };

    if painted.borrow().as_ref() != Some(&paint_key) {
        // Paint from a task, not the render pass — the canvas element is
        // stable across diffs, so painting never fights the vdom; a failed
        // paint (element not mounted yet) retries on the next snapshot and
        // the `onmounted` paint below covers the first frame.
        let canvas_id = canvas_id.clone();
        let key = paint_key.clone();
        let painted = painted.clone();
        spawn(async move {
            match paint_gradient_strip_canvas(&canvas_id, &key) {
                Ok(()) => *painted.borrow_mut() = Some(key),
                Err(error) => log::debug!("gradient strip paint skipped: {error}"),
            }
        });
    }

    let mount_canvas_id = canvas_id.clone();
    let mount_key = paint_key.clone();
    let mount_painted = painted.clone();
    rsx! {
        div { class: "tw:relative tw:h-6 tw:w-full tw:overflow-hidden tw:rounded-sm tw:border tw:border-border",
            canvas {
                id: "{canvas_id}",
                class: "ux-produced-product-pixel-canvas",
                width: "{STRIP_TEXELS}",
                height: "1",
                style: "image-rendering: {image_rendering};",
                onmounted: move |_| {
                    let canvas_id = mount_canvas_id.clone();
                    let key = mount_key.clone();
                    let painted = mount_painted.clone();
                    spawn(async move {
                        match paint_gradient_strip_canvas(&canvas_id, &key) {
                            Ok(()) => *painted.borrow_mut() = Some(key),
                            Err(error) => log::warn!("gradient strip mount paint failed: {error}"),
                        }
                    });
                },
            }
        }
    }
}

/// Everything a paint depends on — compared against the last-painted value
/// so an unchanged re-render skips the repaint.
#[derive(Clone, PartialEq)]
struct GradientStripPaintKey {
    gradient: Gradient,
    second: Option<Gradient>,
    mix: f32,
}

/// Sample `key` across [`STRIP_TEXELS`] texels and blit the result onto the
/// canvas identified by `canvas_id` (the `produced_product_view.rs` blit
/// pattern).
fn paint_gradient_strip_canvas(canvas_id: &str, key: &GradientStripPaintKey) -> Result<(), String> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| format!("canvas #{canvas_id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element #{canvas_id} is not a canvas"))?;
    if canvas.width() != STRIP_TEXELS || canvas.height() != 1 {
        canvas.set_width(STRIP_TEXELS);
        canvas.set_height(1);
    }
    let context = canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "canvas has no 2d context".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has an unexpected type".to_string())?;

    let rgba = gradient_strip_rgba(key);
    let image =
        web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), STRIP_TEXELS, 1)
            .map_err(|error| format!("build ImageData: {error:?}"))?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|error| format!("putImageData: {error:?}"))?;
    // Paint runs async after mount, so an unpainted canvas is a possible page
    // state; the story-capture ready-wait polls this marker (set
    // imperatively — Dioxus never writes this attribute, so the vdom won't
    // clear it on re-render).
    canvas
        .set_attribute("data-preview-painted", "1")
        .map_err(|error| format!("mark painted: {error:?}"))?;
    Ok(())
}

/// One RGBA byte quad per texel: `key.gradient` sampled alone, or blended
/// toward `key.second` by `key.mix` — a cheap per-channel lerp of the two
/// already-sampled display colors, not a second gradient recombination.
fn gradient_strip_rgba(key: &GradientStripPaintKey) -> Vec<u8> {
    let mix = key.mix.clamp(0.0, 1.0);
    let mut rgba = Vec::with_capacity(STRIP_TEXELS as usize * 4);
    for x in 0..STRIP_TEXELS {
        let t = x as f32 / (STRIP_TEXELS - 1) as f32;
        let primary = sample_gradient_as_srgb(&key.gradient, t);
        let srgb = match &key.second {
            Some(second) if mix > 0.0 => {
                let secondary = sample_gradient_as_srgb(second, t);
                [
                    primary[0] + (secondary[0] - primary[0]) * mix,
                    primary[1] + (secondary[1] - primary[1]) * mix,
                    primary[2] + (secondary[2] - primary[2]) * mix,
                ]
            }
            _ => primary,
        };
        for channel in srgb {
            rgba.push((channel * 255.0).round().clamp(0.0, 255.0) as u8);
        }
        rgba.push(255);
    }
    rgba
}
