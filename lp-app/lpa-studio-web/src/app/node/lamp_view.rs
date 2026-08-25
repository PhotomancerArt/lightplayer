//! Canvas renderer for a control product's lamps — the product display.
//!
//! A control product is up to dome scale today (1500 lamps) and Radiance
//! scale tomorrow (~30k), so one DOM node per lamp is not an option: the
//! positioned-`<span>` renderer this replaced rebuilt the whole lamp field
//! through the vdom on every live frame. Here the field is one `<canvas>`,
//! and each lamp is a **voronoi cell** — the same seam-inset polygons the
//! mapping canvas draws ([`point_cells`]), so the product display speaks
//! the design language instead of a row of blurred dots.
//!
//! Geometry is scale-free and computed ONCE per layout: cell polygons live
//! in the layout's own hint space (all lengths derived from the field's
//! median lamp pitch and its declared footprint — never an absolute pixel
//! or percentage clamp; doc units are arbitrary, see the
//! canvas-object-renderables ADR), cached as `Path2d`s keyed by the layout
//! `Rc`'s identity. A live frame is then one `clearRect` plus one solid
//! `fill` per lit cell under one aspect-FIT transform — the cell bounds
//! uniformly scaled and centered in the box (fit, never stretch, and an
//! edge cell never clips; both G1 rulings) — no pixel buffer, no
//! per-lamp math. Cells tile without overlap by construction, so plain
//! source-over fills replace the old screen blending; unlit cells stay
//! transparent and the black frame behind the canvas shows through, which
//! is the same nothing the dot renderer drew.
//!
//! View mode is a product display, not a wiring tool — no numbers, no
//! arrows. Those are authoring instruments and live in the mapping editor.
//!
//! Upgrade path: the per-frame fill count is the cost that scales, and at
//! Radiance scale this component grows a WebGL/instanced backend behind
//! the same props (the cached cell polygons carry over as instanced fans).
//! Not yet — 1500 path fills are well under a frame, and a GPU context per
//! card is its own bill.
//!
//! [`point_cells`]: lpa_mapping_editor::point_cells

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_mapping_editor::{neutral_lamp_rgb, point_cells};
use lpa_studio_core::{
    ColorOrder, ControlDisplayLayout, ControlSampleEncoding, UiControlProductPreview,
    UiPatchSurface, UiPatchSurfaceFixture,
};
use wasm_bindgen::{JsCast, closure::Closure};

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
    // The layout's cell polygons as Path2ds, rebuilt only when the
    // producer publishes a new layout — a live frame reuses them and
    // allocates nothing.
    let cell_paths = use_hook(|| Rc::new(RefCell::new(None::<LampCellPaths>)));
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
        let cell_paths = cell_paths.clone();
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
                    let mut paths = cell_paths.borrow_mut();
                    paint_lamp_canvas(&canvas_id, &preview, live, &mut paths)
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
/// geometry rather than as black lamps. No run could be read at all = empty
/// (no feed — the host keeps the last good one).
///
/// Each run decodes against ITS OWN output's published frame
/// ([`cell_frame`]), never against one frame for the whole fixture: a
/// fixture can drive two boxes (the mini dome drives both), and wire lamp
/// 39 of one output is a different strand from wire lamp 39 of the other.
/// Reading them all through one wire made two objects wear one object's
/// light — the selected sector chased on its own sprite AND on whichever
/// sector happened to sit at the same wire lamps of the other box.
pub(crate) fn fixture_live_colors(
    surface: &UiPatchSurface,
    fixture: &UiPatchSurfaceFixture,
) -> Vec<[u8; 3]> {
    let patch = &fixture.patch;
    let mut colors = vec![UNLIT_RGB; patch.lamps as usize];
    let mut fed = false;
    for cell in &patch.cells {
        let Some(frame) = cell_frame(surface, &cell.id) else {
            continue;
        };
        fed = true;
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
    if fed { colors } else { Vec::new() }
}

/// The published frame of the output whose bay carries this RUN.
///
/// Per run, not per fixture: `UiFixturePatch::frame` carries only the first
/// output a fixture drives, which is enough for the bay's own face and a lie
/// for anything that must show one object's light. The wire a run landed on
/// is the only wire that can answer for it — and a run keeps ONE id across
/// both faces (and across a clip into two ports), so the bay that holds it
/// names its output.
pub(crate) fn cell_frame<'a>(
    surface: &'a UiPatchSurface,
    cell_id: &str,
) -> Option<&'a UiControlProductPreview> {
    surface
        .outputs
        .iter()
        .find(|output| {
            output
                .bay
                .ports
                .iter()
                .any(|port| port.cells.iter().any(|cell| cell.id == cell_id))
        })?
        .bay
        .frame
        .as_ref()
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

