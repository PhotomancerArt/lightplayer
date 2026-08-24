//! The reactive lamp strip: one canvas, two presentations, one rule.
//!
//! Every per-lamp readout in Studio paints the SAME picture — a row of lamp
//! values in some reading order — and the only thing that changes with the
//! surface is how much room each lamp gets. The spike's ruling (§4,
//! `spikes/patching-controls/index.html:310-320`): **bulbs while each lamp
//! gets ≥ 7px, gradient beyond**. Above the threshold a strip draws discrete
//! rounded blocks with a hairline of track between them, because at that
//! size a lamp is a THING; below it the blocks would be sub-pixel lies, so
//! the strip becomes the one-texel-per-lamp pixelated presentation the patch
//! bay's cells have always used.
//!
//! Generalized out of the patch bay's `PatchCellStrip` (D34a) so the panel's
//! strips and the bay's cells are one code path. The bay pins itself to
//! [`StripMode::Gradient`]: a cell is a picture of discrete lamps at cell
//! scale and has always drawn that way, and the reactive rule is a
//! PANEL-scale decision.
//!
//! **What the strip does not do.** It has no opinion about where its colours
//! came from — wire bytes, a fixture-space decode, or the controller's
//! unmapped-chase preview. Callers decode; the strip paints. Its repaint key
//! is the DIGEST of the colours it was handed plus the box it measured, so a
//! live frame that changes nothing repaints nothing and no caller needs a
//! frame revision to hand it.
//!
//! **Animation.** There is none here. Every picture this strip draws — the
//! published wire, a fixture-space decode, the core-computed chase of an
//! unmapped object — arrives as new colours from the caller, on the
//! controller's own frame cadence. No rAF loop, no timer, no clock: the
//! harness browser pane (`document.hidden`, zero rAF) paints on mount like
//! every other pixel canvas in Studio, and a surface with no engine frames
//! renders the same pixels every run.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use wasm_bindgen::{Clamped, JsCast, closure::Closure};

/// Monotonic strip-canvas element ids (one per mounted strip).
static NEXT_STRIP_CANVAS_ID: AtomicU64 = AtomicU64::new(0);

/// The spike's threshold: CSS px per lamp at or above which a strip draws
/// discrete bulbs rather than a gradient.
pub(crate) const BULB_MIN_PX_PER_LAMP: f64 = 7.0;

/// Track left between bulbs, in CSS px (the spike's ~2px gap).
const BULB_GAP_PX: f64 = 2.0;

/// Corner radius of a bulb, in CSS px — the spike's rounded block.
const BULB_RADIUS_PX: f64 = 2.5;

/// The FIRST paint retries while the box measures zero: a paint task can run
/// before the browser flushes layout, and the bulbs/gradient choice is a
/// measurement. The canvas is painted (and marked) on the first attempt
/// regardless — only the MODE waits for a real box, so a story ready-gate
/// never blocks on layout timing. Same budget as the lamp canvas.
const FIRST_PAINT_ATTEMPTS: u32 = 24;
const FIRST_PAINT_RETRY_MS: u32 = 16;

/// Which presentations a strip is allowed to choose between.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StripMode {
    /// Always one texel per lamp, CSS-scaled with `image-rendering:
    /// pixelated` — the patch bay's cells, whose look predates the rule and
    /// is deliberately not up for measurement.
    Gradient,
    /// The spike §4 rule: measure the box and pick.
    Reactive,
}

/// What a strip actually drew, once it had a box to measure.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum StripPresentation {
    /// Discrete rounded blocks, one per lamp.
    Bulbs,
    /// One texel per lamp, scaled.
    #[default]
    Gradient,
}

impl StripPresentation {
    /// The one-word readout (the spike's mode chip) and the DOM stamp.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Bulbs => "bulbs",
            Self::Gradient => "gradient",
        }
    }
}

