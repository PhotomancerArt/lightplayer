//! The tape transport's animation: one rAF driver per clock face painting
//! the scrolling tape strip and the calm digits (plan
//! 2026-08-04-2355-clock-tape-hero, P3).
//!
//! The strip is a ruler of effective time streaming under a fixed centered
//! playhead at a FIXED scale (14 css px per tape second): a faster rate
//! streams the strip visibly faster, which is the point — "I really
//! expect the speed slider to make that tape move faster / slower" (Q5
//! reversed at the live build; the spike's speed-linked zoom is banked
//! for the input-recorder reel). An adaptive tick ladder picked map-style
//! (constant at this scale), the clock-birth edge at t = 0 with a faint
//! pre-birth wash, and h:mm:ss / m:ss digits that stay whole-second calm
//! at rest.
//!
//! Between probes the effective time extrapolates locally —
//! `t = anchor.seconds + elapsed × rate` while running, frozen paused —
//! and every transport-block change re-anchors (the `TraceCard.reading`
//! fingerprint pattern; the DTO derives `PartialEq` for exactly this).
//!
//! Everything else follows [`super::phasor_trace`]'s contract to the
//! letter: painting is imperative onto elements addressed by id (never
//! through the vdom — the digits update the same way, a `textContent`
//! write, because re-rendering a component at 60 fps to move a number is
//! the exact churn the canvas idiom exists to avoid); the first paint of
//! an anchor is time-independent (elapsed exactly zero) so story captures
//! are byte-identical; the lazily-resolved `data-story-capture` flag stops
//! the loop on the story page; the `data-preview-painted` marker rides the
//! same ready-wait contract as the preview canvases; and dropping the
//! driver cancels the scheduled frame.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use lpa_studio_core::UiClockTransport;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

use super::tape_transport::{format_clock, tape_px_per_sec, tape_tick_pair};

/// The last transport block that landed, and when — the extrapolation
/// anchor (the DTO is the change fingerprint).
struct TapeAnchor {
    transport: UiClockTransport,
    /// Milliseconds timestamp (`performance.now()`) the block landed.
    anchored_at_ms: f64,
}

struct DriverInner {
    window: web_sys::Window,
    performance: web_sys::Performance,
    canvas_id: String,
    digits_id: String,
    offlive_id: String,
    anchor: RefCell<Option<TapeAnchor>>,
    raf_id: Cell<Option<i32>>,
    tick: RefCell<Option<web_sys::js_sys::Function>>,
    /// Lazily-resolved story-page flag — see `phasor_trace::DriverInner`.
    frozen: Cell<Option<bool>>,
    /// Mid-drag flag: digits grow a tenths place while the tape is under
    /// the finger.
    dragging: Cell<bool>,
    /// The scrub value under the finger, overriding the DTO's staged one
    /// (drag-local echo, the KnobField preview pattern): the paint shows
    /// `t + (preview − staged)` so the strip follows the pointer exactly
    /// while the throttled write stream catches up through the DTO.
    scrub_preview: Cell<Option<f32>>,
    /// Last digits string written, so the `textContent` write only happens
    /// when the display actually changes (once a second at rest).
    last_digits: RefCell<String>,
    /// Last off-live line written (the digits cluster's amber sub-line —
    /// the separate chip "jumped around" and died at the G1 gate).
    last_offlive: RefCell<String>,
    /// Watches the tape canvas for box changes and repaints it at the new
    /// size. Load-bearing on the frozen story page, where the rAF loop stops
    /// after the first frame: the stylesheet is injected by the wasm bundle
    /// after boot, so a paint that beats it measures a box the canvas does
    /// not end up with, and without the observer that stale bitmap is a
    /// second stable terminal — the exact oscillation of
    /// 2026-08-05-clock-face-baselines-oscillate, reached through this
    /// canvas instead of the trace cards.
    resize: RefCell<Option<web_sys::ResizeObserver>>,
}