/// One lamp field's cell geometry: seam-inset voronoi polygons in the
/// layout's hint space, each carrying the `sample_start` its live colour
/// decodes from. Pure data — the `Path2d` form the painter fills is built
/// from this by [`LampCellPaths`].
pub(crate) struct LampFieldGeometry {
    /// Hint-space bbox over every cell VERTEX (`[min_x, min_y, width,
    /// height]`) — what the draw-time transform aspect-FITS into the
    /// canvas box (G1 rulings): framing the polygons rather than the
    /// `[0, hint]` extent means an edge lamp's cell can never be cut off
    /// by the canvas border, and a field that occupies a sliver of its
    /// texture (the fyeah sign in a square texture) fills its box.
    /// All-zero when the layout has no drawable cell.
    pub bounds: [f64; 4],
    /// Per displayed lamp: its sample offset and its cell polygon
    /// (possibly empty when coincident lamps shared all its territory).
    pub cells: Vec<(u32, Vec<[f32; 2]>)>,
}

/// Compute a layout's cell geometry — once per layout, not per frame.
///
/// Positions leave normalized space for **hint space** (`x·width_hint`,
/// `y·height_hint`) so distances mean one thing on both axes, and every
/// length derives from the field's own numbers: cell radius is 0.92 × the
/// median nearest-neighbour distance ([`point_cells`]), floored by the
/// footprint the layout itself declares (`ControlLamp2d::radius` is
/// normalized against the larger hint dimension). No absolute clamps —
/// the old dot renderer's %-of-box diameter and 5 px floor assumed a
/// scale the layout never promised.
pub(crate) fn lamp_field_geometry(layout: &lpa_studio_core::ControlLayout2d) -> LampFieldGeometry {
    let hint = [
        f64::from(layout.width_hint.max(1)),
        f64::from(layout.height_hint.max(1)),
    ];
    let positions: Vec<[f32; 2]> = layout
        .lamps
        .iter()
        .map(|lamp| {
            [
                lamp.center[0].clamp(0.0, 1.0) * hint[0] as f32,
                lamp.center[1].clamp(0.0, 1.0) * hint[1] as f32,
            ]
        })
        .collect();
    // The footprint floor in hint units. Median across lamps: today every
    // producer declares one uniform radius, and a mixed field should not
    // let one outsized declaration inflate everyone else's floor.
    let floor = {
        let mut radii: Vec<f32> = layout.lamps.iter().map(|lamp| lamp.radius).collect();
        radii.sort_by(f32::total_cmp);
        let max_hint = hint[0].max(hint[1]) as f32;
        radii
            .get(radii.len() / 2)
            .map_or(f32::EPSILON, |radius| (radius * max_hint).max(f32::EPSILON))
    };
    let cells: Vec<(u32, Vec<[f32; 2]>)> = point_cells(&positions, floor)
        .into_iter()
        .zip(&layout.lamps)
        .map(|(cell, lamp)| (lamp.sample_start, cell.polygon))
        .collect();
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for vertex in cells.iter().flat_map(|(_, polygon)| polygon) {
        min[0] = min[0].min(f64::from(vertex[0]));
        min[1] = min[1].min(f64::from(vertex[1]));
        max[0] = max[0].max(f64::from(vertex[0]));
        max[1] = max[1].max(f64::from(vertex[1]));
    }
    let bounds = if min[0].is_finite() {
        [min[0], min[1], max[0] - min[0], max[1] - min[1]]
    } else {
        [0.0; 4]
    };
    LampFieldGeometry { bounds, cells }
}

