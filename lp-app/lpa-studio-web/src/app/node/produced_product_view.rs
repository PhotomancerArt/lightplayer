//! Leaf presentation for a produced product.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::{
    ColorOrder, ControlDisplayLayout, ControlSampleEncoding, UiAction, UiControlProductPreview,
    UiProducedProduct, UiProductKind, UiProductPreview, UiProductPreviewFrame,
    UiProductTrackingState,
};
use wasm_bindgen::{Clamped, JsCast};

use crate::app::node::map_view::{
    MapArrowsOverlay, MapViewOptions, neutral_lamp_rgb, universe_rgb, wiring_arrow_overlay,
};
use crate::app::node::{BindingChip, BindingChipDirection, SlotPane, SlotPaneTreatment};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedProductView(
    product: UiProducedProduct,
    #[props(default = false)] initially_open: bool,
    #[props(default)] focus_action: Option<UiAction>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
    /// The clock's live effective seconds — the clock face passes it so its
    /// time product renders a counter instead of the caption band.
    #[props(default = None)]
    time_seconds: Option<String>,
) -> Element {
    let aspects = product.visible_aspects();
    // Visual and control previews are width-capped hero media: the pane hugs
    // them (`fit`) and they bleed to the pane edges (`flush`) so the pane is the
    // only frame. Empty/other placeholders have no intrinsic size and keep the
    // padded, full-width treatment.
    let media = matches!(product.kind, UiProductKind::Visual | UiProductKind::Control);
    let bus_target = product.binding.bindings.bus_target.clone();
    let treatment = if bus_target.is_some() {
        SlotPaneTreatment::Bound
    } else {
        SlotPaneTreatment::Neutral
    };

    rsx! {
        SlotPane {
            title: product.name.clone(),
            aspects,
            initially_open,
            treatment,
            fit: media,
            flush: media,
            on_action,
            authoring: product.authoring.clone(),
            ProductPreview {
                kind: product.kind,
                preview: product.preview.clone(),
                tracking: product.tracking,
                frame: product.frame,
                focus_action,
                on_action,
                time_seconds,
            }
            if let Some(endpoint) = bus_target {
                div { class: "tw:flex tw:min-w-0 tw:justify-center tw:p-1.5",
                    BindingChip {
                        endpoint,
                        direction: BindingChipDirection::Publishes,
                    }
                }
            }
        }
    }
}

