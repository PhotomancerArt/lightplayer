//! The clock face's trace animation: one rAF driver per face painting every
//! phasor trace card (clock-face v2).
//!
//! Each card is a little black-and-white oscilloscope of ONE downstream
//! reading — the SHAPED value that consumer actually reads, drawn over a
//! trailing window with the newest sample at the right edge. The trace is an
//! analytic function of effective time (the engine's scrub-exact contract),
//! so every frame redraws the whole window from closed form; nothing is
//! accumulated, and a probe correction simply redraws the identical curve
//! from the corrected phase.
//!
//! **One driver, many cards**: a face's cards all animate from a single
//! `requestAnimationFrame` loop (the crate's rAF idiom — see
//! `base/popover.rs`) rather than one loop per card. Between probes the
//! phase extrapolates locally — `φ_now = φ_probe + elapsed/T` — and each
//! landing probe re-anchors the extrapolation; the correction may visibly
//! snap, which is accepted v1 (G3 note). Frozen readings hold still.
//!
//! Painting is imperative onto `<canvas>` elements addressed by id (the
//! `ProductPreviewCanvas` pattern in `produced_product_view.rs`): the
//! elements are stable across vdom diffs, and nothing here ever renders
//! through Dioxus. The first paint of a card is time-independent (elapsed
//! exactly zero from the fixture's own phase), which is what makes a story
//! capture of frame zero deterministic; the `data-preview-painted` marker
//! rides the same contract as the preview canvases.
//!
//! Dropping the driver cancels the scheduled frame — a face that unmounts
//! leaks neither closure nor animation (the loop also idles for free while
//! the tab is hidden, which is rAF's own behavior).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use lpa_studio_core::{UiPhasorReading, Waveform};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Trailing window a card draws, in effective seconds: a few cycles of its
/// own period, floored so a fast phasor still shows texture and capped so
/// plasma's 100 s crawl fits (`clamp(2.5·T, 4 s, 300 s)`); a frozen reading
/// gets a fixed window for its flat line. From the spike's `windowFor`.
fn trace_window_seconds(period_seconds: f32) -> f32 {
    if period_seconds > 0.0 {
        (2.5 * period_seconds).clamp(4.0, 300.0)
    } else {
        8.0
    }
}

/// Vertical padding inside the canvas, in device pixels at dpr 1 (scaled).
const TRACE_PAD_PX: f64 = 3.0;

/// One card the driver is animating.
struct TraceCard {
    canvas_id: String,
    /// The DTO the card was built from — the change fingerprint. A render
    /// that carries an identical reading keeps the card's anchor; any field
    /// moving re-anchors the extrapolation at the new probe value.
    reading: UiPhasorReading,
    /// Milliseconds timestamp (`performance.now()`) the reading landed —
    /// the extrapolation anchor.
    anchored_at_ms: f64,
}

struct DriverInner {
    window: web_sys::Window,
    performance: web_sys::Performance,
    cards: RefCell<Vec<TraceCard>>,
    raf_id: Cell<Option<i32>>,
    tick: RefCell<Option<web_sys::js_sys::Function>>,
    /// Lazily-resolved "animations are frozen here" flag: on the story page
    /// (marked by its `data-story-capture` box) the loop never runs and
    /// every paint is the deterministic elapsed-zero frame, so consecutive
    /// capture shots are byte-identical — the harness freezes CSS
    /// animations but cannot freeze rAF, and an animating baseline would
    /// churn on every CI capture.
    frozen: Cell<Option<bool>>,
}

/// The face-level animation driver. `None` inside when there is no browser
/// window (host-side component tests render the markup; nothing animates).
pub(crate) struct PhasorTraceDriver {
    inner: Option<Rc<DriverInner>>,
    _closure: Option<Closure<dyn FnMut(f64)>>,
}