/// A layout's cells as ready-to-fill `Path2d`s, keyed by the layout
/// `Rc`'s identity so [`LampView`] rebuilds geometry only when the
/// producer publishes a new layout — never on a live frame.
pub(crate) struct LampCellPaths {
    /// `Rc::as_ptr` of the `ControlDisplayLayout` these paths were built
    /// from (identity, not contents — same contract as [`LampPaintKey`]).
    layout_key: usize,
    /// Hint-space bbox of every cell vertex — see
    /// [`LampFieldGeometry::bounds`].
    bounds: [f64; 4],
    cells: Vec<(u32, web_sys::Path2d)>,
}

impl LampCellPaths {
    /// Build (or reuse) the paths for `preview`'s current layout.
    pub(crate) fn ensure<'a>(
        slot: &'a mut Option<LampCellPaths>,
        preview: &UiControlProductPreview,
    ) -> Result<&'a LampCellPaths, String> {
        let Some(layout_rc) = preview.display_layout.as_ref() else {
            return Err("control preview has no 2D display layout".to_string());
        };
        let layout_key = Rc::as_ptr(layout_rc) as *const u8 as usize;
        if slot
            .as_ref()
            .is_none_or(|paths| paths.layout_key != layout_key)
        {
            let ControlDisplayLayout::Layout2d(layout) = layout_rc.as_ref();
            let geometry = lamp_field_geometry(layout);
            let mut cells = Vec::with_capacity(geometry.cells.len());
            for (sample_start, polygon) in geometry.cells {
                if polygon.len() < 3 {
                    continue;
                }
                let path =
                    web_sys::Path2d::new().map_err(|error| format!("build Path2d: {error:?}"))?;
                path.move_to(f64::from(polygon[0][0]), f64::from(polygon[0][1]));
                for vertex in &polygon[1..] {
                    path.line_to(f64::from(vertex[0]), f64::from(vertex[1]));
                }
                path.close_path();
                cells.push((sample_start, path));
            }
            *slot = Some(LampCellPaths {
                layout_key,
                bounds: geometry.bounds,
                cells,
            });
        }
        Ok(slot.as_ref().expect("just ensured"))
    }

    /// Fill every lit cell into the `dest` box (`[x, y, width, height]`
    /// in device pixels) on `context`, aspect-FITTING the cell bounds —
    /// uniform scale, centered, never stretched (G1: a squished dome
    /// read as an ellipse) and never clipped (the bounds include every
    /// cell vertex, so an edge lamp's cell cannot be cut off by the
    /// border). Background is the caller's: the live canvas clears to
    /// transparent (the black frame behind it shows through), the poster
    /// fills black first — either way this only paints cells, so both
    /// stay the same picture.
    pub(crate) fn fill(
        &self,
        context: &web_sys::CanvasRenderingContext2d,
        preview: &UiControlProductPreview,
        live: bool,
        dest: [f64; 4],
    ) -> Result<(), String> {
        let [x, y, width, height] = dest;
        let [bounds_x, bounds_y, bounds_w, bounds_h] = self.bounds;
        if bounds_w <= 0.0 || bounds_h <= 0.0 {
            return Ok(());
        }
        let scale = (width / bounds_w).min(height / bounds_h);
        context
            .set_transform(
                scale,
                0.0,
                0.0,
                scale,
                x + (width - scale * bounds_w) / 2.0 - scale * bounds_x,
                y + (height - scale * bounds_h) / 2.0 - scale * bounds_y,
            )
            .map_err(|error| format!("set transform: {error:?}"))?;
        let neutral = neutral_lamp_rgb();
        let mut style = String::new();
        for (sample_start, path) in &self.cells {
            let color = if live {
                control_rgb_at_sample(preview, *sample_start).unwrap_or([0, 0, 0])
            } else {
                neutral
            };
            // A black cell over the black frame is the same nothing the
            // dot renderer drew, at none of the cost.
            if color == [0, 0, 0] {
                continue;
            }
            style.clear();
            use std::fmt::Write as _;
            let _ = write!(style, "rgb({},{},{})", color[0], color[1], color[2]);
            context.set_fill_style_str(&style);
            context.fill_with_path_2d(path);
        }
        context
            .reset_transform()
            .map_err(|error| format!("reset transform: {error:?}"))?;
        Ok(())
    }
}

