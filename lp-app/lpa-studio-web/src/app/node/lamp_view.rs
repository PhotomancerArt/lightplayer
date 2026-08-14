//! Canvas renderer for a control product's lamps — the product display.
//!
//! A control product is up to dome scale today (1500 lamps) and Radiance
//! scale tomorrow (~30k), so one DOM node per lamp is not an option: the
//! positioned-`<span>` renderer this replaces rebuilt the whole lamp field
//! through the vdom on every live frame. Here the field is one `<canvas>`:
//! lamps are rasterized into an RGBA buffer in Rust and blitted with
//! `putImageData` (the [`ProductPreviewCanvas`] precedent), so a live frame
//! costs one JS call regardless of lamp count.
//!
//! View mode is a product display, not a wiring tool — no numbers, no
//! arrows. Those are authoring instruments and live in the mapping editor.
//!
//! Upgrade path: the software rasterizer's per-frame fill is the cost that
//! scales, and at Radiance scale this component grows a WebGL/instanced
//! backend behind the same props. Not yet — 1500 lamps rasterize in well
//! under a millisecond, and a GPU context per card is its own bill.
//!
//! [`ProductPreviewCanvas`]: crate::app::node::ProductPreviewCanvas

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_mapping_editor::neutral_lamp_rgb;
use lpa_studio_core::{
    ColorOrder, ControlDisplayLayout, ControlSampleEncoding, UiControlProductPreview,
    UiFixturePatch,
};
use wasm_bindgen::{Clamped, JsCast, closure::Closure};

/// Monotonic lamp-canvas element ids (one per mounted lamp view).
static NEXT_LAMP_CANVAS_ID: AtomicU64 = AtomicU64::new(0);

/// The FIRST paint retries: the task can run before the browser flushes
/// layout, and a product with no live feed (stories, gallery) gets no later
/// snapshot to retry on — while story capture blocks on the painted marker.
/// Once a canvas has painted, a later failure just waits for the next frame,
/// so a canvas that is measurable never accumulates retry tasks.
const FIRST_PAINT_ATTEMPTS: u32 = 24;
const FIRST_PAINT_RETRY_MS: u32 = 16;

/// What the canvas currently shows. Buffer and layout identity are pointers,
/// not contents: both are shared `Rc`s rebuilt only when the producer
/// changes them, and the revision alone is not a safe key (two products can
/// share an engine revision).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LampPaintKey {
    revision: i64,
    bytes: usize,
    layout: usize,
    live: bool,
    size_epoch: u64,
}

/// One control product's lamps, painted on a single canvas.
///
/// `live` off paints the neutral lamp color, so a product with no feed
/// (stories, an untracked output) still reads as a layout.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn LampView(
    preview: UiControlProductPreview,
    #[props(default = true)] live: bool,
) -> Element {
    let canvas_id = use_hook(|| {
        let id = NEXT_LAMP_CANVAS_ID.fetch_add(1, Ordering::Relaxed);
        format!("lamp-view-canvas-{id}")
    });
    // Reused across paints: at DPR 2 a card-sized field is a couple of
    // megabytes, and reallocating that per live frame is the only allocation
    // this renderer would otherwise make.
    let scratch = use_hook(|| Rc::new(RefCell::new(Vec::<u8>::new())));
    let painted = use_hook(|| Rc::new(Cell::new(None::<LampPaintKey>)));
    let observer = use_hook(|| Rc::new(RefCell::new(None::<LampResizeObserver>)));
    // A box resize moves no prop, so the observer bumps this instead.
    let size_epoch = use_signal(|| 0_u64);

    let paint_key = LampPaintKey {
        revision: preview.revision,
        bytes: Rc::as_ptr(&preview.bytes) as *const u8 as usize,
        layout: preview
            .display_layout
            .as_ref()
            .map_or(0, |layout| Rc::as_ptr(layout) as usize),
        live,
        size_epoch: size_epoch(),
    };

    if painted.get() != Some(paint_key) {
        // Paint from a task, not the render pass: the canvas element is
        // stable across diffs, so painting never fights the vdom.
        let canvas_id = canvas_id.clone();
        let preview = preview.clone();
        let scratch = scratch.clone();
        let painted = painted.clone();
        let observer = observer.clone();
        spawn(async move {
            let attempts = if painted.get().is_some() {
                1
            } else {
                FIRST_PAINT_ATTEMPTS
            };
            for attempt in 0..attempts {
                let result = {
                    let mut buffer = scratch.borrow_mut();
                    paint_lamp_canvas(&canvas_id, &preview, live, &mut buffer)
                };
                match result {
                    Ok(()) => {
                        painted.set(Some(paint_key));
                        install_lamp_resize_observer(&observer, &canvas_id, size_epoch);
                        return;
                    }
                    Err(error) if attempt + 1 == attempts => {
                        log::debug!("lamp canvas paint skipped: {error}");
                    }
                    Err(_) => {
                        gloo_timers::future::TimeoutFuture::new(FIRST_PAINT_RETRY_MS).await;
                    }
                }
            }
        });
    }

    rsx! {
        canvas { id: "{canvas_id}", class: "ux-produced-product-lamp-canvas" }
    }
}