/// The bare preview media (pixel grid / lamp layout / skeleton / message)
/// inside the product's aspect frame. `pub(crate)` so node-face heroes and
/// playlist strip thumbnails reuse the exact preview path instead of
/// mocking a lookalike.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProductPreview(
    kind: UiProductKind,
    preview: UiProductPreview,
    tracking: UiProductTrackingState,
    frame: UiProductPreviewFrame,
    focus_action: Option<UiAction>,
    on_action: Option<EventHandler<UiAction>>,
    /// Mapping view options for control lamp layouts (fixture face passes
    /// its toggles; everywhere else keeps the default live look).
    #[props(default)]
    map_view: MapViewOptions,
    /// The clock's live effective seconds ("42.35"), when the caller has a
    /// reading — turns a time product's band into a counter.
    #[props(default = None)]
    time_seconds: Option<String>,
) -> Element {
    // A time product is metadata-only BY DESIGN (nothing to draw behind the
    // handle — the phasor cards below are its real face), so it never gets
    // the aspect-framed hero the pictorial products fill (a full-width box
    // of nothing — G2 gate feedback). What it shows instead depends on the
    // caller: the clock face passes the clock's live effective seconds and
    // gets a COUNTER — the number a scrub or speed change visibly moves
    // ("Time product" as a caption did nothing for anyone) — while product
    // rows without a reading keep the compact caption band.
    if kind == UiProductKind::Time && matches!(preview, UiProductPreview::MetadataOnly) {
        if let Some(seconds) = time_seconds {
            return rsx! {
                div { class: "ux-produced-product-metadata",
                    span {
                        class: "tw:font-mono tw:text-2xl tw:font-semibold tw:leading-none tw:tabular-nums tw:text-strong-foreground",
                        title: "Effective clock seconds — scrubbing and speed move this number",
                        "{seconds}"
                        span { class: "tw:text-sm tw:font-normal tw:text-subtle-foreground", " s" }
                    }
                    span { class: "tw:text-[10px] tw:uppercase tw:leading-none tw:tracking-[0.1em] tw:text-dim-foreground",
                        "seconds"
                    }
                }
            };
        }
        return rsx! {
            div { class: "ux-produced-product-metadata",
                strong { class: "tw:text-sm tw:text-strong-foreground",
                    {metadata_only_title(kind)}
                }
                span { class: "tw:text-xs tw:leading-snug tw:text-muted-foreground",
                    {metadata_only_detail(kind)}
                }
            }
        };
    }

    let frame_class = product_frame_class(kind);
    let frame_style = preview_frame_style(&preview, frame);
    let overlay = product_tracking_overlay(kind, tracking);

    rsx! {
        div { class: "{frame_class}", style: "{frame_style}",
            match preview {
                UiProductPreview::VisualSrgb8 {
                    width,
                    height,
                    revision,
                    bytes,
                } => rsx! {
                    ProductPreviewCanvas { width, height, revision, bytes }
                },
                UiProductPreview::ControlNative(preview) => rsx! {
                    ControlProductPreview { preview, map_view }
                },
                UiProductPreview::Pending => rsx! {
                    ProductSkeleton {
                        kind,
                        tone: if tracking == UiProductTrackingState::Tracking {
                            ProductSkeletonTone::Working
                        } else {
                            ProductSkeletonTone::Quiet
                        },
                        title: "Tracking product",
                        detail: "Waiting for the first preview.",
                        show_text: tracking == UiProductTrackingState::Tracking,
                    }
                },
                UiProductPreview::Error { message } => rsx! {
                    ProductMessage {
                        tone: ProductMessageTone::Error,
                        message,
                    }
                },
                UiProductPreview::Unsupported { reason } => rsx! {
                    ProductMessage {
                        tone: ProductMessageTone::Warning,
                        message: reason,
                    }
                },
                UiProductPreview::Empty => rsx! {
                    ProductSkeleton {
                        kind,
                        tone: ProductSkeletonTone::Quiet,
                        title: "No product",
                        detail: "This output has not resolved to a product.",
                        show_text: true,
                    }
                },
                UiProductPreview::MetadataOnly => rsx! {
                    ProductSkeleton {
                        kind,
                        tone: ProductSkeletonTone::Quiet,
                        title: metadata_only_title(kind),
                        detail: metadata_only_detail(kind),
                        show_text: true,
                    }
                },
            }
            if let Some(overlay) = overlay {
                ProductTrackingOverlay {
                    title: overlay.title,
                    detail: overlay.detail,
                    focus_action,
                    on_action,
                }
            }
        }
    }
}

/// Metadata-only heading. A **time** product is metadata-only by design, not
/// by omission: there is nothing to draw behind the handle, and the way to
/// look at it is the clock face's phasor listing. Saying "Studio does not
/// render this yet" there would report a gap that is not one.
fn metadata_only_title(kind: UiProductKind) -> &'static str {
    match kind {
        UiProductKind::Time => UiProductKind::Time.detail_label(),
        _ => "Metadata only",
    }
}

fn metadata_only_detail(kind: UiProductKind) -> &'static str {
    match kind {
        UiProductKind::Time => {
            "A queryable timebase: seconds, this tick's delta, and the phasors riding it."
        }
        _ => "Studio does not render this product type yet.",
    }
}