/// Size the canvas backing store and fill the lamp cells onto it.
fn paint_lamp_canvas(
    canvas_id: &str,
    preview: &UiControlProductPreview,
    live: bool,
    paths: &mut Option<LampCellPaths>,
) -> Result<(), String> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| format!("canvas #{canvas_id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element #{canvas_id} is not a canvas"))?;

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

    let paths = LampCellPaths::ensure(paths, preview)?;
    // Transparent clear: the frame behind the canvas is black, so unlit
    // territory needs no fill — the same backdrop the poster bakes in.
    context.reset_transform().ok();
    context.clear_rect(0.0, 0.0, f64::from(width), f64::from(height));
    paths.fill(
        &context,
        preview,
        live,
        [0.0, 0.0, f64::from(width), f64::from(height)],
    )?;
    // Paint runs async after mount, so an unpainted canvas is a possible page
    // state; the story-capture ready-wait polls this marker so a baseline can
    // never freeze that state (set imperatively — Dioxus never writes this
    // attribute, so the vdom won't clear it on re-render).
    canvas
        .set_attribute("data-preview-painted", "1")
        .map_err(|error| format!("mark painted: {error:?}"))?;
    Ok(())
}

/// One lamp's display colour, decoded from a control preview's own samples
/// — the sample-layout walk (span, encoding, colour order) plus the linear
/// → sRGB transfer, shared with the patch bay's cell strips so the bay and
/// the lamp field can never disagree about a colour.
pub(crate) fn control_rgb_at_sample(
    preview: &UiControlProductPreview,
    sample_start: u32,
) -> Option<[u8; 3]> {
    let color_order = control_color_order_at_sample(preview, sample_start)?;
    Some(order_channels(
        color_order,
        sample_triple(preview, sample_start)?,
    ))
}

/// The colour order the product's OWN layout declares at `sample_start`, or
/// `None` where it declares none — a `Raw` run, or wire the layout says
/// nothing about at all.
///
/// A wire's layout only covers the stretches a producer was PLACED on: free
/// lamps between runs carry real samples (the engine's selection highlight
/// paints them) that no span names. A reader that wants those lamps has to
/// bring its own order — see [`wire_lamp_rgb`] and the panel's A1 decode
/// line.
pub(crate) fn control_color_order_at_sample(
    preview: &UiControlProductPreview,
    sample_start: u32,
) -> Option<ColorOrder> {
    let span = preview.sample_layout.spans.iter().find(|span| {
        matches!(span.encoding, ControlSampleEncoding::RgbPixels { .. })
            && sample_start >= span.start
            && sample_start.saturating_add(3) <= span.start.saturating_add(span.len)
            && (sample_start - span.start).is_multiple_of(3)
    })?;
    match span.encoding {
        ControlSampleEncoding::RgbPixels { color_order, .. } => Some(color_order),
        ControlSampleEncoding::Raw => None,
    }
}

/// One WIRE lamp's colour, decoded under the layout's own order where the
/// layout declares one and under `assumed` where it does not (A1).
///
/// The honest shape of a walk-up port strip: mapped stretches decode by what
/// the wire says about itself, and the free stretches between them decode by
/// the lamp type of the fixture the user is about to put there. `None` means
/// the sample is not on the wire at all (past the published extent) — a lamp
/// with no signal behind it, not a black lamp.
pub(crate) fn wire_lamp_rgb(
    preview: &UiControlProductPreview,
    wire_lamp: u32,
    assumed: ColorOrder,
) -> Option<[u8; 3]> {
    let sample_start = wire_lamp.saturating_mul(3);
    let order = control_color_order_at_sample(preview, sample_start).unwrap_or(assumed);
    Some(order_channels(order, sample_triple(preview, sample_start)?))
}

/// Three consecutive samples as display sRGB8, in the order they ride the
/// wire (no colour interpretation yet).
fn sample_triple(preview: &UiControlProductPreview, sample_start: u32) -> Option<[u8; 3]> {
    let sample = |offset: u32| -> Option<u8> {
        let index = sample_start.checked_add(offset)? as usize;
        let byte_index = index.checked_mul(2)?;
        let lo = *preview.bytes.get(byte_index)?;
        let hi = *preview.bytes.get(byte_index + 1)?;
        Some(linear_unorm16_to_srgb8(u16::from_le_bytes([lo, hi])))
    };
    Some([sample(0)?, sample(1)?, sample(2)?])
}