/// The clock face's tape driver. `None` inside when there is no browser
/// window (host-side component tests render the markup; nothing animates).
pub(crate) struct TapeTransportDriver {
    inner: Option<Rc<DriverInner>>,
    _closure: Option<Closure<dyn FnMut(f64)>>,
    /// The `ResizeObserver`'s callback. Held here rather than in the shared
    /// inner so the driver's `Rc` graph stays acyclic (the rAF closure's
    /// pattern): dropping the driver frees both.
    _resize_closure: Option<Closure<dyn FnMut(web_sys::js_sys::Array)>>,
    canvas_id: String,
    digits_id: String,
    offlive_id: String,
}

impl TapeTransportDriver {
    pub(crate) fn new(canvas_id: String, digits_id: String, offlive_id: String) -> Self {
        let Some(window) = web_sys::window() else {
            return Self {
                inner: None,
                _closure: None,
                _resize_closure: None,
                canvas_id,
                digits_id,
                offlive_id,
            };
        };
        let Some(performance) = window.performance() else {
            return Self {
                inner: None,
                _closure: None,
                _resize_closure: None,
                canvas_id,
                digits_id,
                offlive_id,
            };
        };
        let inner = Rc::new(DriverInner {
            window,
            performance,
            canvas_id: canvas_id.clone(),
            digits_id: digits_id.clone(),
            offlive_id: offlive_id.clone(),
            anchor: RefCell::new(None),
            raf_id: Cell::new(None),
            tick: RefCell::new(None),
            frozen: Cell::new(None),
            dragging: Cell::new(false),
            scrub_preview: Cell::new(None),
            last_digits: RefCell::new(String::new()),
            last_offlive: RefCell::new(String::new()),
            resize: RefCell::new(None),
        });
        let for_frames = inner.clone();
        let closure = Closure::wrap(Box::new(move |now: f64| {
            for_frames.raf_id.set(None);
            for_frames.paint(now);
            for_frames.schedule();
        }) as Box<dyn FnMut(f64)>);
        *inner.tick.borrow_mut() = Some(
            closure
                .as_ref()
                .unchecked_ref::<web_sys::js_sys::Function>()
                .clone(),
        );
        // The canvas's box changed (the stylesheet landed, the pane
        // resized): repaint so the backing store matches the box again.
        // Painting is idempotent — time is pinned to the anchor on a frozen
        // page and re-derived from it otherwise — so an extra pass only ever
        // corrects geometry (the trace driver's pattern, same defect).
        let for_resize = inner.clone();
        let resize_closure = Closure::<dyn FnMut(web_sys::js_sys::Array)>::new(
            move |_entries: web_sys::js_sys::Array| {
                let now = for_resize.performance.now();
                for_resize.paint(now);
            },
        );
        *inner.resize.borrow_mut() =
            web_sys::ResizeObserver::new(resize_closure.as_ref().unchecked_ref()).ok();
        Self {
            inner: Some(inner),
            _closure: Some(closure),
            _resize_closure: Some(resize_closure),
            canvas_id,
            digits_id,
            offlive_id,
        }
    }

    pub(crate) fn canvas_id(&self) -> &str {
        &self.canvas_id
    }

    pub(crate) fn digits_id(&self) -> &str {
        &self.digits_id
    }

    pub(crate) fn offlive_id(&self) -> &str {
        &self.offlive_id
    }

