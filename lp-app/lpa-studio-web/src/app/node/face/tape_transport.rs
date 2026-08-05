//! The clock card's OUTPUT hero: the tape transport instrument (plan
//! 2026-08-04-2355-clock-tape-hero, P3; design record
//! `spikes/clock-transport-hero/index.html`, gate rounds 1–2).
//!
//! One shared surface renders the transport: the scrolling tape strip
//! under a fixed playhead (painted by [`super::tape_driver`]), the calm
//! digits, the run/pause button, the log ׼–×8 speed fader with octave
//! detents, and the amber off-live chip. This phase renders the chrome
//! statically — every gesture (drag-scrub, fader, run/pause, tap-to-
//! return) lands in P4 through the standard slot-edit path.
//!
//! Spike verdicts binding here: speed-linked zoom (constant pixel
//! velocity — ×8 shows 8× the time in the same pixels, "fast" is tick
//! density); octave detents with ×1 pulling hardest; a fixed-width
//! readout that must NEVER reflow the fader; whole-second digits at rest
//! with tenths only mid-drag; amber = off-live (`status-attention`
//! family, class toggle on the box, not canvas).

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use dioxus::prelude::*;
use lpa_studio_core::UiClockTransport;

use super::tape_driver::TapeTransportDriver;

/// Base tape velocity at ×1, in css px per effective second (Q5).
pub(crate) const TAPE_BASE_PX_PER_SEC: f64 = 14.0;

/// The octave detent stops of the speed fader (round-2 verdict; a
/// quantize-to-0.1 mode was tried and rejected — no landmark stops; fine
/// speed lives on the phasor knobs).
pub(crate) const RATE_DETENTS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];

/// The off-live chip appears past this offset magnitude, in seconds (the
/// amber box border tracks any non-zero offset; the chip waits for one
/// worth reading).
pub(crate) const OFFLIVE_CHIP_EPSILON_S: f32 = 0.05;

/// Monotonic per-face id base for the tape's imperatively-addressed
/// elements (canvas + digits), same idiom as the trace canvases.
static NEXT_TAPE_FACE_ID: AtomicU64 = AtomicU64::new(0);

/// Speed-linked zoom (Q5, ships as-is): constant pixel velocity, so the
/// px/s never changes and a faster clock packs more seconds into the same
/// pixels. Rejected-for-now variant (open note from the gate): also raise
/// actual velocity with `base / rate.sqrt()` — revisitable after field
/// feel, NOT wired to any toggle.
pub(crate) fn tape_px_per_sec(rate: f32) -> f64 {
    TAPE_BASE_PX_PER_SEC / f64::from(rate).max(1e-3)
}

/// Adaptive tick granularity, map-style: the first `[minor, major]` pair
/// where minors stay ≥ 8 css px apart AND major labels ≥ 44 css px apart.
const TAPE_TICK_LADDER: [(f64, f64); 7] = [
    (0.2, 1.0),
    (1.0, 5.0),
    (5.0, 30.0),
    (15.0, 60.0),
    (60.0, 300.0),
    (300.0, 1800.0),
    (900.0, 3600.0),
];

pub(crate) fn tape_tick_pair(pps_css: f64) -> (f64, f64) {
    for (minor, major) in TAPE_TICK_LADDER {
        if minor * pps_css >= 8.0 && major * pps_css >= 44.0 {
            return (minor, major);
        }
    }
    TAPE_TICK_LADDER[TAPE_TICK_LADDER.len() - 1]
}

/// Clock digits: `m:ss` (hours only when they exist: `h:mm:ss`), a real
/// minus sign ahead of negative time, tenths appended only mid-drag.
pub(crate) fn format_clock(seconds: f64, tenths: bool) -> String {
    let sign = if seconds < 0.0 { "\u{2212}" } else { "" };
    let t = seconds.abs();
    let hours = (t / 3600.0).floor() as i64;
    let minutes = ((t % 3600.0) / 60.0).floor() as i64;
    let secs = (t % 60.0).floor() as i64;
    let core = if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    };
    if tenths {
        format!("{sign}{core}.{}", ((t % 1.0) * 10.0).floor() as i64)
    } else {
        format!("{sign}{core}")
    }
}

/// Where a rate sits on the fader track, as a 0–1 fraction of the log
/// ׼–×8 span.
pub(crate) fn rate_frac(rate: f32) -> f32 {
    ((rate.max(1e-6).log2() + 2.0) / 5.0).clamp(0.0, 1.0)
}

/// The readout's number: integers bare (`×1`), fractional rates with the
/// leading zero dropped (`×.25`), one decimal above ×1 (`×1.5`) — the
/// spike's `fmtRate`, fixed-width friendly.
pub(crate) fn format_rate(rate: f32) -> String {
    if rate >= 1.0 {
        if rate.fract() == 0.0 {
            format!("{}", rate as i64)
        } else {
            format!("{rate:.1}")
        }
    } else {
        let rounded = (rate * 100.0).round() / 100.0;
        format!("{rounded}").trim_start_matches('0').to_string()
    }
}