/// Wire-order channels back into RGB.
fn order_channels(color_order: ColorOrder, [a, b, c]: [u8; 3]) -> [u8; 3] {
    match color_order {
        ColorOrder::Rgb => [a, b, c],
        ColorOrder::Grb => [b, a, c],
        ColorOrder::Rbg => [a, c, b],
        ColorOrder::Gbr => [c, a, b],
        ColorOrder::Brg => [b, c, a],
        ColorOrder::Bgr => [c, b, a],
    }
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
pub(crate) fn linear_unorm16_to_srgb8(value: u16) -> u8 {
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
    use lpa_studio_core::{
        ControlExtent, ControlSampleLayout, ControlSampleSpan, NodeId, UiFixturePatch, UiPatchBay,
        UiPatchCell, UiPatchPort, UiPatchSurfaceOutput,
    };

    use super::*;

    /// The layout→geometry contract: positions leave normalized space
    /// for hint space (per-axis), every cell keeps its lamp's
    /// `sample_start`, and the footprint floor comes from the layout's
    /// own numbers (radius × the larger hint dimension) — no absolute
    /// clamps anywhere.
    #[test]
    fn lamp_field_geometry_scales_into_hint_space() {
        let lamps: Vec<lpa_studio_core::ControlLamp2d> = (0..4)
            .map(|i| lpa_studio_core::ControlLamp2d {
                lamp_index: i,
                sample_start: i * 3,
                center: [i as f32 * 0.25, 0.5],
                radius: 0.02,
            })
            .collect();
        let layout =
            lpa_studio_core::ControlLayout2d::new(lpa_studio_core::Revision(7), 200, 100, lamps);
        let geometry = lamp_field_geometry(&layout);
        assert_eq!(geometry.cells.len(), 4);
        // The bounds frame the CELLS, not the [0, hint] extent: seeds
        // span x 0..150 at y 50, radius 0.92 × 50 — the bbox reaches
        // (inset) beyond the first and last seed on both axes.
        let [bounds_x, bounds_y, bounds_w, bounds_h] = geometry.bounds;
        assert!(bounds_x < 0.0 && bounds_x > -50.0, "bx {bounds_x}");
        assert!(bounds_y < 50.0 && bounds_y > 0.0, "by {bounds_y}");
        assert!(
            bounds_w > 150.0 && bounds_w < 250.0,
            "bw {bounds_w} spans the seeds plus cell reach"
        );
        assert!(bounds_h > 0.0 && bounds_h < 100.0, "bh {bounds_h}");
        for (index, (sample_start, polygon)) in geometry.cells.iter().enumerate() {
            assert_eq!(*sample_start, index as u32 * 3);
            assert!(!polygon.is_empty(), "cell {index} vanished");
            // Pitch in hint space is 0.25 × 200 = 50; a cell reaches at
            // most its radius (0.92 × 50, inset) from its seed — hint-
            // space scale, not a % of some box.
            let seed = [index as f32 * 50.0, 50.0];
            for v in polygon {
                let dx = v[0] - seed[0];
                let dy = v[1] - seed[1];
                let d = f64::from(dx * dx + dy * dy).sqrt();
                assert!(d <= 0.92 * 50.0 + 1e-3, "cell {index} vertex {v:?} at {d}");
            }
        }
        // A sparse field falls back to the declared footprint: radius
        // 0.02 × max(200, 100) = 4 in hint units.
        let lone = lpa_studio_core::ControlLayout2d::new(
            lpa_studio_core::Revision(7),
            200,
            100,
            vec![lpa_studio_core::ControlLamp2d {
                lamp_index: 0,
                sample_start: 0,
                center: [0.5, 0.5],
                radius: 0.02,
            }],
        );
        let geometry = lamp_field_geometry(&lone);
        let (_, polygon) = &geometry.cells[0];
        for v in polygon {
            let dx = f64::from(v[0] - 100.0);
            let dy = f64::from(v[1] - 50.0);
            let d = (dx * dx + dy * dy).sqrt();
            assert!((d - 4.0 * 0.9).abs() < 1e-3, "footprint floor, got {d}");
        }
    }

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
            // The bay's run identity (`node:output:source:wire`) — what a
            // run is looked up by on the wire it landed on.
            id: format!("2:0:{source_start}:{wire_start}"),
            source_start,
            lamps,
            wire_start,
            reversed,
            ..UiPatchCell::default()
        }
    }

    /// One output: a single port carrying `cells`, with `frame` published
    /// (or not). The surface's outputs are what a run's frame is resolved
    /// through, so every test wire lists the runs that landed on it.
    fn wire(
        node: u32,
        label: &str,
        frame: Option<UiControlProductPreview>,
        cells: &[UiPatchCell],
    ) -> UiPatchSurfaceOutput {
        UiPatchSurfaceOutput {
            node: NodeId::new(node),
            label: label.to_string(),
            bay: UiPatchBay {
                ports: vec![UiPatchPort {
                    key: 0,
                    pin_label: "IO2".to_string(),
                    start: 0,
                    lamps: 64,
                    cells: cells.to_vec(),
                }],
                frame,
                ..UiPatchBay::default()
            },
            ..UiPatchSurfaceOutput::default()
        }
    }

    /// The fixture under test, on the wires its runs landed on.
    fn surface(patch: UiFixturePatch, outputs: Vec<UiPatchSurfaceOutput>) -> UiPatchSurface {
        UiPatchSurface {
            fixtures: vec![UiPatchSurfaceFixture {
                node: NodeId::new(2),
                label: "dome".to_string(),
                patch,
                ..UiPatchSurfaceFixture::default()
            }],
            outputs,
            ..UiPatchSurface::default()
        }
    }

    const RED: [u8; 3] = [255, 0, 0];
    const GREEN: [u8; 3] = [0, 255, 0];
    const BLUE: [u8; 3] = [0, 0, 255];
    const BLACK: [u8; 3] = [0, 0, 0];

    /// The feed contract end to end: doc index = `source_start + offset`,
    /// wire index rebased per cell (reversed runs read the wire end-first),
    /// unclaimed lamps stay the unlit neutral, no frame = no feed.
    #[test]
    fn fixture_live_colors_rebase_doc_lamps_onto_the_wire() {
        let cells = vec![
            // Doc 0..3 land forward at wire 3..6: R G B from lamp 3 → B R G.
            run(0, 3, 3, false),
            // Doc 4..7 land REVERSED at wire 0..3: doc 4 reads wire 2.
            run(4, 3, 0, true),
        ];
        let patch = UiFixturePatch {
            lamps: 7,
            cells: cells.clone(),
            single_output: true,
            ..UiFixturePatch::default()
        };
        let lit = surface(
            patch.clone(),
            vec![wire(10, "1", Some(wire_frame(6)), &cells)],
        );
        assert_eq!(
            fixture_live_colors(&lit, &lit.fixtures[0]),
            vec![
                RED, GREEN, BLUE,      // wire 3,4,5 (cycle restarts at wire 3 = red)
                UNLIT_RGB, // doc 3: no cell claims it
                BLUE, GREEN, RED, // wire 2,1,0
            ]
        );
        let unfed = surface(patch, vec![wire(10, "1", None, &cells)]);
        assert_eq!(
            fixture_live_colors(&unfed, &unfed.fixtures[0]),
            Vec::<[u8; 3]>::new(),
            "no frame = no feed (the host keeps its last good one)"
        );
    }

    /// A cell reaching past the frame's samples keeps the unlit neutral
    /// (never panics, never wraps), and a cell reaching past the fixture's
    /// own width is clipped.
    #[test]
    fn fixture_live_colors_survive_out_of_range_cells() {
        let cells = vec![run(0, 2, 5, false), run(1, 9, 1, false)];
        let surface = surface(
            UiFixturePatch {
                lamps: 2,
                cells: cells.clone(),
                single_output: true,
                ..UiFixturePatch::default()
            },
            vec![wire(10, "1", Some(wire_frame(6)), &cells)],
        );
        let colors = fixture_live_colors(&surface, &surface.fixtures[0]);
        assert_eq!(colors.len(), 2);
        // Doc 0 = wire 5 (blue); doc 1: wire 6 is past the frame, so the
        // second cell's wire 1 (green) wins the contested slot.
        assert_eq!(colors[0], BLUE);
        assert_eq!(colors[1], GREEN);
    }

    /// THE two-box defect (G1 round 3, the mini dome): a fixture driving two
    /// outputs must read every run from ITS OWN wire.
    ///
    /// The dome's sectors sit at the SAME wire lamps on two different boxes —
    /// wire 0–29 of "1" and wire 0–29 of "Box 2" are different strands. Read
    /// through one frame for the whole fixture, the selected sector's chase
    /// appeared on its own sprite AND on whichever sector happened to share
    /// its wire numbers on the other box: one selection, two objects lit.
    #[test]
    fn each_run_reads_the_frame_of_the_output_it_landed_on() {
        // Sector A on "1", sector B on "Box 2" — same wire lamps, different
        // boxes, exactly as the mini-dome's walk-up patch lands them.
        let a = run(0, 3, 0, false);
        let b = run(3, 3, 0, false);
        let dark = UiControlProductPreview {
            bytes: vec![0_u8; 3 * 3 * 2].into(),
            ..wire_frame(3)
        };
        let surface = surface(
            UiFixturePatch {
                lamps: 6,
                cells: vec![a.clone(), b.clone()],
                single_output: false,
                // The DTO still carries the first output's frame; nothing
                // here may read the second box's lamps through it.
                frame: Some(wire_frame(3)),
                ..UiFixturePatch::default()
            },
            vec![
                wire(10, "1", Some(wire_frame(3)), &[a]),
                wire(11, "Box 2", Some(dark), &[b]),
            ],
        );

        assert_eq!(
            fixture_live_colors(&surface, &surface.fixtures[0]),
            vec![RED, GREEN, BLUE, BLACK, BLACK, BLACK],
            "the lit box paints only its own sector; the dark box's sector stays dark",
        );
    }

    /// One box has published and the other has not: the runs that CAN be
    /// read still light, and the rest keep the unlit neutral rather than
    /// borrowing the other wire's lamps.
    #[test]
    fn a_run_on_an_unpublished_output_stays_unlit() {
        let a = run(0, 3, 0, false);
        let b = run(3, 3, 0, false);
        let surface = surface(
            UiFixturePatch {
                lamps: 6,
                cells: vec![a.clone(), b.clone()],
                single_output: false,
                frame: Some(wire_frame(3)),
                ..UiFixturePatch::default()
            },
            vec![
                wire(10, "1", Some(wire_frame(3)), &[a]),
                wire(11, "Box 2", None, &[b]),
            ],
        );

        assert_eq!(
            fixture_live_colors(&surface, &surface.fixtures[0]),
            vec![RED, GREEN, BLUE, UNLIT_RGB, UNLIT_RGB, UNLIT_RGB],
        );
    }

    /// A1's decode: the wire's own layout wins wherever it declares an
    /// order, and the caller's assumption covers only the stretches it says
    /// nothing about — the free lamps a walk-up user is about to spend.
    /// Past the published extent there is no lamp at all, not a black one.
    #[test]
    fn undeclared_wire_decodes_under_the_assumed_lamp_type() {
        let mut frame = wire_frame(4);
        // Only the first two lamps are placed, and that run is GRB.
        frame.sample_layout.spans = vec![ControlSampleSpan {
            row: 0,
            start: 0,
            len: 6,
            encoding: ControlSampleEncoding::RgbPixels {
                count: 2,
                color_order: ColorOrder::Grb,
            },
        }];
        // Lamp 0's samples are (65535, 0, 0): GRB reads that as green.
        assert_eq!(wire_lamp_rgb(&frame, 0, ColorOrder::Rgb), Some(GREEN));
        assert_eq!(
            control_color_order_at_sample(&frame, 0),
            Some(ColorOrder::Grb)
        );
        // Lamp 3's samples are (65535, 0, 0) too, but nothing declares them:
        // the assumption decides, and it is the caller's to state.
        assert_eq!(control_color_order_at_sample(&frame, 9), None);
        assert_eq!(wire_lamp_rgb(&frame, 3, ColorOrder::Rgb), Some(RED));
        assert_eq!(wire_lamp_rgb(&frame, 3, ColorOrder::Grb), Some(GREEN));
        assert_eq!(
            control_rgb_at_sample(&frame, 9),
            None,
            "the layout-only decode still refuses to invent an order"
        );
        assert_eq!(wire_lamp_rgb(&frame, 9, ColorOrder::Rgb), None);
    }
}