    /// Reconcile the driver against this render's transport block. An
    /// unchanged block keeps its extrapolation anchor; a changed one
    /// re-anchors at now and repaints immediately (elapsed exactly zero —
    /// the deterministic first frame).
    ///
    /// **A live drag holds its anchor.** The scrub write stream is
    /// throttled, so mid-drag every echo arrives carrying a position the
    /// finger has already left. Re-anchoring on it did two things at once:
    /// it reset `anchored_at_ms`, dropping the running time accumulated
    /// since the drag began, and it moved `scrub_offset_seconds` out from
    /// under `preview_delta` — so the strip snapped backwards once per
    /// dispatch interval and then caught up on the next pointer move. While
    /// the finger is down the preview IS the value (`paint` adds the
    /// pointer's delta against this anchor), and the echo has nothing to
    /// contribute; the anchor re-settles on the first sync after release.
    pub(crate) fn sync(&self, transport: &UiClockTransport) {
        let Some(inner) = &self.inner else {
            return;
        };
        let now = inner.performance.now();
        if inner.dragging.get() {
            inner.schedule();
            return;
        }
        let changed = {
            let mut anchor = inner.anchor.borrow_mut();
            match anchor.as_ref() {
                Some(anchored) if anchored.transport == *transport => false,
                _ => {
                    *anchor = Some(TapeAnchor {
                        transport: transport.clone(),
                        anchored_at_ms: now,
                    });
                    true
                }
            }
        };
        if changed {
            inner.paint(now);
        }
        inner.schedule();
    }

    /// The tape canvas just mounted: paint its first frame (the sync-time
    /// paint may have run before the element existed) and put it under the
    /// resize observer. On a frozen page the paint pins itself to the anchor
    /// so a capture of the mounted state is byte-stable run to run; the
    /// observer is what keeps that stable state the STYLED one when this
    /// paint beat the stylesheet.
    pub(crate) fn canvas_mounted(&self) {
        let Some(inner) = &self.inner else {
            return;
        };
        inner.paint(inner.performance.now());
        if let Some(observer) = inner.resize.borrow().as_ref()
            && let Some(canvas) = inner
                .window
                .document()
                .and_then(|document| document.get_element_by_id(&inner.canvas_id))
        {
            observer.observe(&canvas);
        }
    }

    /// A scrub drag is live (or just ended): tenths on the digits, and the
    /// strip renders the preview value rather than waiting for the write
    /// stream's echo. Repaints immediately so the strip is under the
    /// finger this frame, not the next one.
    pub(crate) fn set_scrub_drag(&self, preview: Option<f32>) {
        let Some(inner) = &self.inner else {
            return;
        };
        // Releasing: fold the finger's last position INTO the held anchor
        // before dropping the preview. The anchor still describes where the
        // drag started, so clearing the preview alone would paint one frame
        // back at the pre-drag position and then jump forward when the DTO
        // lands — the release-end twin of the jank this anchor-hold fixes.
        // `seconds` tracks `scrub_offset_seconds` one-for-one (that is what
        // makes `preview_delta` a pure correction), so moving both by the
        // same delta leaves `t` continuous across the hand-off.
        if preview.is_none()
            && let Some(settled) = inner.scrub_preview.get()
            && let Some(anchor) = inner.anchor.borrow_mut().as_mut()
        {
            let delta = settled - anchor.transport.scrub_offset_seconds;
            anchor.transport.scrub_offset_seconds = settled;
            anchor.transport.seconds += delta;
        }
        inner.dragging.set(preview.is_some());
        inner.scrub_preview.set(preview);
        inner.paint(inner.performance.now());
    }

    /// The scrub value under the finger, when a drag is live.
    pub(crate) fn scrub_preview(&self) -> Option<f32> {
        self.inner
            .as_ref()
            .and_then(|inner| inner.scrub_preview.get())
    }
}

impl Drop for TapeTransportDriver {
    fn drop(&mut self) {
        let Some(inner) = &self.inner else {
            return;
        };
        if let Some(id) = inner.raf_id.take() {
            let _ = inner.window.cancel_animation_frame(id);
        }
        if let Some(observer) = inner.resize.borrow_mut().take() {
            observer.disconnect();
        }
    }
}

/// Ruler geometry, in css px at dpr 1 (scaled at paint): the tick band
/// hugs the bottom edge, labels sit just above it, the playhead needle
/// starts below its triangle. From the spike's `drawTape`.
const RULER_BAND_PX: f64 = 26.0;
const RULER_LABEL_PX: f64 = 32.0;