/// Whether the rate is seated exactly on an octave detent (the readout
/// wears the accent when it is).
pub(crate) fn on_detent(rate: f32) -> bool {
    RATE_DETENTS.contains(&rate)
}

/// The tape transport instrument, one per clock face. Static chrome this
/// phase: the strip animates (and freezes deterministically on the story
/// page), the fader/run/chip render their real layout, and P4 wires every
/// gesture through the standard slot-edit path.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn TapeTransport(transport: UiClockTransport) -> Element {
    let driver = use_hook(|| {
        let id = NEXT_TAPE_FACE_ID.fetch_add(1, Ordering::Relaxed);
        Rc::new(TapeTransportDriver::new(
            format!("tape-transport-{id}"),
            format!("tape-transport-digits-{id}"),
        ))
    });
    driver.sync(&transport);

    let offlive = transport.scrub_offset_seconds != 0.0;
    let chip_visible = transport.scrub_offset_seconds.abs() > OFFLIVE_CHIP_EPSILON_S;
    let running = transport.running;

    // Amber = off-live, a class toggle on the box (never canvas): the
    // border is the ambient signal, the chip is the actionable one.
    let box_class = if offlive {
        "tw:overflow-hidden tw:rounded-md tw:border tw:border-status-attention-border tw:bg-terminal"
    } else {
        "tw:overflow-hidden tw:rounded-md tw:border tw:border-border-muted tw:bg-terminal"
    };
    // `button { font: inherit }` in the base sheet beats layered tw
    // utilities — the font is set explicitly here (wiring-UI lesson).
    let run_class = if running {
        "tw:inline-flex tw:h-7 tw:min-w-[34px] tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-[7px] tw:border tw:border-border-strong tw:bg-card-raised tw:px-2.5 tw:font-sans tw:text-xs tw:font-semibold tw:text-accent"
    } else {
        "tw:inline-flex tw:h-7 tw:min-w-[34px] tw:flex-none tw:cursor-pointer tw:items-center tw:justify-center tw:rounded-[7px] tw:border tw:border-border-strong tw:bg-card-raised tw:px-2.5 tw:font-sans tw:text-xs tw:font-semibold tw:text-muted-foreground tw:hover:text-strong-foreground"
    };
    let readout_value_class = if on_detent(transport.rate) {
        "tw:font-semibold tw:text-accent"
    } else {
        "tw:font-semibold tw:text-strong-foreground"
    };
    let thumb_style = format!(
        "left: calc((100% - 18px) * {} + 1px);",
        rate_frac(transport.rate)
    );
    let chip_sign = if transport.scrub_offset_seconds < 0.0 {
        "\u{2212}"
    } else {
        "+"
    };
    let chip_offset = transport.scrub_offset_seconds.abs();
    let initial_digits = format_clock(f64::from(transport.seconds), false);

    let mounted_driver = driver.clone();
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2",
            div { class: box_class,
                // Painted imperatively by the face's rAF driver — never
                // through the vdom (see `tape_driver`).
                canvas {
                    id: "{driver.canvas_id()}",
                    class: "tw:block tw:h-[62px] tw:w-full tw:text-strong-foreground",
                    onmounted: move |_| mounted_driver.canvas_mounted(),
                }
            }
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2.5",
                button {
                    r#type: "button",
                    class: run_class,
                    title: if running { "Pause the clock" } else { "Run the clock" },
                    span { class: "tw:text-xs tw:leading-none",
                        if running { "\u{275a}\u{275a}" } else { "\u{25b6}" }
                    }
                }
                // The driver rewrites this span's text every displayed
                // second (whole seconds at rest, tenths mid-drag) — the
                // initial render is the DTO's own anchor, deterministic.
                span {
                    id: "{driver.digits_id()}",
                    class: "tw:font-mono tw:text-lg tw:font-semibold tw:leading-none tw:tracking-[0.01em] tw:tabular-nums tw:text-strong-foreground",
                    "{initial_digits}"
                }
                span { class: "tw:ml-auto tw:inline-flex tw:flex-none tw:items-center tw:gap-2",
                    span { class: "tw:text-[9px] tw:uppercase tw:tracking-[0.1em] tw:text-dim-foreground",
                        "speed"
                    }
                    span {
                        class: "tw:relative tw:h-[22px] tw:w-[190px] tw:flex-none tw:cursor-ew-resize tw:touch-none tw:rounded-md tw:border tw:border-border-muted tw:bg-track",
                        title: "drag \u{00b7} double-click = \u{00d7}1",
                        // Detent ticks on the track, ×1 emphasized.
                        for detent in RATE_DETENTS {
                            span {
                                key: "{detent}",
                                class: if detent == 1.0 { "tw:absolute tw:bottom-0.5 tw:top-[12px] tw:w-px tw:bg-subtle-foreground" } else { "tw:absolute tw:bottom-0.5 tw:top-[15px] tw:w-px tw:bg-border-strong" },
                                style: format!(
                                    "left: calc({:.1}% + {:.1}px);",
                                    rate_frac(detent) * 100.0,
                                    (1.0 - rate_frac(detent)) * 16.0 - 8.0,
                                ),
                            }
                        }
                        span {
                            class: "tw:absolute tw:inset-y-0.5 tw:w-4 tw:rounded tw:border tw:border-border-strong tw:bg-card-raised tw:after:absolute tw:after:inset-y-[3px] tw:after:left-1/2 tw:after:w-px tw:after:bg-accent tw:after:content-['']",
                            style: thumb_style,
                        }
                    }
                    // Fixed width: a changing readout must NEVER reflow
                    // the fader (round-2 gate feedback).
                    span { class: "tw:w-9 tw:flex-none tw:text-left tw:font-mono tw:text-[11px] tw:tabular-nums tw:text-muted-foreground",
                        "\u{00d7}"
                        span { class: readout_value_class, {format_rate(transport.rate)} }
                    }
                }
                if chip_visible {
                    button {
                        r#type: "button",
                        class: "tw:inline-flex tw:flex-none tw:cursor-pointer tw:items-center tw:gap-1 tw:whitespace-nowrap tw:rounded-md tw:border tw:border-status-attention-border tw:bg-status-attention-bg tw:px-2 tw:py-1 tw:font-mono tw:text-[10px] tw:font-normal tw:text-status-attention-foreground",
                        title: "scrubbed off-live \u{2014} tap to return",
                        span { class: "tw:font-semibold", "{chip_sign}{chip_offset:.1} s" }
                        span { " \u{00b7} live" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Calm digits: m:ss under an hour, h:mm:ss above, a real minus sign,
    /// tenths only when asked (mid-drag).
    #[test]
    fn clock_digits_format_calm_and_signed() {
        assert_eq!(format_clock(0.0, false), "0:00");
        assert_eq!(format_clock(447.0, false), "7:27");
        assert_eq!(format_clock(59.999, false), "0:59");
        assert_eq!(format_clock(60.0, false), "1:00");
        assert_eq!(format_clock(3600.0, false), "1:00:00");
        assert_eq!(format_clock(3.0 * 3600.0 + 47.0 * 60.0, false), "3:47:00");
        assert_eq!(format_clock(-12.4, false), "\u{2212}0:12");
        assert_eq!(format_clock(447.35, true), "7:27.3");
        assert_eq!(format_clock(-12.46, true), "\u{2212}0:12.4");
    }

    /// Speed-linked zoom: constant pixel velocity, so px-per-second is
    /// inverse in the rate (×8 packs 8× the time into the same pixels).
    #[test]
    fn zoom_is_speed_linked() {
        assert_eq!(tape_px_per_sec(1.0), 14.0);
        assert_eq!(tape_px_per_sec(8.0), 1.75);
        assert_eq!(tape_px_per_sec(0.25), 56.0);
    }

    /// The ladder picks the first pair keeping minors ≥ 8 css px and major
    /// labels ≥ 44 css px — the spike's map-style granularity.
    #[test]
    fn tick_ladder_adapts_to_zoom() {
        assert_eq!(tape_tick_pair(tape_px_per_sec(1.0)), (1.0, 5.0));
        assert_eq!(tape_tick_pair(tape_px_per_sec(8.0)), (5.0, 30.0));
        assert_eq!(tape_tick_pair(tape_px_per_sec(0.25)), (0.2, 1.0));
        // Absurdly zoomed out: the coarsest pair is the floor.
        assert_eq!(tape_tick_pair(0.01), (900.0, 3600.0));
    }

    /// The fader is log ׼–×8: detents land at even fractions and the
    /// ends clamp.
    #[test]
    fn rate_fraction_is_log_over_the_span() {
        assert_eq!(rate_frac(0.25), 0.0);
        assert_eq!(rate_frac(1.0), 0.4);
        assert_eq!(rate_frac(8.0), 1.0);
        assert!((rate_frac(2.0) - 0.6).abs() < 1e-6);
        assert_eq!(rate_frac(16.0), 1.0);
    }

    /// Readout numbers stay fixed-width friendly: integers bare, sub-×1
    /// drops the leading zero, one decimal above ×1.
    #[test]
    fn rate_readout_matches_the_spike() {
        assert_eq!(format_rate(1.0), "1");
        assert_eq!(format_rate(8.0), "8");
        assert_eq!(format_rate(0.25), ".25");
        assert_eq!(format_rate(0.5), ".5");
        assert_eq!(format_rate(1.5), "1.5");
        assert!(on_detent(1.0));
        assert!(!on_detent(1.3));
    }
}