/// The dived fixture's live lamp colors, decoded out of its output's
/// published WIRE frame through the patch cells — the mapping editor's
/// live view (`live_feed`), rebuilt on the one-project canvas after the
/// face embed's own-product feed died with it (R1).
///
/// Index = the lamp's position in the fixture's own resolved document —
/// the same space the canvas's `data-lamp` attributes and
/// [`UiPatchCell::source_start`] use. Lamps no cell claims stay
/// [`UNLIT_RGB`]: they never light on hardware, and the neutral reads as
/// geometry rather than as black lamps. No frame yet = empty (no feed —
/// the host keeps the last good one).
///
/// A fixture feeding two outputs decodes every cell from the ONE frame
/// [`UiFixturePatch::frame`] carries (the first output's), the same
/// honest limit the patch bay's fixture face draws with.
pub(crate) fn fixture_live_colors(patch: &UiFixturePatch) -> Vec<[u8; 3]> {
    let Some(frame) = patch.frame.as_ref() else {
        return Vec::new();
    };
    let mut colors = vec![UNLIT_RGB; patch.lamps as usize];
    for cell in &patch.cells {
        for index in 0..cell.lamps {
            let Some(slot) = colors.get_mut(cell.source_start.saturating_add(index) as usize)
            else {
                continue;
            };
            // The run lands on the wire end-first when reversed — the same
            // mapping the patch bay's fixture-side strips draw with.
            let wire_lamp = if cell.reversed {
                cell.wire_start
                    .saturating_add(cell.lamps.saturating_sub(1).saturating_sub(index))
            } else {
                cell.wire_start.saturating_add(index)
            };
            if let Some(rgb) = control_rgb_at_sample(frame, wire_lamp.saturating_mul(3)) {
                *slot = rgb;
            }
        }
    }
    colors
}

/// What a lamp with no frame sample behind it draws as — the same neutral
/// the patch bay's cell strips use, so an unfed lamp reads as geometry
/// rather than as a black lamp.
pub(crate) const UNLIT_RGB: [u8; 3] = [58, 63, 70];

/// Whether the product's sample layout carries anything this renderer can
/// colour a lamp from.
pub(crate) fn control_sample_layout_has_rgb(preview: &UiControlProductPreview) -> bool {
    preview.sample_layout.spans.iter().any(|span| {
        matches!(
            span.encoding,
            ControlSampleEncoding::RgbPixels { count, .. } if count > 0
        )
    })
}