/// One row of lamps, painted on one canvas.
///
/// `colors` is the strip in READING ORDER — the caller's business entirely
/// (wire order for a port strip, object order for an object strip). An empty
/// list mounts nothing: a strip with no colours has nothing honest to draw,
/// and the host box's track showing through IS "no signal".
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn LampStrip(
    colors: Vec<[u8; 3]>,
    #[props(default = StripMode::Reactive)] mode: StripMode,
    /// Where to report the presentation the measured box chose, for a fact
    /// line that wants to name it. Written only when it CHANGES, so the
    /// report can never loop a render.
    #[props(default)]
    presentation: Option<Signal<StripPresentation>>,
) -> Element {
    let canvas_id = use_hook(|| {
        let id = NEXT_STRIP_CANVAS_ID.fetch_add(1, Ordering::Relaxed);
        format!("lamp-strip-canvas-{id}")
    });
    let painted = use_hook(|| Rc::new(Cell::new(None::<StripPaintKey>)));
    let observer = use_hook(|| Rc::new(RefCell::new(None::<StripResizeObserver>)));
    // A box resize moves no prop, so the observer bumps this instead.
    let size_epoch = use_signal(|| 0_u64);
    // The presentation reported back by the paint, whether or not a caller
    // asked for it: it also decides a CLASS, so the strip always needs one
    // of these even when nothing outside is listening.
    let own = use_signal(StripPresentation::default);
    let shown = presentation.unwrap_or(own);

    let colors = Rc::new(colors);
    let texels = colors.len().max(1) as u32;
    let key = StripPaintKey {
        digest: color_digest(&colors),
        lamps: colors.len(),
        mode,
        size_epoch: size_epoch(),
    };

    if !colors.is_empty() && painted.get() != Some(key) {
        // Paint from a task, not the render pass: the canvas element is
        // stable across diffs, so painting never fights the vdom.
        let paint = StripPaint {
            canvas_id: canvas_id.clone(),
            colors: colors.clone(),
            mode,
            key,
            painted: painted.clone(),
            observer: observer.clone(),
            shown,
            size_epoch,
        };
        spawn(paint.run());
    }

    let mount = StripPaint {
        canvas_id: canvas_id.clone(),
        colors: colors.clone(),
        mode,
        key,
        painted: painted.clone(),
        observer: observer.clone(),
        shown,
        size_epoch,
    };
    rsx! {
        if !colors.is_empty() {
            // `ux-produced-product-pixel-canvas` positions absolutely and
            // fills its container — the caller supplies the sized, relative
            // host box, exactly as the patch bay's cells and the gradient
            // strip already do. Reusing the class also puts these strips
            // under the story-capture ready-wait with no script change.
            canvas {
                id: "{canvas_id}",
                class: strip_class(shown()),
                width: "{texels}",
                height: "1",
                onmounted: move |_| {
                    spawn(mount.clone().run());
                },
            }
        }
    }
}

/// The canvas's classes for a presentation.
///
/// `ux-produced-product-pixel-canvas` always: it is what positions and fills
/// the host box, and it puts every strip under the story-capture ready-wait
/// (which polls that class for `data-preview-painted`) with no script change.
///
/// `ux-box-sized-canvas` only while BULBS are drawn, because only then is the
/// backing store sized from layout. That class is the story gate's assertion
/// that a canvas's bitmap matches the box it was measured for — the
/// clock-face lesson (`docs/defects/2026-08-05-clock-face-baselines-oscillate.md`):
/// a layout-sized canvas painted before the stylesheet lands is a second
/// stable terminal, and baselines alternate between the two. In gradient mode
/// the bitmap is deliberately one texel per lamp, so the same assertion would
/// never hold and the class stays off.
fn strip_class(shown: StripPresentation) -> &'static str {
    match shown {
        StripPresentation::Bulbs => "ux-produced-product-pixel-canvas ux-box-sized-canvas",
        StripPresentation::Gradient => "ux-produced-product-pixel-canvas",
    }
}

/// Everything one paint attempt needs, so the render-time task and the
/// mount-time task are the same code rather than two copies that can drift.
#[derive(Clone)]
struct StripPaint {
    canvas_id: String,
    colors: Rc<Vec<[u8; 3]>>,
    mode: StripMode,
    key: StripPaintKey,
    painted: Rc<Cell<Option<StripPaintKey>>>,
    observer: Rc<RefCell<Option<StripResizeObserver>>>,
    shown: Signal<StripPresentation>,
    size_epoch: Signal<u64>,
}

impl StripPaint {
    async fn run(self) {
        let attempts = if self.painted.get().is_some() {
            1
        } else {
            FIRST_PAINT_ATTEMPTS
        };
        for attempt in 0..attempts {
            let last = attempt + 1 == attempts;
            match paint_strip(&self.canvas_id, &self.colors, self.mode) {
                // Painted, but against a box the browser has not laid out
                // yet: the pixels are on screen and the canvas is marked, so
                // nothing waits on us — we just keep looking for the real
                // box so the MODE settles on a measurement.
                Ok(paint) if !paint.measured && !last => {
                    gloo_timers::future::TimeoutFuture::new(FIRST_PAINT_RETRY_MS).await;
                }
                Ok(paint) => {
                    self.painted.set(Some(self.key));
                    let mut reported = self.shown;
                    if *reported.peek() != paint.shown {
                        reported.set(paint.shown);
                    }
                    if self.mode == StripMode::Reactive {
                        install_strip_resize_observer(
                            &self.observer,
                            &self.canvas_id,
                            self.size_epoch,
                        );
                    }
                    return;
                }
                Err(error) if last => log::debug!("lamp strip paint skipped: {error}"),
                Err(_) => {
                    gloo_timers::future::TimeoutFuture::new(FIRST_PAINT_RETRY_MS).await;
                }
            }
        }
    }
}

