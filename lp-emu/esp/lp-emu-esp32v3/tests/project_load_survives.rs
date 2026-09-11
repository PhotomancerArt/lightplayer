//! M4 P4b's classic-level pin: the shipped image survives a project load.
//!
//! Before the fix in `lp-xt-emu`'s `executor/call.rs` (CALL0/CALLX0 wrote
//! `PS.CALLINC = 0`), loading **any** project killed the guest a few emulated
//! seconds in: the 1 ms pacer landed between a `call8` and its callee's
//! `entry`, xtensa-lx-rt's vector reached `save_context` through a `call0`
//! and saved a `PS` whose CALLINC was already zero, and the `entry` re-run
//! after `rfe` rotated by nothing — writing the callee's SP into the
//! caller's `a1`, whose next `retw` then took `_WindowUnderflow8` from the
//! wrong save area and reloaded `a1 = 0`. The crate README's "The window,
//! across a context save" has the trace; `lp-xt-emu`'s `mach_callinc`
//! fixture is the architectural pin. This is the product-level one: the same
//! image, the same upload script, `--strict-bus`, run to a deadline past
//! where it used to die, and the frames counted.
//!
//! **Counted in frames, never in emulated microseconds.** The deadline is a
//! run bound, not the gate: the gate is that the run *reaches* it, that no
//! access was unmapped, that the project loaded, that the server loop's
//! heartbeat is still arriving after the load, and that at least sixty whole
//! frames came off pad 18.
//!
//! `#[ignore]`d and gated on the reference image exactly as `tests/boot_idle.rs`
//! is (`test_support::fw_esp32v3_image`): `just test-emu-esp32v3-boot` builds
//! the image and runs it; a bare `cargo test` skips it.

use lp_emu_esp32v3::control;
use lp_emu_esp32v3::machine::{AppSource, BootMode, Esp32V3Builder, Outcome, StopCondition};
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, skip_notice};

/// The pad the scratch project's endpoint is rewritten to (`D10 -> IO18`;
/// the script's header says why).
const PAD: u8 = 18;

/// Emulated time the run is given. The shipped image built at `63965f5c7`
/// died at 4.556 s on this machine before the fix (its `frame-dump` build at
/// 3.794 s; P4's own image at 2.574 s), and a regression of the same shape
/// kills the guest within the first few hundred pacer ticks that land in a
/// call/entry gap once rendering starts (~3 s in) — seconds, never minutes.
/// Six seconds is past every measured crash with margin, gives rendering
/// three seconds of ticks, and leaves ~1,200 frames on the pad at the
/// walk's 30 ms chunk pacing. At 8 s the run cost 224 s of wall clock on an
/// M2 Max; the emulated seconds are the price, so they are not spent idly.
const DEADLINE_US: u64 = 6_000_000;

/// The line the classic answers the upload's `loadProject` with.
const LOADED: &str = "\"loadProject\":{\"handle\":1}";
/// The server loop's wire heartbeat (`M!{"id":0,"msg":{"heartbeat":{...`),
/// sent every five seconds of guest time by the loop that owns the render
/// tick. One after the load is the proof the loop outlived it. The
/// `[stack] heartbeat:` triple is *not* used: `tests/boot_idle.rs` documents
/// that it is elicited by memory-stats calls, not emitted on the loop's own
/// schedule.
const WIRE_HEARTBEAT: &str = "\"msg\":{\"heartbeat\":{";

/// One WS281x frame of the project's 64 LEDs, three bytes each.
const WHOLE_FRAME_BYTES: usize = 64 * 3;
const MIN_WHOLE_FRAMES: usize = 60;

fn script() -> lp_emu_esp_common::ScriptedSource {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/walks/shader-oracle.script");
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    control::parse_byte_script(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
#[ignore = "needs the reference image: run through `just test-emu-esp32v3-boot`"]
fn the_shipped_image_survives_a_project_load_and_keeps_rendering() {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice(
                "the_shipped_image_survives_a_project_load_and_keeps_rendering",
                &reason,
            );
            return;
        }
    };
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .uart0_script(script())
        .build()
        .expect("the shipped image builds a machine");

    let outcome = machine.run_until(&StopCondition::after_micros(DEADLINE_US));
    let text = machine.uart0().text();
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the run reaches its deadline rather than stopping: {outcome:?}\n--- UART0 ---\n{text}"
    );
    assert!(
        machine.first_strict_violation().is_none(),
        "no strict-bus violation: {:?}",
        machine.first_strict_violation()
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "zero unmapped accesses"
    );

    let loaded_at = text.find(LOADED).unwrap_or_else(|| {
        panic!("the project loaded ({LOADED:?} on the console)\n--- UART0 ---\n{text}")
    });
    assert!(
        text[loaded_at..].contains(WIRE_HEARTBEAT),
        "the server loop's heartbeat still arrives after the load\n--- UART0 after the load ---\n{}",
        &text[loaded_at..]
    );

    machine.flush_frames();
    let frames = machine.frames(PAD);
    // Whole: the decoder's own `is_complete` (every pulse decoded, the bits
    // made whole, the reset seen) and the project's 64 LEDs on the wire.
    let whole = frames
        .iter()
        .filter(|f| f.is_complete() && f.wire.len() == WHOLE_FRAME_BYTES)
        .count();
    let bit_errors: u64 = frames.iter().map(|f| f.error_count).sum();
    assert_eq!(
        bit_errors,
        0,
        "no bit errors in {} frames on pad {PAD}",
        frames.len()
    );
    assert!(
        whole >= MIN_WHOLE_FRAMES,
        "at least {MIN_WHOLE_FRAMES} whole frames on pad {PAD}: {whole} whole of {} decoded",
        frames.len()
    );
}