/// Rasterize the lamp field into `buffer` and blit it onto the canvas.
///
/// Geometry matches the renderer this replaces so the view ⇄ edit flip lands
/// on the same framing: lamp centers are fractions of the (already inset)
/// canvas box, and diameter is a percentage of its width floored at 5 CSS px.
fn paint_lamp_canvas(
    canvas_id: &str,
    preview: &UiControlProductPreview,
    live: bool,
    buffer: &mut Vec<u8>,
) -> Result<(), String> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| format!("canvas #{canvas_id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element #{canvas_id} is not a canvas"))?;
    let Some(ControlDisplayLayout::Layout2d(layout)) = preview.display_layout.as_deref() else {
        return Err("control preview has no 2D display layout".to_string());
    };

    let css_width = f64::from(canvas.client_width());
    let css_height = f64::from(canvas.client_height());
    if css_width < 1.0 || css_height < 1.0 {
        return Err(format!("canvas #{canvas_id} has no laid-out box yet"));
    }
    // Backing store in device pixels so dots stay crisp on a retina panel.
    // Capped: a 4x display would quadruple the fill for no visible gain.
    let dpr = web_sys::window()
        .map_or(1.0, |window| window.device_pixel_ratio())
        .clamp(1.0, 3.0);
    let width = (css_width * dpr).round().max(1.0) as u32;
    let height = (css_height * dpr).round().max(1.0) as u32;
    if canvas.width() != width {
        canvas.set_width(width);
    }
    if canvas.height() != height {
        canvas.set_height(height);
    }
    let context = canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "canvas has no 2d context".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has an unexpected type".to_string())?;

    // Transparent black: the frame behind the canvas is black, so unlit
    // background needs no fill and lamp pixels blend as they did under
    // `mix-blend-mode: screen`.
    buffer.clear();
    buffer.resize(width as usize * height as usize * 4, 0);
    let neutral = neutral_lamp_rgb();
    for lamp in &layout.lamps {
        let color = if live {
            control_rgb_at_sample(preview, lamp.sample_start).unwrap_or([0, 0, 0])
        } else {
            neutral
        };
        // A black lamp screen-blends to nothing over the black frame — the
        // same nothing the div renderer drew, at none of the cost.
        if color == [0, 0, 0] {
            continue;
        }
        let diameter_pct = f64::from(lamp.radius.max(0.006) * 96.0).clamp(3.5, 18.0);
        let radius = (diameter_pct / 100.0 * css_width).max(5.0) / 2.0 * dpr;
        let center_x = f64::from(lamp.center[0].clamp(0.0, 1.0)) * f64::from(width);
        let center_y = f64::from(lamp.center[1].clamp(0.0, 1.0)) * f64::from(height);
        paint_lamp_dot(buffer, width, height, [center_x, center_y], radius, color);
    }

    let image =
        web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(buffer), width, height)
            .map_err(|error| format!("build ImageData: {error:?}"))?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|error| format!("putImageData: {error:?}"))?;
    // Paint runs async after mount, so an unpainted canvas is a possible page
    // state; the story-capture ready-wait polls this marker so a baseline can
    // never freeze that state (set imperatively — Dioxus never writes this
    // attribute, so the vdom won't clear it on re-render).
    canvas
        .set_attribute("data-preview-painted", "1")
        .map_err(|error| format!("mark painted: {error:?}"))?;
    Ok(())
}