fn product_frame_class(kind: UiProductKind) -> &'static str {
    match kind {
        UiProductKind::Visual | UiProductKind::Control => {
            "ux-produced-product-frame ux-produced-product-frame-capped"
        }
        _ => "ux-produced-product-frame",
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ControlProductPreview(
    preview: UiControlProductPreview,
    #[props(default)] map_view: MapViewOptions,
) -> Element {
    let Some(ControlDisplayLayout::Layout2d(layout)) = preview.display_layout.as_ref() else {
        return rsx! {
            ProductMessage {
                tone: ProductMessageTone::Warning,
                message: "Control product has no display layout.".to_string(),
            }
        };
    };

    if !control_sample_layout_has_rgb(&preview) {
        return rsx! {
            ProductMessage {
                tone: ProductMessageTone::Warning,
                message: "Control product sample layout is not RGB.".to_string(),
            }
        };
    }

    let arrows = map_view
        .arrows
        .then(|| wiring_arrow_overlay(layout))
        .unwrap_or_default();
    let lamps = control_lamp_render(&preview, map_view);
    rsx! {
        div { class: "ux-produced-product-control-layout",
            // Inset wrapper pulls the whole lamp field off the frame edges so
            // edge-mapped lamps read fully instead of half-clipping (M3 gate:
            // "zoom it out a bit"). Geometry inside stays texture-faithful.
            div { class: "ux-map-inset",
                for (index, lamp) in lamps.iter().enumerate() {
                    span {
                        key: "{index}",
                        class: "ux-produced-product-control-lamp",
                        style: "{lamp.style}",
                    }
                }
                MapArrowsOverlay { overlay: arrows }
                // Numbers are a separate layer: the lamp elements screen-blend
                // against the black frame, which would wash dark glyphs out.
                if map_view.numbers {
                    for (index, lamp) in lamps.iter().enumerate() {
                        if let Some(number) = lamp.number.as_ref() {
                            span {
                                key: "num-{index}",
                                class: "ux-map-lamp-num",
                                style: "{lamp.position_style}",
                                "{number}"
                            }
                        }
                    }
                }
            }
            if layout.lamps.is_empty() {
                ProductMessage {
                    tone: ProductMessageTone::Warning,
                    message: "Control product display layout is empty.".to_string(),
                }
            }
        }
    }
}

/// Monotonic preview-canvas element ids (one per mounted canvas).
static NEXT_PREVIEW_CANVAS_ID: AtomicU64 = AtomicU64::new(0);

/// One visual preview = one `<canvas>` painted with `putImageData`. Replaces
/// the per-pixel `<span>` grid, whose 1024 keyed DOM nodes were re-diffed on
/// every view snapshot (the dominant UI-thread cost with live sim previews).
/// `pub(crate)` so the agent history filmstrip renders its thumbnails through
/// the exact preview path (same doctrine as [`ProductPreview`]'s reuse by
/// faces and playlist strips).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn ProductPreviewCanvas(
    width: u32,
    height: u32,
    revision: i64,
    bytes: Rc<[u8]>,
) -> Element {
    let canvas_id = use_hook(|| {
        let id = NEXT_PREVIEW_CANVAS_ID.fetch_add(1, Ordering::Relaxed);
        format!("product-preview-canvas-{id}")
    });
    // What the canvas currently shows: probe revision + buffer identity
    // (two products can share an engine revision, so the revision alone is
    // not a safe skip key). Parent re-renders without new probe bytes skip
    // the repaint entirely.
    let painted = use_hook(|| Rc::new(Cell::new(None::<(i64, usize)>)));

    let paint_key = (revision, Rc::as_ptr(&bytes) as *const u8 as usize);
    if painted.get() != Some(paint_key) {
        // Paint from a task, not the render pass. The canvas element is
        // stable across diffs, so painting never fights the vdom; a failed
        // paint (element not mounted yet) retries on the next snapshot and
        // the `onmounted` paint below covers the first frame.
        let canvas_id = canvas_id.clone();
        let bytes = bytes.clone();
        let painted = painted.clone();
        spawn(async move {
            match paint_preview_canvas(&canvas_id, width, height, &bytes) {
                Ok(()) => painted.set(Some(paint_key)),
                Err(error) => log::debug!("preview canvas paint skipped: {error}"),
            }
        });
    }

    let mount_canvas_id = canvas_id.clone();
    let mount_painted = painted.clone();
    rsx! {
        canvas {
            id: "{canvas_id}",
            class: "ux-produced-product-pixel-canvas",
            width: "{width}",
            height: "{height}",
            onmounted: move |_| {
                let canvas_id = mount_canvas_id.clone();
                let bytes = bytes.clone();
                let painted = mount_painted.clone();
                spawn(async move {
                    match paint_preview_canvas(&canvas_id, width, height, &bytes) {
                        Ok(()) => painted.set(Some(paint_key)),
                        Err(error) => log::warn!("preview canvas mount paint failed: {error}"),
                    }
                });
            },
        }
    }
}