impl DriverInner {
    /// Schedule the next frame while an anchor is live; quietly stops the
    /// loop when a paint has discovered it lives inside the story page's
    /// capture box (the flag comes from the paints themselves — a
    /// document-level query at driver creation would cache the wrong
    /// answer, see `phasor_trace`).
    fn schedule(&self) {
        if self.raf_id.get().is_some()
            || self.anchor.borrow().is_none()
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

    /// Draw the tape strip and refresh the digits as of `now_ms`.
    ///
    /// Quietly does nothing when no anchor has landed or the canvas is not
    /// in the DOM yet — the mount hook and the next frame both retry.
    fn paint(&self, now_ms: f64) {
        let Some(anchored_at_ms) = self.anchor.borrow().as_ref().map(|a| a.anchored_at_ms) else {
            return;
        };
        let Some(canvas) = self
            .window
            .document()
            .and_then(|document| document.get_element_by_id(&self.canvas_id))
            .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            return;
        };
        let frozen = canvas
            .closest("[data-story-capture=\"1\"]")
            .ok()
            .flatten()
            .is_some();
        self.frozen.set(Some(frozen));
        let now_ms = if frozen { anchored_at_ms } else { now_ms };
        let Ok(Some(context)) = canvas.get_context("2d") else {
            return;
        };
        let Ok(context) = context.dyn_into::<web_sys::CanvasRenderingContext2d>() else {
            return;
        };

        // Match the backing store to the CSS box (styled w-full h-[62px]).
        let rect = canvas.get_bounding_client_rect();
        let dpr = self.window.device_pixel_ratio();
        let width = (rect.width() * dpr).round().max(1.0) as u32;
        let height = (rect.height() * dpr).round().max(1.0) as u32;
        if canvas.width() != width || canvas.height() != height {
            canvas.set_width(width);
            canvas.set_height(height);
        }
        let (w, h) = (f64::from(width), f64::from(height));

        // Ticks and labels draw in the canvas's own `color` at ruler
        // alphas; the playhead and birth edge take the accent token from
        // the same computed style (custom properties inherit).
        let style = self.window.get_computed_style(&canvas).ok().flatten();
        let base = style
            .as_ref()
            .and_then(|style| style.get_property_value("color").ok())
            .filter(|color| !color.is_empty())
            .unwrap_or_else(|| String::from("#e6e4de"));
        let accent = style
            .as_ref()
            .map(|style| style.get_property_value("--studio-color-accent"))
            .and_then(Result::ok)
            .map(|color| color.trim().to_string())
            .filter(|color| !color.is_empty())
            .unwrap_or_else(|| String::from("#7be0b2"));

        let anchor = self.anchor.borrow();
        let Some(anchor) = anchor.as_ref() else {
            return;
        };
        let transport = &anchor.transport;
        let elapsed = ((now_ms - anchor.anchored_at_ms) / 1000.0).max(0.0);
        // Drag-local echo: the preview's DELTA against the anchored staged
        // value rides on top of the extrapolation, so the strip follows
        // the pointer exactly and converges to zero correction as the
        // throttled writes echo back through the DTO.
        let preview_delta = self.scrub_preview.get().map_or(0.0, |preview| {
            f64::from(preview - transport.scrub_offset_seconds)
        });
        let t = f64::from(transport.seconds)
            + preview_delta
            + if transport.play_state.is_playing() {
                elapsed * f64::from(transport.rate)
            } else {
                0.0
            };
        let pps_css = tape_px_per_sec();
        let pps = pps_css * dpr;
        let cx = w / 2.0;

        context.clear_rect(0.0, 0.0, w, h);

        // Clock birth: faint wash before t = 0, accent edge line at it.
        let zero_x = cx - t * pps;
        if zero_x > 0.0 {
            context.set_fill_style_str(&base);
            context.set_global_alpha(0.025);
            context.fill_rect(0.0, 0.0, zero_x.min(w), h);
            context.set_global_alpha(0.5);
            context.set_stroke_style_str(&accent);
            context.set_line_width(dpr);
            context.begin_path();
            context.move_to(zero_x, 0.0);
            context.line_to(zero_x, h);
            context.stroke();
            context.set_global_alpha(1.0);
        }

        // Adaptive ruler: minors short, majors tall with a time label.
        // Ticks are indexed integers × the minor step — quantized, so a
        // long runtime never accumulates float drift across the strip.
        let (minor, major) = tape_tick_pair(pps_css);
        let major_every = (major / minor).round() as i64;
        let first = ((t - cx / pps) / minor).ceil().max(0.0) as i64;
        let last = ((t + cx / pps) / minor).floor() as i64;
        let y_top = h - RULER_BAND_PX * dpr;
        let y_mid = (y_top + h) / 2.0;
        let label_y = h - RULER_LABEL_PX * dpr;
        context.set_font(&format!("{}px ui-monospace, Menlo, monospace", 9.0 * dpr));
        context.set_text_align("center");
        context.set_line_width(dpr);
        for index in first..=last {
            let s = index as f64 * minor;
            let x = cx + (s - t) * pps;
            let is_major = index % major_every == 0;
            context.set_stroke_style_str(&base);
            context.set_global_alpha(if is_major { 0.32 } else { 0.12 });
            context.begin_path();
            context.move_to(x, if is_major { y_top } else { y_mid });
            context.line_to(x, h);
            context.stroke();
            if is_major {
                context.set_global_alpha(0.7);
                context.set_fill_style_str(&base);
                let _ = context.fill_text(&format_clock(s, false), x, label_y);
            }
        }
        context.set_global_alpha(1.0);

        // Fixed centered playhead: accent needle + triangle.
        context.set_stroke_style_str(&accent);
        context.set_line_width(1.5 * dpr);
        context.begin_path();
        context.move_to(cx, 8.0 * dpr);
        context.line_to(cx, h);
        context.stroke();
        context.set_fill_style_str(&accent);
        context.begin_path();
        context.move_to(cx - 4.0 * dpr, 2.0 * dpr);
        context.line_to(cx + 4.0 * dpr, 2.0 * dpr);
        context.line_to(cx, 9.0 * dpr);
        context.close_path();
        context.fill();

        // Calm digits, written imperatively like the canvas (never through
        // the vdom): whole seconds at rest, tenths only mid-drag (P4 sets
        // the flag). The write is skipped while the string is unchanged.
        let digits = format_clock(t, self.dragging.get());
        if *self.last_digits.borrow() != digits {
            if let Some(element) = self
                .window
                .document()
                .and_then(|document| document.get_element_by_id(&self.digits_id))
            {
                element.set_text_content(Some(&digits));
            }
            *self.last_digits.borrow_mut() = digits;
        }

        // The off-live readout rides UNDER the digits (its own reserved
        // line — the free-floating chip "jumped around" during a drag and
        // died at the G1 gate), imperative like them: the drag preview
        // updates it per move, and an on-live transport writes it empty.
        let scrub = self
            .scrub_preview
            .get()
            .unwrap_or(transport.scrub_offset_seconds);
        let offlive = if scrub.abs() > super::tape_transport::OFFLIVE_CHIP_EPSILON_S {
            let sign = if scrub < 0.0 { "\u{2212}" } else { "+" };
            format!("{sign}{:.1} s \u{00b7} live", scrub.abs())
        } else {
            String::new()
        };
        if *self.last_offlive.borrow() != offlive {
            if let Some(element) = self
                .window
                .document()
                .and_then(|document| document.get_element_by_id(&self.offlive_id))
            {
                element.set_text_content(Some(&offlive));
            }
            *self.last_offlive.borrow_mut() = offlive;
        }

        // Same ready-marker contract as the preview canvases: the story
        // capture harness can wait for a painted first frame.
        let _ = canvas.set_attribute("data-preview-painted", "1");
    }
}