/// One lamp: solid to 75% of the radius, then a linear fade — the radial
/// gradient the div renderer carried in CSS — screen-blended into `buffer`
/// so overlapping lamps add up instead of overwriting.
fn paint_lamp_dot(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    center: [f64; 2],
    radius: f64,
    color: [u8; 3],
) {
    let [center_x, center_y] = center;
    let x0 = (center_x - radius).floor().max(0.0) as u32;
    let y0 = (center_y - radius).floor().max(0.0) as u32;
    let x1 = (center_x + radius)
        .ceil()
        .clamp(0.0, f64::from(width) - 1.0) as u32;
    let y1 = (center_y + radius)
        .ceil()
        .clamp(0.0, f64::from(height) - 1.0) as u32;
    let core = radius * 0.75;
    let fade = (radius - core).max(f64::EPSILON);
    let radius_sq = radius * radius;

    for y in y0..=y1 {
        let dy = f64::from(y) + 0.5 - center_y;
        for x in x0..=x1 {
            let dx = f64::from(x) + 0.5 - center_x;
            let distance_sq = dx * dx + dy * dy;
            if distance_sq > radius_sq {
                continue;
            }
            let coverage = if distance_sq <= core * core {
                1.0
            } else {
                (radius - distance_sq.sqrt()) / fade
            };
            let index = (y as usize * width as usize + x as usize) * 4;
            for channel in 0..3 {
                let source = f64::from(color[channel]) * coverage;
                let destination = f64::from(buffer[index + channel]);
                buffer[index + channel] =
                    (255.0 - (255.0 - destination) * (255.0 - source) / 255.0).round() as u8;
            }
            buffer[index + 3] = 255;
        }
    }
}

/// One lamp's display colour, decoded from a control preview's own samples
/// — the sample-layout walk (span, encoding, colour order) plus the linear
/// → sRGB transfer, shared with the patch bay's cell strips so the bay and
/// the lamp field can never disagree about a colour.
pub(crate) fn control_rgb_at_sample(
    preview: &UiControlProductPreview,
    sample_start: u32,
) -> Option<[u8; 3]> {
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
        Some(linear_unorm16_to_srgb8(u16::from_le_bytes([lo, hi])))
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

/// Encode one LINEAR unorm16 control sample as display sRGB8.
///
/// Control samples ride the wire in linear light (the engine renders
/// `Unorm16` and ships the buffer raw); the render-TEXTURE probe converts
/// engine-side (`rgba16_linear_to_srgb8`), so until this conversion the two
/// previews disagreed — linear-as-sRGB reads darker and oversaturated (the
/// 2026-08-05 G1 finding: "much more saturated than the shader"). Same
/// transfer as the engine's LUT, float form (non-embedded client; ~4.5k
/// calls/frame is nothing here).
fn linear_unorm16_to_srgb8(value: u16) -> u8 {
    let linear = value as f32 / 65535.0;
    let srgb = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0 + 0.5) as u8
}

fn install_lamp_resize_observer(
    observer: &Rc<RefCell<Option<LampResizeObserver>>>,
    canvas_id: &str,
    mut size_epoch: Signal<u64>,
) {
    if observer.borrow().is_some() {
        return;
    }
    let installed = LampResizeObserver::install(canvas_id, move || {
        let next = *size_epoch.peek() + 1;
        size_epoch.set(next);
    });
    *observer.borrow_mut() = installed;
}

/// Watches the canvas box: a layout change (window resize, card expand)
/// moves no prop, so nothing else would report that the backing store is now
/// the wrong size. Disconnects on drop.
struct LampResizeObserver {
    observer: web_sys::ResizeObserver,
    _callback: Closure<dyn FnMut(web_sys::js_sys::Array)>,
}

impl LampResizeObserver {
    fn install(canvas_id: &str, mut on_resize: impl FnMut() + 'static) -> Option<Self> {
        let element = web_sys::window()?
            .document()?
            .get_element_by_id(canvas_id)?;
        // Seeded from the box the caller just painted against, so the
        // observer's initial delivery is not a spurious repaint.
        let last = Cell::new((element.client_width(), element.client_height()));
        let observed = element.clone();
        let callback = Closure::<dyn FnMut(web_sys::js_sys::Array)>::new(
            move |_entries: web_sys::js_sys::Array| {
                let now = (observed.client_width(), observed.client_height());
                if now != last.get() {
                    last.set(now);
                    on_resize();
                }
            },
        );
        let observer = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()).ok()?;
        observer.observe(&element);
        Some(Self {
            observer,
            _callback: callback,
        })
    }
}

impl Drop for LampResizeObserver {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{ControlExtent, ControlSampleLayout, ControlSampleSpan, UiPatchCell};

