//! M2 P1's machine-level gates: a wire carries a level (G1-2), and a pin
//! script is the same run twice (G1-3).
//!
//! These run against the **vendored mask ROM and nothing else**
//! (`AppSource::None`), so they need no firmware build and are not
//! `#[ignore]`d — every workspace test run pays for them, which is right for
//! a determinism claim. The guest is beside the point here: what is under
//! test is the fabric, the pin log and the script's timeline, and a run with
//! no application exercises all three while holding the guest's own
//! behaviour fixed.
//!
//! The register-level half of the input side — esp-hal's own sequences
//! replayed byte for byte — is in `src/periph/gpio.rs`'s tests (G1-1), where
//! the `Sandbox` harness lives.

use std::path::PathBuf;

use lp_emu_esp_common::pins::PadId;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, Outcome, PinLogSink, StopCondition, TimeGrade,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::pinscript::{parse_pin_script, parse_wire};
use sha2::{Digest, Sha256};

/// `IO_MUX.gpio[n]`, `+0x004 + 4n`.
fn io_mux(pad: u32) -> u32 {
    memmap::periph::IO_MUX + 0x004 + 4 * pad
}

/// `GPIO.in_`.
const GPIO_IN: u32 = memmap::periph::GPIO + 0x03c;
/// `GPIO.out_w1ts` / `GPIO.enable_w1ts` / `func_out_sel_cfg[n]`.
const GPIO_OUT_W1TS: u32 = memmap::periph::GPIO + 0x008;
const GPIO_ENABLE_W1TS: u32 = memmap::periph::GPIO + 0x024;
fn func_out_sel_cfg(pad: u32) -> u32 {
    memmap::periph::GPIO + 0x554 + 4 * pad
}

/// `IO_MUX.gpio[n]` reset (`fun_drv = 2`) with `fun_ie` set and the matrix
/// function selected — what esp-hal leaves behind for an input pin.
const IO_MUX_INPUT: u32 = 0x0800 | (1 << 12) | (1 << 9);
/// `func_out_sel_cfg[n].out_sel = 128`: follow `GPIO_OUT[n]`.
const OUT_SEL_GPIO: u32 = 128;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("lp-emu-c6-pin-input");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join(name)
}

/// A ROM-only machine with the pin log open.
fn machine(script: &str, wires: &[&str], log: &PathBuf) -> Esp32C6Machine {
    let mut builder = Esp32C6Builder::new()
        .app(AppSource::None)
        .time_grade(TimeGrade::T1)
        .pin_log(PinLogSink::File(log.clone()));
    for w in wires {
        let (a, b) = parse_wire(w).expect("a permitted wire");
        builder = builder.wire(a, b);
    }
    if !script.is_empty() {
        builder = builder.pin_script(parse_pin_script(script).expect("a valid script"));
    }
    builder.build().expect("the machine builds")
}

fn run_for(m: &mut Esp32C6Machine, micros: u64) -> Outcome {
    m.run_until(&StopCondition::after_micros(micros))
}

/// **G1-2: a wire carries a level.**
///
/// `--wire 20:21` and a script that drives gpio20. The level appears on
/// gpio21's `in_` — a real MMIO read through the bus's own decode, with
/// gpio21's input buffer turned on through `IO_MUX` the way the driver does
/// it — and both pads' edges are in the pin log.
#[test]
fn g1_2_a_wire_carries_a_driven_level_onto_the_far_pads_in_register() {
    let log = scratch("g1-2.pinlog");
    let mut m = machine("1000 pin 20 1\n2000 pin 20 0\n", &["20:21"], &log);

    // gpio21's input buffer on, the way `Input::new` turns it on.
    assert!(m.poke_word(io_mux(21), IO_MUX_INPUT));
    assert_eq!(m.peek_word(GPIO_IN), Some(0), "nothing is driving it yet");

    assert!(matches!(run_for(&mut m, 1_500), Outcome::Deadline { .. }));
    assert_eq!(
        m.peek_word(GPIO_IN),
        Some(1 << 21),
        "the level driven on gpio20 reads back on gpio21"
    );

    assert!(matches!(run_for(&mut m, 2_500), Outcome::Deadline { .. }));
    assert_eq!(m.peek_word(GPIO_IN), Some(0), "and it follows back down");

    // Both sides of the wire, as the `pins` verb reports them.
    let pads = m.pads();
    let g20 = pads.iter().find(|p| p.pad == 20).expect("gpio20");
    let g21 = pads.iter().find(|p| p.pad == 21).expect("gpio21");
    assert_eq!(g20.wired_to, [21]);
    assert_eq!(g21.wired_to, [20]);
    assert_eq!(g20.driven, Some(false), "the script is holding it low");
    assert_eq!(g21.driven, None, "the far pad has no driver of its own");
    assert!(g21.input_enable);

    drop(m);
    let text = std::fs::read_to_string(&log).expect("the pin log");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "1000.000 gpio20 1",
            "1000.000 gpio21 1",
            "2000.000 gpio20 0",
            "2000.000 gpio21 0",
        ],
        "the edge is in the pin log, on both pads, at the script's microsecond"
    );
}

