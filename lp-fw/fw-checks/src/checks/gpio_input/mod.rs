//! The `gpio-input` payload: a pad read back, through the product's own
//! button path and through a GPIO interrupt.
//!
//! Every other payload in this crate makes the chip *say* something. This one
//! makes it *listen*, and it is the first that does: before M2 nothing in
//! `fw-checks` read an input at all, and the emulator's own pin fabric said so
//! in its module doc — "Not modelled: input (nothing drives a pad from
//! outside)".
//!
//! # The asymmetry, stated first
//!
//! There is no wire and no hands in this payload's captures, so **the two
//! sides are driven differently and the transcripts say so**:
//!
//! - **On silicon** the firmware drives its own pads and reads them back — a
//!   self-loop. `GPIO.in_` reads a pad's own driven level once the input
//!   buffer is on (`IO_MUX.fun_ie`), so no jumper is needed. What that
//!   measures is the **read path**: the input buffer, `in_`, the edge
//!   detector, the interrupt matrix, the debouncer.
//! - **On an emulated configuration** the same image runs and the pads are
//!   driven from **outside**, by a `--pin-script` whose text is
//!   [`write_pin_script`] below. What that measures is the
//!   **outside-driver path**: the signal fabric resolving a level onto a pad
//!   the guest is not driving, and the same read path above it.
//!
//! Both exercise the product's `Input`/debouncer path and both raise the GPIO
//! interrupt. Neither is a lie and conflating them would be, so which side
//! drove is on the `[gpio-input] drive=` line, in each transcript's sidecar
//! `note`, and in the host registry's description of this payload.
//!
//! The switch is **runtime**, not compile-time, so that the image bytes are
//! identical on both sides and a difference between the transcripts can never
//! be a difference between two builds. The firmware asks the pads: the
//! drive-select pad ([`DRIVE_SELECT_GPIO`]) is an input with a pull-down, and
//! a run in which something outside holds it **high** at arming time is a run
//! in which something outside is driving. On the desk board nothing is
//! connected to that pad, so it reads low and the firmware drives its own; the
//! emulator's script holds it high from cycle 0. The default — nobody drives —
//! is the self-loop, which is the safe direction: a script that failed to load
//! would produce a self-loop transcript that says `drive=self-loop`, not a
//! silent one that says nothing happened.
//!
//! # The two readers
//!
//! **The button goes through the product's own driver.** Not a
//! re-implementation of it: the harness constructs `Esp32GpioButtonDriver`
//! over the board's real `HwRegistry`, opens `button:local:D9`, and calls
//! `ButtonInput::poll`, which is `debouncer.sample(now_ms, input.is_low())`
//! and nothing else. The `test_button` diagnostic's wiring facts — "D9/GPIO20
//! with an internal pull-up and a normally-open button to GND", so **pressed
//! is low** — are this payload's now; that feature and that driver stay where
//! they are.
//!
//! **The encoder goes through the GPIO interrupt, and cannot pass without
//! it.** Two pads listen on both edges, and the quadrature decode
//! ([`Quadrature`]) happens in the handler. There is no polling fallback: a
//! machine whose `pin[n].int_ena` / `int_type` / `status` / `pcpu_int` path
//! does not work produces zero `reader":"encoder"` records, not slow ones.
//!
//! # Why the sample grid is the debouncer's clock
//!
//! `ButtonInput::poll` takes `now_ms` from its caller, and this payload hands
//! it **the sample index times [`POLL_MS`]** rather than a reading of the
//! chip's clock. That is deliberate. The debouncer's arithmetic is then a pure
//! function of the sample index and the levels observed, so the only
//! machine-dependent input to a `state` or a `samples` field is *the level the
//! pad carried* — which is the claim the payload exists to make. A wall clock
//! in that argument would have made both fields timing figures wearing a
//! structural field's name.
//!
//! The pace itself is still real: the loop sleeps to an absolute schedule, so
//! sample *k* happens at arming plus *k* × [`POLL_MS`] of guest time on either
//! machine, and the script's edges are placed at the **middle** of a sample
//! interval. That leaves ±10 ms between an edge and the sample that must see
//! it — the tolerance the emulated side needs, because its script is anchored
//! on the [`ARMED_MARKER`] line reaching the host rather than on the
//! instruction that printed it.
//!
//! # The script
//!
//! One table, [`SCRIPT`], drives both sides: the firmware walks it on silicon
//! and [`write_pin_script`] renders it as the `--pin-script` file the emulated
//! side is driven by. `lp-cli`'s registry-parity test holds the committed file
//! equal to what this renders, so the two sides cannot drift apart quietly.
//!
//! What it contains, and what each part is for:
//!
//! | ms after arming | what | what it proves |
//! |---|---|---|
//! | 110–114 | a five-edge bounce burst settling pressed | **one** state change, not five (the debouncer works) |
//! | 310 | release | the second state change |
//! | 410 / 610 | a clean press and release | the same pair with no bounce under it |
//! | 710–730 | a 20 ms glitch, below the 30 ms debounce | **no** state change at all |
//! | 900–935 | eight quadrature transitions, cw, at 200 Hz | two detents, positions 1 and 2 |
//! | 1000–1035 | eight transitions, ccw | two detents, positions 1 and 0 |
//!
//! The glitch is the negative half of the debounce claim and it is why
//! `samples` is on the record: a reader can see not only that four state
//! changes arrived but at which sample each one did, and a debouncer that had
//! silently stopped debouncing would report six.