    use super::*;

    /// Endpoints exact; the linear midtone must brighten (linear 0.2 →
    /// sRGB ≈ 0.48), which is the whole point of the conversion — shown
    /// raw it read dark/oversaturated next to the shader hero.
    #[test]
    fn linear_to_srgb_matches_the_transfer() {
        assert_eq!(linear_unorm16_to_srgb8(0), 0);
        assert_eq!(linear_unorm16_to_srgb8(65535), 255);
        let mid = linear_unorm16_to_srgb8((0.2f32 * 65535.0) as u16);
        assert!(
            (123..=125).contains(&mid),
            "linear 0.2 ≈ sRGB 124, got {mid}"
        );
    }

    /// A wire frame whose lamp `n` carries full-red at `n == 0`, full-green
    /// at `n == 1`, … cycling — distinguishable colors per wire position.
    fn wire_frame(lamps: u32) -> UiControlProductPreview {
        let mut bytes = Vec::with_capacity(lamps as usize * 6);
        for lamp in 0..lamps {
            let mut rgb = [0_u16; 3];
            rgb[(lamp % 3) as usize] = 65535;
            for sample in rgb {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
        }
        UiControlProductPreview {
            revision: 1,
            extent: ControlExtent::new(1, lamps * 3),
            sample_format: lpa_studio_core::UiControlSampleFormat::U16,
            sample_layout: ControlSampleLayout {
                spans: vec![ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: lamps * 3,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: lamps,
                        color_order: ColorOrder::Rgb,
                    },
                }],
            },
            display_layout: None,
            bytes: bytes.into(),
        }
    }

    fn run(source_start: u32, lamps: u32, wire_start: u32, reversed: bool) -> UiPatchCell {
        UiPatchCell {
            source_start,
            lamps,
            wire_start,
            reversed,
            ..UiPatchCell::default()
        }
    }

    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    /// The feed contract end to end: doc index = `source_start + offset`,
    /// wire index rebased per cell (reversed runs read the wire end-first),
    /// unclaimed lamps stay the unlit neutral, no frame = no feed.
    #[test]
    fn fixture_live_colors_rebase_doc_lamps_onto_the_wire() {
        let mut patch = UiFixturePatch {
            lamps: 7,
            cells: vec![
                // Doc 0..3 land forward at wire 3..6: R G B from lamp 3 → B R G.
                run(0, 3, 3, false),
                // Doc 4..7 land REVERSED at wire 0..3: doc 4 reads wire 2.
                run(4, 3, 0, true),
            ],
            frame: Some(wire_frame(6)),
            single_output: true,
        };
        assert_eq!(
            fixture_live_colors(&patch),
            vec![
                RED, GREEN, BLUE,      // wire 3,4,5 (cycle restarts at wire 3 = red)
                UNLIT_RGB, // doc 3: no cell claims it
                BLUE, GREEN, RED, // wire 2,1,0
            ]
        );
        patch.frame = None;
        assert_eq!(
            fixture_live_colors(&patch),
            Vec::<[u8; 3]>::new(),
            "no frame = no feed (the host keeps its last good one)"
        );
    }

    /// A cell reaching past the frame's samples keeps the unlit neutral
    /// (never panics, never wraps), and a cell reaching past the fixture's
    /// own width is clipped.
    #[test]
    fn fixture_live_colors_survive_out_of_range_cells() {
        let patch = UiFixturePatch {
            lamps: 2,
            cells: vec![run(0, 2, 5, false), run(1, 9, 1, false)],
            frame: Some(wire_frame(6)),
            single_output: true,
        };
        let colors = fixture_live_colors(&patch);
        assert_eq!(colors.len(), 2);
        // Doc 0 = wire 5 (blue); doc 1: wire 6 is past the frame, so the
        // second cell's wire 1 (green) wins the contested slot.
        assert_eq!(colors[0], BLUE);
        assert_eq!(colors[1], GREEN);
    }
}