/// What the canvas currently shows. The colours are a DIGEST rather than a
/// frame revision: a strip is handed pixels, not a feed, and identical
/// pixels are identical whatever produced them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StripPaintKey {
    digest: u64,
    lamps: usize,
    mode: StripMode,
    size_epoch: u64,
}

/// FNV-1a over the strip's colour bytes — a cheap "same picture?" that costs
/// one pass over data the caller just built anyway.
fn color_digest(colors: &[[u8; 3]]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for color in colors {
        for byte in color {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// The outcome of one paint: what was drawn, and whether the box it was
/// measured against was real.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StripPainted {
    shown: StripPresentation,
    measured: bool,
}

// -- painting ------------------------------------------------------------------

fn paint_strip(
    canvas_id: &str,
    colors: &[[u8; 3]],
    mode: StripMode,
) -> Result<StripPainted, String> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| format!("canvas #{canvas_id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element #{canvas_id} is not a canvas"))?;
    let context = canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "canvas has no 2d context".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has an unexpected type".to_string())?;

    // The BOUNDING RECT, not `clientWidth`: the story gate asserts a
    // layout-sized canvas's bitmap is `round(rect.width * dpr)`, and an
    // integer-rounded `clientWidth` can land a pixel off that and hang the
    // gate forever waiting for a repaint that would never differ.
    let rect = canvas.get_bounding_client_rect();
    let css_width = rect.width();
    let css_height = rect.height();
    let measured = css_width > 0.0 && css_height > 0.0;
    let lamps = colors.len().max(1);
    let shown = match mode {
        StripMode::Gradient => StripPresentation::Gradient,
        StripMode::Reactive if measured && css_width / lamps as f64 >= BULB_MIN_PX_PER_LAMP => {
            StripPresentation::Bulbs
        }
        StripMode::Reactive => StripPresentation::Gradient,
    };

    match shown {
        StripPresentation::Bulbs => {
            paint_bulbs(&canvas, &context, colors, css_width, css_height)?;
        }
        StripPresentation::Gradient => paint_gradient(&canvas, &context, colors)?,
    }
    canvas
        .set_attribute("data-strip-mode", shown.label())
        .map_err(|error| format!("mark mode: {error:?}"))?;
    canvas
        .set_attribute("data-preview-painted", "1")
        .map_err(|error| format!("mark painted: {error:?}"))?;
    Ok(StripPainted { shown, measured })
}

/// One texel per lamp, blitted and CSS-scaled (`image-rendering: pixelated`).
///
/// The presentation the patch bay's cells have always used: below the
/// threshold a lamp is smaller than a pixel-and-a-gap, and smoothing it
/// would draw a gradient the strip is not showing.
fn paint_gradient(
    canvas: &web_sys::HtmlCanvasElement,
    context: &web_sys::CanvasRenderingContext2d,
    colors: &[[u8; 3]],
) -> Result<(), String> {
    let texels = colors.len().max(1) as u32;
    if canvas.width() != texels || canvas.height() != 1 {
        canvas.set_width(texels);
        canvas.set_height(1);
    }
    let mut rgba = Vec::with_capacity(texels as usize * 4);
    for color in colors {
        rgba.extend_from_slice(color);
        rgba.push(255);
    }
    if colors.is_empty() {
        rgba.extend_from_slice(&[0, 0, 0, 0]);
    }
    let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&rgba), texels, 1)
        .map_err(|error| format!("build ImageData: {error:?}"))?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|error| format!("putImageData: {error:?}"))
}

/// Discrete rounded blocks, one per lamp, with track showing between them.
///
/// Painted at DEVICE pixels so the corners are crisp on a retina panel — the
/// canvas is 1:1 with its CSS box, which is also why the class's
/// `image-rendering: pixelated` costs nothing here.
fn paint_bulbs(
    canvas: &web_sys::HtmlCanvasElement,
    context: &web_sys::CanvasRenderingContext2d,
    colors: &[[u8; 3]],
    css_width: f64,
    css_height: f64,
) -> Result<(), String> {
    let dpr = web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .filter(|dpr| *dpr > 0.0)
        .unwrap_or(1.0);
    let width = ((css_width * dpr).round() as u32).max(1);
    let height = ((css_height * dpr).round() as u32).max(1);
    if canvas.width() != width || canvas.height() != height {
        canvas.set_width(width);
        canvas.set_height(height);
    }
    context.clear_rect(0.0, 0.0, f64::from(width), f64::from(height));

    let lamps = colors.len().max(1) as f64;
    let pitch = f64::from(width) / lamps;
    let gap = (BULB_GAP_PX * dpr).min(pitch * 0.4);
    let bulb_width = (pitch - gap).max(1.0);
    let bulb_height = f64::from(height);
    let radius = BULB_RADIUS_PX
        .min(bulb_width / 2.0)
        .min(bulb_height / 2.0)
        .max(0.0);
    for (index, color) in colors.iter().enumerate() {
        let left = index as f64 * pitch + gap / 2.0;
        context.set_fill_style_str(&format!("rgb({} {} {})", color[0], color[1], color[2]));
        rounded_rect(context, left, 0.0, bulb_width, bulb_height, radius)?;
        context.fill();
    }
    Ok(())
}