use crate::emit_record_json;

/// The sentinel. The payload runs a fixed script and finishes.
pub const DONE_MARKER: &str = "[gpio-input] === DONE ===";

/// Printed the instant the payload's own clock starts, and the line the
/// emulated side's `--pin-script` anchors its whole timeline on.
///
/// It has to be a line the *host* can see, because a pin script watches the
/// device's consoles. Everything after it in [`SCRIPT`] is expressed as a
/// delay from the step before, so the one offset between the two timelines —
/// the time this line takes to reach the host — shifts every edge by the same
/// amount instead of accumulating.
pub const ARMED_MARKER: &str = "[gpio-input] === ARMED ===";

/// The button pad: GPIO20, which the XIAO ESP32-C6 silkscreen calls **D9**.
///
/// The silkscreen number is not the GPIO number on this board and that has
/// cost a sitting elsewhere, so both are written down here and in the
/// harness. GPIO20 is the pad `tests/test_button.rs` has always named, and it
/// is none of the four this plan forbids (GPIO9 the BOOT strap, GPIO12/13 the
/// USB pair, GPIO16/17 the UART0 tap, GPIO18 the strip).
pub const BUTTON_GPIO: u8 = 20;

/// Encoder channel A: GPIO0, the silkscreen's **D0**.
pub const ENCODER_A_GPIO: u8 = 0;

/// Encoder channel B: GPIO1, the silkscreen's **D1**.
///
/// GPIO19 (**D8**) is the other free header pad and is deliberately *not*
/// used: it is the RX side of the `--wire 18:19` loopback M2 P3 needs, and a
/// payload that claimed it would take that away.
pub const ENCODER_B_GPIO: u8 = 1;

/// The drive-select pad: GPIO2, the silkscreen's **D2**.
///
/// Read once, at arming, as an input with a pull-down. High means something
/// outside is driving this run's pads. See the module docs.
pub const DRIVE_SELECT_GPIO: u8 = 2;

/// The button sample period, in milliseconds of guest time.
///
/// Twice the `test_button` diagnostic's 5 ms, and chosen for the tolerance
/// rather than for fidelity: with edges placed at the middle of an interval,
/// a 20 ms grid absorbs up to 10 ms of offset between the two sides'
/// timelines. The product's debouncer settles in 30 ms
/// (`ButtonDebouncer::DEFAULT_STABLE_MS`), so two whole samples still sit
/// inside a settle window.
pub const POLL_MS: u32 = 20;

/// How many samples the button phase takes: 45 × 20 ms = 900 ms, which is the
/// last moment before the encoder phase's first edge.
pub const BUTTON_SAMPLES: u32 = 45;

/// The microsecond at which the run ends, measured from arming. Past the last
/// scripted edge by a comfortable margin, so that a detent decoded at
/// 1,035 ms is drained and printed before the sentinel.
pub const RUN_END_US: u64 = 1_120_000;

/// Which pad a scripted edge belongs to. Roles rather than numbers, because
/// the firmware and the script name the same pad two different ways.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pad {
    Button,
    EncoderA,
    EncoderB,
}