/// Expand tightly-packed RGB8 preview bytes to RGBA and blit them onto the
/// canvas identified by `canvas_id` (the `preview_host_impl.rs` blit
/// pattern, minus the transferable-buffer source).
fn paint_preview_canvas(
    canvas_id: &str,
    width: u32,
    height: u32,
    bytes: &[u8],
) -> Result<(), String> {
    let width = width.max(1);
    let height = height.max(1);
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| format!("canvas #{canvas_id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element #{canvas_id} is not a canvas"))?;
    if canvas.width() != width || canvas.height() != height {
        canvas.set_width(width);
        canvas.set_height(height);
    }
    let context = canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "canvas has no 2d context".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has an unexpected type".to_string())?;

    let mut rgba = vec![255_u8; width as usize * height as usize * 4];
    for (dst, src) in rgba.chunks_exact_mut(4).zip(bytes.chunks_exact(3)) {
        dst[..3].copy_from_slice(src);
    }
    let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), width, height)
        .map_err(|error| format!("build ImageData: {error:?}"))?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|error| format!("putImageData: {error:?}"))?;
    // Paint runs async after mount, so an unpainted canvas is a possible
    // page state; the story-capture ready-wait polls this marker so a
    // baseline can never freeze that state (set imperatively — Dioxus never
    // writes this attribute, so the vdom won't clear it on re-render).
    canvas
        .set_attribute("data-preview-painted", "1")
        .map_err(|error| format!("mark painted: {error:?}"))?;
    Ok(())
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductSkeleton(
    kind: UiProductKind,
    tone: ProductSkeletonTone,
    title: &'static str,
    detail: &'static str,
    #[props(default = true)] show_text: bool,
) -> Element {
    let class = product_skeleton_class(kind, tone);

    rsx! {
        div { class,
            div { class: "ux-produced-product-skeleton-graphic", aria_hidden: "true",
                for index in 0..12 {
                    span { key: "{index}", class: "ux-produced-product-skeleton-bar" }
                }
            }
            if show_text {
                div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:text-center",
                    strong { class: "tw:text-sm tw:text-strong-foreground", "{title}" }
                    span { class: "tw:text-xs tw:leading-snug tw:text-muted-foreground", "{detail}" }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductMessage(tone: ProductMessageTone, message: String) -> Element {
    let class = match tone {
        ProductMessageTone::Warning => {
            "ux-produced-product-message ux-produced-product-message-warning"
        }
        ProductMessageTone::Error => {
            "ux-produced-product-message ux-produced-product-message-error"
        }
    };

    rsx! {
        div { class, "{message}" }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductTrackingOverlay(
    title: &'static str,
    detail: &'static str,
    focus_action: Option<UiAction>,
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    if let (Some(action), Some(handler)) = (focus_action, on_action) {
        return rsx! {
            button {
                class: "ux-produced-product-overlay ux-produced-product-overlay-button",
                r#type: "button",
                onclick: move |event| {
                    event.stop_propagation();
                    handler.call(action.clone());
                },
                strong { "{title}" }
                span { "{detail}" }
            }
        };
    }

    rsx! {
        div { class: "ux-produced-product-overlay",
            strong { "{title}" }
            span { "{detail}" }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProductSkeletonTone {
    Quiet,
    Working,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProductMessageTone {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProductOverlayCopy {
    title: &'static str,
    detail: &'static str,
}

fn product_tracking_overlay(
    kind: UiProductKind,
    tracking: UiProductTrackingState,
) -> Option<ProductOverlayCopy> {
    let label = match kind {
        UiProductKind::Visual => "Visual output",
        UiProductKind::Control => "Control output",
        _ => return None,
    };
    match tracking {
        UiProductTrackingState::Untracked => Some(ProductOverlayCopy {
            title: if kind == UiProductKind::Visual {
                "Visual output not tracked"
            } else {
                "Control output not tracked"
            },
            detail: "Click to view",
        }),
        UiProductTrackingState::Paused => Some(ProductOverlayCopy {
            title: if label == "Visual output" {
                "Visual output paused"
            } else {
                "Control output paused"
            },
            detail: "Click to view",
        }),
        UiProductTrackingState::Tracking => None,
    }
}

fn preview_frame_style(preview: &UiProductPreview, frame: UiProductPreviewFrame) -> String {
    if let UiProductPreview::ControlNative(control) = preview
        && let Some(ControlDisplayLayout::Layout2d(layout)) = control.display_layout.as_ref()
    {
        return format!(
            "aspect-ratio: {} / {};",
            layout.width_hint.max(1),
            layout.height_hint.max(1)
        );
    }
    format!("aspect-ratio: {} / {};", frame.width, frame.height)
}

fn control_sample_layout_has_rgb(preview: &UiControlProductPreview) -> bool {
    preview.sample_layout.spans.iter().any(|span| {
        matches!(
            span.encoding,
            ControlSampleEncoding::RgbPixels { count, .. } if count > 0
        )
    })
}

struct ControlLampRender {
    style: String,
    position_style: String,
    number: Option<String>,
}

fn control_lamp_render(
    preview: &UiControlProductPreview,
    map_view: MapViewOptions,
) -> Vec<ControlLampRender> {
    let Some(ControlDisplayLayout::Layout2d(layout)) = preview.display_layout.as_ref() else {
        return Vec::new();
    };
    layout
        .lamps
        .iter()
        .map(|lamp| {
            // Fill precedence: live output > universe color > neutral.
            let [r, g, b] = if map_view.live {
                control_rgb_at_sample(preview, lamp.sample_start).unwrap_or([0, 0, 0])
            } else if map_view.universes {
                universe_rgb(lamp.lamp_index)
            } else {
                neutral_lamp_rgb()
            };
            let diameter = (lamp.radius.max(0.006) * 96.0).clamp(3.5, 18.0);
            let position_style = format!(
                "left: {:.3}%; top: {:.3}%;",
                lamp.center[0].clamp(0.0, 1.0) * 100.0,
                lamp.center[1].clamp(0.0, 1.0) * 100.0,
            );
            let style = format!(
                "--lamp-r: {r}; --lamp-g: {g}; --lamp-b: {b}; {position_style} width: max(5px, {diameter:.3}%); height: max(5px, {diameter:.3}%);"
            );
            ControlLampRender {
                style,
                position_style,
                number: map_view
                    .numbers
                    .then(|| (lamp.lamp_index + 1).to_string()),
            }
        })
        .collect()
}

/// Live lamp colors indexed by wiring index — the same sample decode the
/// display renderer uses, packaged for the mapping editor's live view.
pub(crate) fn control_live_lamp_colors(preview: &UiControlProductPreview) -> Vec<[u8; 3]> {
    let Some(ControlDisplayLayout::Layout2d(layout)) = preview.display_layout.as_ref() else {
        return Vec::new();
    };
    let len = layout
        .lamps
        .iter()
        .map(|lamp| lamp.lamp_index as usize + 1)
        .max()
        .unwrap_or(0);
    let mut colors = vec![[0_u8; 3]; len];
    for lamp in &layout.lamps {
        if let Some(rgb) = control_rgb_at_sample(preview, lamp.sample_start) {
            colors[lamp.lamp_index as usize] = rgb;
        }
    }
    colors
}

fn control_rgb_at_sample(preview: &UiControlProductPreview, sample_start: u32) -> Option<[u8; 3]> {
    let span = preview.sample_layout.spans.iter().find(|span| {
        matches!(span.encoding, ControlSampleEncoding::RgbPixels { .. })
            && sample_start >= span.start
            && sample_start.saturating_add(3) <= span.start.saturating_add(span.len)
            && (sample_start - span.start).is_multiple_of(3)
    })?;
    let color_order = match span.encoding {
        ControlSampleEncoding::RgbPixels { color_order, .. } => color_order,
        ControlSampleEncoding::Raw => return None,
    };
    let sample = |offset: u32| -> Option<u8> {
        let index = sample_start.checked_add(offset)? as usize;
        let byte_index = index.checked_mul(2)?;
        let lo = *preview.bytes.get(byte_index)?;
        let hi = *preview.bytes.get(byte_index + 1)?;
        Some((u16::from_le_bytes([lo, hi]) >> 8) as u8)
    };
    let a = sample(0)?;
    let b = sample(1)?;
    let c = sample(2)?;
    Some(match color_order {
        ColorOrder::Rgb => [a, b, c],
        ColorOrder::Grb => [b, a, c],
        ColorOrder::Rbg => [a, c, b],
        ColorOrder::Gbr => [c, a, b],
        ColorOrder::Brg => [b, c, a],
        ColorOrder::Bgr => [c, b, a],
    })
}

fn product_skeleton_class(kind: UiProductKind, tone: ProductSkeletonTone) -> &'static str {
    match (kind, tone) {
        (UiProductKind::Visual, ProductSkeletonTone::Working) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-visual ux-produced-product-skeleton-working"
        }
        (UiProductKind::Visual, ProductSkeletonTone::Quiet) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-visual"
        }
        (UiProductKind::Control, ProductSkeletonTone::Working) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-control ux-produced-product-skeleton-working"
        }
        (UiProductKind::Control, ProductSkeletonTone::Quiet) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-control"
        }
        (_, ProductSkeletonTone::Working) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-working"
        }
        (_, ProductSkeletonTone::Quiet) => "ux-produced-product-skeleton",
    }
}