/// A rounded rectangle path, built from arcs (`roundRect` is newer than the
/// browsers this ships to).
fn rounded_rect(
    context: &web_sys::CanvasRenderingContext2d,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) -> Result<(), String> {
    let right = x + width;
    let bottom = y + height;
    context.begin_path();
    context.move_to(x + radius, y);
    let arc = |x1: f64, y1: f64, x2: f64, y2: f64| -> Result<(), String> {
        context
            .arc_to(x1, y1, x2, y2, radius)
            .map_err(|error| format!("arcTo: {error:?}"))
    };
    arc(right, y, right, bottom)?;
    arc(right, bottom, x, bottom)?;
    arc(x, bottom, x, y)?;
    arc(x, y, right, y)?;
    context.close_path();
    Ok(())
}

// -- the box -------------------------------------------------------------------

fn install_strip_resize_observer(
    observer: &Rc<RefCell<Option<StripResizeObserver>>>,
    canvas_id: &str,
    mut size_epoch: Signal<u64>,
) {
    if observer.borrow().is_some() {
        return;
    }
    let installed = StripResizeObserver::install(canvas_id, move || {
        let next = *size_epoch.peek() + 1;
        size_epoch.set(next);
    });
    *observer.borrow_mut() = installed;
}

/// Watches the strip's box: the bulbs/gradient choice is a MEASUREMENT, and
/// a window resize moves no prop — nothing else would report that the strip
/// crossed the threshold. Disconnects on drop.
struct StripResizeObserver {
    observer: web_sys::ResizeObserver,
    _callback: Closure<dyn FnMut(web_sys::js_sys::Array)>,
}

impl StripResizeObserver {
    fn install(canvas_id: &str, mut on_resize: impl FnMut() + 'static) -> Option<Self> {
        let element = web_sys::window()?
            .document()?
            .get_element_by_id(canvas_id)?;
        // Compared in DEVICE pixels — the same quantity the bulbs paint
        // sizes its backing store from, so the observer fires exactly when a
        // repaint would produce a different bitmap and never on a sub-pixel
        // wobble that would not. Seeded from the box the caller just painted
        // against, so the initial delivery is not a spurious repaint.
        let device_box = |element: &web_sys::Element| {
            let dpr = web_sys::window()
                .map(|window| window.device_pixel_ratio())
                .filter(|dpr| *dpr > 0.0)
                .unwrap_or(1.0);
            let rect = element.get_bounding_client_rect();
            (
                (rect.width() * dpr).round() as i64,
                (rect.height() * dpr).round() as i64,
            )
        };
        let last = Cell::new(device_box(&element));
        let observed = element.clone();
        let callback = Closure::<dyn FnMut(web_sys::js_sys::Array)>::new(
            move |_entries: web_sys::js_sys::Array| {
                let now = device_box(&observed);
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

impl Drop for StripResizeObserver {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only the layout-sized presentation claims `ux-box-sized-canvas`: the
    /// story gate asserts that class's bitmap matches its box, and a gradient
    /// strip's bitmap is deliberately one texel per lamp instead.
    #[test]
    fn only_bulbs_claim_the_box_sized_gate() {
        assert!(strip_class(StripPresentation::Bulbs).contains("ux-box-sized-canvas"));
        assert!(!strip_class(StripPresentation::Gradient).contains("ux-box-sized-canvas"));
        for shown in [StripPresentation::Bulbs, StripPresentation::Gradient] {
            assert!(
                strip_class(shown).contains("ux-produced-product-pixel-canvas"),
                "every strip stays under the painted-marker ready-wait"
            );
        }
    }

    /// The repaint key is the picture: identical colours never repaint, and
    /// a single changed lamp always does. (The strip has no clock of its
    /// own — a live surface repaints because its CALLER handed it new
    /// colours, which is the only reason it ever should.)
    #[test]
    fn the_digest_is_the_picture() {
        let a: Vec<[u8; 3]> = (0..60).map(|lamp| [lamp as u8, 40, 200]).collect();
        assert_eq!(color_digest(&a), color_digest(&a.clone()));
        let mut b = a.clone();
        b[30][1] = b[30][1].wrapping_add(1);
        assert_ne!(color_digest(&a), color_digest(&b));
        let mut shorter = a.clone();
        shorter.pop();
        assert_ne!(color_digest(&a), color_digest(&shorter));
    }
}