impl Pad {
    /// The GPIO number this role sits on.
    pub const fn gpio(self) -> u8 {
        match self {
            Pad::Button => BUTTON_GPIO,
            Pad::EncoderA => ENCODER_A_GPIO,
            Pad::EncoderB => ENCODER_B_GPIO,
        }
    }

    /// The level the pad rests at when nothing is happening.
    ///
    /// The button rests **high**: it is a normally-open switch to ground with
    /// a pull-up, so pressed is low. Both encoder channels rest **low**, which
    /// is the `(0, 0)` the Gray sequence starts from.
    ///
    /// The rest levels are driven explicitly on both sides, at cycle 0 in the
    /// script and before arming in the firmware. A pull-up's *value* is not
    /// modelled on the emulated side — an undriven pad reads low there — so a
    /// script that left the button pad alone would have the firmware see the
    /// button held down from boot.
    pub const fn rest_level(self) -> bool {
        match self {
            Pad::Button => true,
            Pad::EncoderA | Pad::EncoderB => false,
        }
    }
}

/// One scripted level change: when, which pad, what level.
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    /// Microseconds after arming.
    pub at_us: u32,
    pub pad: Pad,
    pub level: bool,
}

const fn e(at_us: u32, pad: Pad, level: bool) -> Edge {
    Edge { at_us, pad, level }
}

/// The whole run, as edges. See the table in the module docs.
///
/// **In time order, and it must stay that way**: the firmware walks it with an
/// absolute deadline per entry and the renderer emits each entry as a delay
/// from the one before, so an out-of-order row would be a negative delay.
/// `the_script_is_in_time_order` below is the check.
pub static SCRIPT: &[Edge] = &[
    // Press one, with contact bounce: five edges over 4 ms, alternating and
    // starting pressed, so the burst settles pressed. An odd count is the
    // rule the emulator's own `button` generator enforces, for the same
    // reason — a burst that settled released would be a button that bounced
    // itself open.
    e(110_000, Pad::Button, false),
    e(111_000, Pad::Button, true),
    e(112_000, Pad::Button, false),
    e(113_000, Pad::Button, true),
    e(114_000, Pad::Button, false),
    // Released, 196 ms later. Sample 16 (320 ms) is the first to see it.
    e(310_000, Pad::Button, true),
    // Press two: the same pair with no bounce under it, so the two presses
    // between them say that the debouncer neither invents an event nor drops
    // one.
    e(410_000, Pad::Button, false),
    e(610_000, Pad::Button, true),
    // A 20 ms glitch: below the 30 ms settle window, and seen by exactly one
    // sample (740 ms is already released again). No record at all comes out
    // of this pair, and that is the point of it.
    e(710_000, Pad::Button, false),
    e(730_000, Pad::Button, true),
    // Eight quadrature transitions clockwise at 200 Hz, from (0,0):
    // a rises, b rises, a falls, b falls. Two full detents.
    e(900_000, Pad::EncoderA, true),
    e(905_000, Pad::EncoderB, true),
    e(910_000, Pad::EncoderA, false),
    e(915_000, Pad::EncoderB, false),
    e(920_000, Pad::EncoderA, true),
    e(925_000, Pad::EncoderB, true),
    e(930_000, Pad::EncoderA, false),
    e(935_000, Pad::EncoderB, false),
    // Eight counter-clockwise: b rises, a rises, b falls, a falls. Back to
    // position 0, which is what makes the pair a round trip rather than two
    // unrelated claims.
    e(1_000_000, Pad::EncoderB, true),
    e(1_005_000, Pad::EncoderA, true),
    e(1_010_000, Pad::EncoderB, false),
    e(1_015_000, Pad::EncoderA, false),
    e(1_020_000, Pad::EncoderB, true),
    e(1_025_000, Pad::EncoderA, true),
    e(1_030_000, Pad::EncoderB, false),
    e(1_035_000, Pad::EncoderA, false),
];

/// Which side put the level on the pad, for the `drive=` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    /// The firmware drove its own pads and read them back (silicon).
    SelfLoop,
    /// Something outside drove them (an emulated configuration's
    /// `--pin-script`).
    External,
}

impl Drive {
    pub const fn as_str(self) -> &'static str {
        match self {
            Drive::SelfLoop => "self-loop",
            Drive::External => "external",
        }
    }
}