/// Identity comparison for Dioxus prop memoization: a driver is equal only
/// to itself (one instance per face; the cards all share it).
impl PartialEq for PhasorTraceDriver {
    fn eq(&self, other: &Self) -> bool {
        match (&self.inner, &other.inner) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl PhasorTraceDriver {
    pub(crate) fn new() -> Self {
        let Some(window) = web_sys::window() else {
            return Self {
                inner: None,
                _closure: None,
            };
        };
        let Some(performance) = window.performance() else {
            return Self {
                inner: None,
                _closure: None,
            };
        };
        let inner = Rc::new(DriverInner {
            window,
            performance,
            cards: RefCell::new(Vec::new()),
            raf_id: Cell::new(None),
            tick: RefCell::new(None),
            frozen: Cell::new(None),
        });
        let for_frames = inner.clone();
        let closure = Closure::wrap(Box::new(move |now: f64| {
            for_frames.raf_id.set(None);
            for card in for_frames.cards.borrow().iter() {
                if let Some(frozen) = paint_card(card, now) {
                    for_frames.frozen.set(Some(frozen));
                }
            }
            for_frames.schedule();
        }) as Box<dyn FnMut(f64)>);
        *inner.tick.borrow_mut() = Some(
            closure
                .as_ref()
                .unchecked_ref::<web_sys::js_sys::Function>()
                .clone(),
        );
        Self {
            inner: Some(inner),
            _closure: Some(closure),
        }
    }

    /// Reconcile the driver against this render's readings, one canvas id
    /// per card. Unchanged readings keep their extrapolation anchor; new or
    /// corrected ones re-anchor at now and repaint immediately (elapsed
    /// exactly zero — the deterministic first frame).
    pub(crate) fn sync(&self, readings: &[UiPhasorReading], canvas_id: impl Fn(usize) -> String) {
        let Some(inner) = &self.inner else {
            return;
        };
        let now = inner.performance.now();
        let mut cards = inner.cards.borrow_mut();
        cards.truncate(readings.len());
        for (index, reading) in readings.iter().enumerate() {
            let id = canvas_id(index);
            match cards.get_mut(index) {
                Some(card) if card.canvas_id == id && card.reading == *reading => {}
                Some(card) => {
                    *card = TraceCard {
                        canvas_id: id,
                        reading: reading.clone(),
                        anchored_at_ms: now,
                    };
                    paint_card(card, now);
                }
                None => {
                    let card = TraceCard {
                        canvas_id: id,
                        reading: reading.clone(),
                        anchored_at_ms: now,
                    };
                    paint_card(&card, now);
                    cards.push(card);
                }
            }
        }
        let live = !cards.is_empty();
        drop(cards);
        if live {
            inner.schedule();
        }
    }

    /// A card's canvas just mounted: paint its first frame (the sync-time
    /// paint may have run before the element existed). On a frozen page the
    /// paint pins itself to the anchor (elapsed exactly zero) so a capture
    /// of the mounted state is byte-stable run to run.
    pub(crate) fn canvas_mounted(&self, index: usize) {
        let Some(inner) = &self.inner else {
            return;
        };
        let now = inner.performance.now();
        if let Some(card) = inner.cards.borrow().get(index)
            && let Some(frozen) = paint_card(card, now)
        {
            inner.frozen.set(Some(frozen));
        }
    }
}

impl Drop for PhasorTraceDriver {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner
            && let Some(id) = inner.raf_id.take()
        {
            let _ = inner.window.cancel_animation_frame(id);
        }
    }
}

impl DriverInner {
    /// Schedule the next frame while any card is live; quietly stops the
    /// loop when the card list empties or a paint has discovered it lives
    /// inside the story page's capture box — the cards then hold their
    /// deterministic elapsed-zero frame. The flag comes from the paints
    /// themselves (`closest()` from the mounted canvas): a document-level
    /// query at driver creation ran before the story shell was in the DOM
    /// and cached the wrong answer.
    fn schedule(&self) {
        if self.raf_id.get().is_some()
            || self.cards.borrow().is_empty()
            || self.frozen.get() == Some(true)
        {
            return;
        }
        let scheduled = self.tick.borrow().as_ref().and_then(|tick| {
            self.window
                .request_animation_frame(tick.unchecked_ref())
                .ok()
        });
        self.raf_id.set(scheduled);
    }
}

/// Draw one card's trailing window as of `now_ms`, and report whether the
/// canvas lives inside the story page's capture box (`Some(frozen)`), where
/// every paint pins to the anchor — elapsed exactly zero — so captures are
/// byte-stable run to run.
///
/// Quietly does nothing (`None`) when the canvas is not in the DOM yet —
/// the mount hook and the next frame both retry.
fn paint_card(card: &TraceCard, now_ms: f64) -> Option<bool> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(&card.canvas_id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())?;
    let frozen = canvas
        .closest("[data-story-capture=\"1\"]")
        .ok()
        .flatten()
        .is_some();
    let now_ms = if frozen { card.anchored_at_ms } else { now_ms };
    let Ok(Some(context)) = canvas.get_context("2d") else {
        return None;
    };
    let Ok(context) = context.dyn_into::<web_sys::CanvasRenderingContext2d>() else {
        return None;
    };

    // Match the backing store to the CSS box (the spike's `px()` helper):
    // the canvas is styled w-full h-[42px], so the pixel size follows
    // layout × devicePixelRatio.
    let rect = canvas.get_bounding_client_rect();
    let dpr = web_sys::window().map_or(1.0, |window| window.device_pixel_ratio());
    let width = (rect.width() * dpr).round().max(1.0) as u32;
    let height = (rect.height() * dpr).round().max(1.0) as u32;
    if canvas.width() != width || canvas.height() != height {
        canvas.set_width(width);
        canvas.set_height(height);
    }
    let (w, h) = (f64::from(width), f64::from(height));

    // B/W by decree: the trace draws in the card's own text color (the
    // canvas inherits it from the id line's tone via CSS `color`), never a
    // status color — shared is carried by the violet BORDER, not the trace.
    let style = web_sys::window()
        .and_then(|window| window.get_computed_style(&canvas).ok().flatten())
        .and_then(|style| style.get_property_value("color").ok())
        .filter(|color| !color.is_empty())
        .unwrap_or_else(|| String::from("#e6e4de"));