/// A pad's own **output** across a wire, which is the shape M2 P3's RMT
/// loopback needs: gpio18 routed to `GPIO_OUT`, `--wire 18:19`, and gpio19
/// reading it.
#[test]
fn a_wired_pads_own_output_reaches_the_far_pads_in_register() {
    let log = scratch("loopback.pinlog");
    let mut m = machine("", &["18:19"], &log);
    assert!(m.poke_word(io_mux(19), IO_MUX_INPUT));
    assert!(m.poke_word(func_out_sel_cfg(18), OUT_SEL_GPIO));
    assert!(m.poke_word(GPIO_ENABLE_W1TS, 1 << 18));
    assert_eq!(m.peek_word(GPIO_IN), Some(0));

    assert!(m.poke_word(GPIO_OUT_W1TS, 1 << 18));
    assert_eq!(
        m.peek_word(GPIO_IN),
        Some(1 << 19),
        "gpio18's output arrives on gpio19"
    );
    let pads = m.pads();
    let g18 = pads.iter().find(|p| p.pad == 18).expect("gpio18");
    assert_eq!(g18.route.as_deref(), Some("gpio-out"));
    assert!(g18.level);
}

/// **G1-3: scripts are deterministic.**
///
/// The same file, twice, including a `button` with bounce and an `encoder` —
/// byte-identical pin logs and identical cycle counts.
#[test]
fn g1_3_the_same_pin_script_run_twice_is_the_same_run() {
    const SCRIPT: &str = "\
# a button with contact bounce and a quadrature pair
button 20 press at 1000 bounce 5 edges over 200 hold 2ms
encoder 21 22 8 cw from 4000 at 4000hz
5000 pin 23 1
";
    let mut digests = Vec::new();
    let mut cycles = Vec::new();
    for run in 0..2 {
        let log = scratch(&format!("g1-3-run{run}.pinlog"));
        let mut m = machine(SCRIPT, &[], &log);
        assert!(matches!(run_for(&mut m, 10_000), Outcome::Deadline { .. }));
        cycles.push(m.cycles());
        drop(m);
        let bytes = std::fs::read(&log).expect("the pin log");
        assert!(!bytes.is_empty(), "run {run} logged nothing");
        digests.push(format!("{:x}", Sha256::digest(&bytes)));
    }
    assert_eq!(
        digests[0], digests[1],
        "G1-3 pin-log sha256 {} / {}",
        digests[0], digests[1]
    );
    assert_eq!(
        cycles[0], cycles[1],
        "G1-3 cycles {} / {}",
        cycles[0], cycles[1]
    );
}

/// The generators' edges reach the pads at the microseconds their module doc
/// states — the same list `pinscript`'s unit test asserts, now through the
/// whole machine and out of the pin log.
#[test]
fn the_generators_land_on_the_pads_at_the_documented_microseconds() {
    let log = scratch("generators.pinlog");
    let mut m = machine(
        "button 20 press at 1000 bounce 3 edges over 100 hold 1ms\n\
         encoder 21 22 4 ccw from 3000 at 2000hz\n",
        &[],
        &log,
    );
    assert!(matches!(run_for(&mut m, 6_000), Outcome::Deadline { .. }));
    drop(m);
    let text = std::fs::read_to_string(&log).expect("the pin log");
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        [
            "0.000 gpio20 1",    // resting: a pull-up, not pressed
            "1000.000 gpio20 0", // the press
            "1050.000 gpio20 1", // bounce
            "1100.000 gpio20 0", // settled pressed
            "2100.000 gpio20 1", // released, 1 ms after the burst
            "3000.000 gpio22 1", // ccw: b^
            "3500.000 gpio21 1", //      a^
            "4000.000 gpio22 0", //      bv
            "4500.000 gpio21 0", //      av
        ]
    );
}

/// A script's `after` needle is unresolved for a run whose consoles never
/// say it, and the script says so by not emptying rather than by firing.
#[test]
fn an_unmet_needle_never_fires_and_the_run_still_ends() {
    let log = scratch("unmet.pinlog");
    let mut m = machine("after \"nothing says this\" pin 20 1\n", &[], &log);
    assert!(matches!(run_for(&mut m, 3_000), Outcome::Deadline { .. }));
    drop(m);
    let text = std::fs::read_to_string(&log).expect("the pin log");
    assert!(text.is_empty(), "no edge: {text:?}");
}

/// **G1-7, at the flag.** A `--wire` that names a forbidden pad is refused
/// before the machine is built, by name and with the reason; gpio18 on the
/// TX side is the one exception.
#[test]
fn g1_7_a_wire_onto_a_spoken_for_pad_is_refused_at_the_flag() {
    for (pad, why) in lp_emu_esp32c6::pinscript::FORBIDDEN_PADS {
        let err = parse_wire(&format!("20:{pad}")).unwrap_err();
        assert!(err.contains(&format!("gpio{pad}")), "{err}");
        assert!(err.contains(why), "{err}");
    }
    assert_eq!(parse_wire("18:19").unwrap(), (PadId(18), PadId(19)));
    assert!(parse_wire("19:18").is_err(), "gpio18 is the TX side only");
}