/// A completed detent: one quarter-turn's worth of transitions, decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Detent {
    /// The position after this detent, counting from 0 at arming.
    pub position: i32,
    /// True clockwise.
    pub cw: bool,
}

/// A 4×-decoded quadrature decoder, small enough to run inside an interrupt
/// handler.
///
/// It keeps the last `(a, b)` state and a signed sub-count. Each edge that
/// moves the state one place around the Gray cycle moves the sub-count by
/// one; four consecutive steps in one direction are a detent, and the
/// sub-count resets. A transition that is neither neighbour — both channels
/// apparently moving at once, which on real hardware means an edge was missed
/// — resets the sub-count rather than guessing a direction, so a missed edge
/// costs a detent instead of inventing one in the wrong direction.
#[derive(Clone, Copy, Debug)]
pub struct Quadrature {
    state: u8,
    sub: i8,
    position: i32,
}

impl Quadrature {
    /// Start from the resting state, `(a, b) = (0, 0)`.
    pub const fn new() -> Self {
        Self {
            state: 0,
            sub: 0,
            position: 0,
        }
    }

    /// The position as it stands.
    pub const fn position(&self) -> i32 {
        self.position
    }

    /// Feed the levels read at an edge. Returns a detent every fourth step in
    /// one direction.
    pub fn edge(&mut self, a: bool, b: bool) -> Option<Detent> {
        let state = (u8::from(a) << 1) | u8::from(b);
        if state == self.state {
            return None;
        }
        // The Gray cycle 00 -> 10 -> 11 -> 01 -> 00 as an index, so that
        // "one place forward" is arithmetic rather than a table of sixteen
        // cases. Clockwise is increasing index.
        const ORDER: [u8; 4] = [0b00, 0b10, 0b11, 0b01];
        let index = |s: u8| ORDER.iter().position(|o| *o == s).unwrap_or(0) as i8;
        let step = (index(state) - index(self.state)).rem_euclid(4);
        self.state = state;
        let delta = match step {
            1 => 1,
            3 => -1,
            // A two-place jump: an edge went missing. Say nothing rather than
            // guess a direction.
            _ => {
                self.sub = 0;
                return None;
            }
        };
        if self.sub != 0 && self.sub.signum() != delta {
            // A reversal mid-detent: start counting the new direction from
            // this step.
            self.sub = 0;
        }
        self.sub += delta;
        if self.sub.abs() < 4 {
            return None;
        }
        let cw = self.sub > 0;
        self.sub = 0;
        self.position += if cw { 1 } else { -1 };
        Some(Detent {
            position: self.position,
            cw,
        })
    }
}

impl Default for Quadrature {
    fn default() -> Self {
        Self::new()
    }
}

/// One button state change, as the debouncer accepted it.
///
/// ```text
/// [fw-check-json] {"kind":"gpio-input","reader":"button","state":"down",
///   "t_us":160042,"samples":9}
/// ```
pub fn emit_button(down: bool, t_us: u64, samples: u32) {
    emit_record_json(format_args!(
        r#"{{"kind":"gpio-input","reader":"button","state":"{}","t_us":{t_us},"samples":{samples}}}"#,
        if down { "down" } else { "up" }
    ));
}

/// One decoded detent.
///
/// ```text
/// [fw-check-json] {"kind":"gpio-input","reader":"encoder","position":1,
///   "dir":"cw","t_us":915231}
/// ```
pub fn emit_encoder(detent: Detent, t_us: u64) {
    emit_record_json(format_args!(
        r#"{{"kind":"gpio-input","reader":"encoder","position":{},"dir":"{}","t_us":{t_us}}}"#,
        detent.position,
        if detent.cw { "cw" } else { "ccw" }
    ));
}