    let reading = &card.reading;
    let period = f64::from(reading.period_seconds);
    // A frozen READING (period 0) holds still — distinct from the frozen
    // PAGE above, which pins the paint time.
    let held = !(period.is_finite() && period > 0.0);
    let elapsed = ((now_ms - card.anchored_at_ms) / 1000.0).max(0.0);
    // The phase now, extrapolated from the probe anchor; held readings stay.
    let phase_now = if held {
        f64::from(reading.phase)
    } else {
        f64::from(reading.phase) + elapsed / period
    };
    // How long this phasor has existed, in effective seconds — columns
    // older than its materialization draw nothing (the spike's `tx < 0`).
    let age = if held {
        f64::INFINITY
    } else {
        (f64::from(reading.cycle) + phase_now) * period
    };

    context.clear_rect(0.0, 0.0, w, h);
    // Faint midline so an empty stretch still reads as a scope.
    context.set_stroke_style_str(&style);
    context.set_global_alpha(0.14);
    context.set_line_width(1.0);
    context.begin_path();
    context.move_to(0.0, h / 2.0);
    context.line_to(w, h / 2.0);
    context.stroke();

    context.set_global_alpha(1.0);
    context.set_line_width((w / 200.0).max(1.25));
    context.begin_path();
    let window_s = f64::from(trace_window_seconds(reading.period_seconds));
    let pad = TRACE_PAD_PX * dpr;
    let mut started = false;
    for x in 0..width {
        let back = window_s * (1.0 - f64::from(x) / w);
        if back > age {
            continue;
        }
        let phase = if held {
            phase_now
        } else {
            phase_now - back / period
        };
        let value = f64::from(shape_phasor(
            reading.waveform,
            phase as f32 + reading.phase_offset,
        ));
        let y = pad + (1.0 - value) * (h - 2.0 * pad);
        if started {
            context.line_to(f64::from(x), y);
        } else {
            context.move_to(f64::from(x), y);
            started = true;
        }
    }
    context.stroke();

    // Same ready-marker contract as the preview canvases: the story
    // capture harness can wait for a painted first frame.
    let _ = canvas.set_attribute("data-preview-painted", "1");
    Some(frozen)
}

/// The consumer's shaping, mirroring the engine's `shape_phasor`
/// (`nodes/shader/phasor_eval.rs`) — the card draws what the uniform reads.
fn shape_phasor(waveform: Waveform, phase: f32) -> f32 {
    let x = wrap_unit(phase);
    match waveform {
        Waveform::Ramp => x,
        Waveform::Sine => 0.5 + 0.5 * (core::f32::consts::TAU * x).sin(),
        Waveform::Triangle => {
            if x < 0.5 {
                2.0 * x
            } else {
                2.0 - 2.0 * x
            }
        }
        Waveform::Square => {
            if x < 0.5 {
                0.0
            } else {
                1.0
            }
        }
    }
}

/// Fold `value` into `[0,1)` — the engine's manual wrap, kept identical so
/// the drawn wave and the uniform agree at the seams.
fn wrap_unit(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let mut frac = value - (value as i64) as f32;
    if frac < 0.0 {
        frac += 1.0;
    }
    if frac >= 1.0 {
        frac = 0.0;
    }
    frac
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shaping here must be the engine's, seam for seam — a card that
    /// drew its own idea of a square wave would lie about the uniform.
    #[test]
    fn shaping_mirrors_the_engine() {
        assert_eq!(shape_phasor(Waveform::Ramp, 0.25), 0.25);
        assert_eq!(shape_phasor(Waveform::Ramp, 1.25), 0.25);
        assert_eq!(shape_phasor(Waveform::Ramp, -0.25), 0.75);
        assert_eq!(shape_phasor(Waveform::Square, 0.49), 0.0);
        assert_eq!(shape_phasor(Waveform::Square, 0.5), 1.0);
        assert_eq!(shape_phasor(Waveform::Triangle, 0.25), 0.5);
        assert_eq!(shape_phasor(Waveform::Triangle, 0.75), 0.5);
        let sine = shape_phasor(Waveform::Sine, 0.25);
        assert!((sine - 1.0).abs() < 1e-6, "sine peaks at quarter: {sine}");
        assert_eq!(shape_phasor(Waveform::Ramp, f32::NAN), 0.0);
    }

    /// The window is a few cycles, floored and capped; frozen gets a fixed
    /// flat-line window (spike `windowFor`).
    #[test]
    fn trace_window_scales_with_the_period() {
        assert_eq!(trace_window_seconds(1.0), 4.0);
        assert_eq!(trace_window_seconds(4.0), 10.0);
        assert_eq!(trace_window_seconds(100.0), 250.0);
        assert_eq!(trace_window_seconds(1000.0), 300.0);
        assert_eq!(trace_window_seconds(0.0), 8.0);
        assert_eq!(trace_window_seconds(-1.0), 8.0);
    }
}