/// Render [`SCRIPT`] as the `--pin-script` file the emulated side is driven
/// by.
///
/// Written through a [`core::fmt::Write`] sink rather than into a `String`,
/// the way [`crate::write_header`] is, so that a payload with no other reason
/// to pull in `alloc` does not gain one.
///
/// The grammar is the emulator's (`lp-emu-esp32c6/src/pinscript.rs`). Two
/// things about the shape are worth saying out loud:
///
/// - **The rest levels are absolute, at cycle 0.** They have to be there
///   before the firmware looks, and they are the same four lines whatever the
///   boot took.
/// - **Everything else is a delay from the step before, behind one `after`
///   fence on [`ARMED_MARKER`].** The absolute form would have been shorter,
///   and `button` and `encoder` generators exist that would have been shorter
///   still — but both are absolute-from-cycle-0 only, and the boot that
///   precedes arming is not the same length on two time grades, let alone on
///   silicon. An anchored chain is the only form whose edges land at the same
///   place in the firmware's own timeline on every configuration.
pub fn write_pin_script(w: &mut impl core::fmt::Write) -> core::fmt::Result {
    writeln!(w, "# gpio-input — the emulated side of the pads.")?;
    writeln!(w, "#")?;
    writeln!(
        w,
        "# GENERATED from `fw_checks::checks::gpio_input::SCRIPT`. Do not edit by hand:"
    )?;
    writeln!(
        w,
        "# `lp-cli`'s validate_registry_parity test holds this file equal to what"
    )?;
    writeln!(
        w,
        "# `write_pin_script` renders, so that the firmware's own timeline and this"
    )?;
    writeln!(w, "# one cannot drift apart quietly.")?;
    writeln!(w, "#")?;
    writeln!(
        w,
        "# gpio{DRIVE_SELECT_GPIO} high says an outside driver is present, which is what"
    )?;
    writeln!(
        w,
        "# makes this run report `drive=external`. Nothing is connected to that pad on"
    )?;
    writeln!(
        w,
        "# the desk board, so silicon reads it low and self-loops."
    )?;
    writeln!(w, "0us pin {DRIVE_SELECT_GPIO} 1")?;
    for pad in [Pad::Button, Pad::EncoderA, Pad::EncoderB] {
        writeln!(w, "0us pin {} {}", pad.gpio(), u8::from(pad.rest_level()))?;
    }
    let mut previous = 0u32;
    for (n, edge) in SCRIPT.iter().enumerate() {
        let delta = edge.at_us - previous;
        previous = edge.at_us;
        if n == 0 {
            writeln!(
                w,
                "after \"{ARMED_MARKER}\" +{delta}us pin {} {}",
                edge.pad.gpio(),
                u8::from(edge.level)
            )?;
        } else {
            writeln!(
                w,
                "then +{delta}us pin {} {}",
                edge.pad.gpio(),
                u8::from(edge.level)
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::string::String;
    use std::vec::Vec;

    /// The renderer emits one delay per row and the firmware walks the rows
    /// with an absolute deadline each. Both break on an out-of-order table,
    /// the renderer by underflowing.
    #[test]
    fn the_script_is_in_time_order() {
        let mut previous = 0;
        for edge in SCRIPT {
            assert!(
                edge.at_us >= previous,
                "SCRIPT is out of time order at {} us",
                edge.at_us
            );
            previous = edge.at_us;
        }
    }

    /// Every scripted **button** edge sits in the middle of a sample
    /// interval, which is the ±10 ms of tolerance the emulated side's anchor
    /// needs. An edge on a sample boundary would decide a `samples` field by
    /// which of two things happened first.
    ///
    /// The encoder's edges are deliberately not held to this: nothing samples
    /// them on a grid — an interrupt fires on each one — so their `position`
    /// and `dir` are functions of edge *order* alone and no tolerance is
    /// needed.
    #[test]
    fn every_button_edge_sits_between_two_samples() {
        let period_us = POLL_MS * 1_000;
        for edge in SCRIPT.iter().filter(|e| e.pad == Pad::Button) {
            let phase = edge.at_us % period_us;
            assert!(
                phase >= 1_000 && phase <= period_us - 1_000,
                "edge at {} us is {} us into a {} us sample interval — too close to a sample",
                edge.at_us,
                phase,
                period_us
            );
        }
    }

    /// The whole point of the burst: five edges, one state change.
    ///
    /// The debouncer itself is `lpc-hardware`'s and is not linked here (the
    /// `lp-emu/` fence's mirror image: this crate stays cheap), so what this
    /// test holds is the arithmetic the burst was designed against — the
    /// levels the sample grid sees, and the settle window they leave.
    #[test]
    fn the_bounce_burst_leaves_one_settled_edge_per_press() {
        let levels = |sample: u32| -> bool {
            let at_us = sample * POLL_MS * 1_000;
            SCRIPT
                .iter()
                .filter(|e| e.pad == Pad::Button && e.at_us <= at_us)
                .next_back()
                .map(|e| e.level)
                .unwrap_or(Pad::Button.rest_level())
        };
        // Samples 0..=5 are before the burst; 6 (120 ms) is the first to see
        // it settled, and it stays settled through the 30 ms window.
        assert!(levels(5), "sample 5 (100 ms) is before the press");
        for sample in 6..=15 {
            assert!(!levels(sample), "sample {sample} should read pressed");
        }
        // The release, and the 30 ms after it.
        for sample in 16..=20 {
            assert!(levels(sample), "sample {sample} should read released");
        }
    }

    /// The glitch is seen by exactly one sample and is therefore never stable
    /// for the 30 ms the debouncer needs.
    #[test]
    fn the_glitch_is_seen_by_exactly_one_sample() {
        let period_us = POLL_MS * 1_000;
        let low_at = 710_000;
        let high_at = 730_000;
        let samples_inside = (0..BUTTON_SAMPLES)
            .filter(|k| {
                let at = k * period_us;
                at >= low_at && at < high_at
            })
            .count();
        assert_eq!(
            samples_inside, 1,
            "the glitch must be visible to one sample: any fewer and it is not \
             exercised, any more and 30 ms of stability could accrue"
        );
    }

    #[test]
    fn a_clockwise_turn_is_one_detent_every_four_transitions() {
        let mut q = Quadrature::new();
        // a rises, b rises, a falls, b falls.
        assert_eq!(q.edge(true, false), None);
        assert_eq!(q.edge(true, true), None);
        assert_eq!(q.edge(false, true), None);
        assert_eq!(
            q.edge(false, false),
            Some(Detent {
                position: 1,
                cw: true
            })
        );
    }

    #[test]
    fn a_counter_clockwise_turn_comes_back_to_zero() {
        let mut q = Quadrature::new();
        for (a, b) in [(true, false), (true, true), (false, true), (false, false)] {
            q.edge(a, b);
        }
        assert_eq!(q.position(), 1);
        // b rises, a rises, b falls, a falls.
        assert_eq!(q.edge(false, true), None);
        assert_eq!(q.edge(true, true), None);
        assert_eq!(q.edge(true, false), None);
        assert_eq!(
            q.edge(false, false),
            Some(Detent {
                position: 0,
                cw: false
            })
        );
    }

    /// Walking the script's own encoder half must produce exactly the four
    /// detents the payload claims: 1, 2 clockwise then 1, 0 back.
    #[test]
    fn the_script_decodes_to_four_detents() {
        let mut q = Quadrature::new();
        let (mut a, mut b) = (false, false);
        let mut detents: Vec<Detent> = Vec::new();
        for edge in SCRIPT {
            match edge.pad {
                Pad::EncoderA => a = edge.level,
                Pad::EncoderB => b = edge.level,
                Pad::Button => continue,
            }
            if let Some(d) = q.edge(a, b) {
                detents.push(d);
            }
        }
        assert_eq!(
            detents.as_slice(),
            &[
                Detent {
                    position: 1,
                    cw: true
                },
                Detent {
                    position: 2,
                    cw: true
                },
                Detent {
                    position: 1,
                    cw: false
                },
                Detent {
                    position: 0,
                    cw: false
                },
            ]
        );
    }

    /// The rendered script is the file the emulated side runs, so the shape
    /// of it is worth pinning: the rest levels first, then one anchored
    /// fence, then one delay per row.
    #[test]
    fn the_rendered_script_is_one_anchor_and_one_delay_per_edge() {
        let mut out = String::new();
        write_pin_script(&mut out).expect("a String never fails to write");
        let lines: Vec<&str> = out
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && !l.trim().is_empty())
            .collect();
        assert_eq!(lines[0], "0us pin 2 1");
        assert_eq!(lines[1], "0us pin 20 1");
        assert_eq!(lines[2], "0us pin 0 0");
        assert_eq!(lines[3], "0us pin 1 0");
        assert_eq!(
            lines[4],
            "after \"[gpio-input] === ARMED ===\" +110000us pin 20 0"
        );
        assert_eq!(lines[5], "then +1000us pin 20 1");
        assert_eq!(lines.len(), 4 + SCRIPT.len());
        assert_eq!(
            lines.last().copied(),
            Some("then +5000us pin 0 0"),
            "the last edge is the counter-clockwise run's a-falls"
        );
    }
}
