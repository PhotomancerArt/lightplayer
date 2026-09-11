//! **The pad**: the WS281x decoders on the whole machine, at 240 MHz, fed
//! from the fabric by `Machine::drain_pins`.
//!
//! M4 P2 proved a frame off the RMT block in isolation — a `Sandbox`, a
//! peripheral, and the test decoding the edges itself
//! (`rmt_registers.rs::a_ws281x_frame_goes_out_word_for_word_and_decodes_back`).
//! This file is the same waveform through the **machine**: the run loop's own
//! window boundaries, the fabric's routing epoch, the machine's decoders, the
//! `ws281x-frame` dump and the pin log. What it adds over P2's test is
//! everything between the block and a reader: that a pad becomes observed
//! when the guest routes it, that the frames survive a snapshot, that two
//! runs write byte-identical dumps, and that none of it moves with
//! `--core-quantum`.
//!
//! ⚠️ **This file compares pixels to nothing.** It proves a waveform exists,
//! is whole, is named, and is deterministic. The oracle comparison is P4's,
//! deliberately: mixing the two would make a decoder-threshold bug at 240 MHz
//! look like a renderer bug.
//!
//! ⚠️ **All three readings are ours.** The words the RMT fetched, the edges on
//! the fabric and the bytes decoded here are one model read three ways, not a
//! measurement of silicon. The silicon twin of the `.pins.jsonl` transcript is
//! M5's.
//!
//! # Why the guest is not the one driving
//!
//! The shipped image configures its four two-block slots at boot and then
//! **starts no channel until a project's output opens**, which needs an
//! `lp-cli upload` over UART0 — blocked by ruling **R6** (see the PR body and
//! the crate README). So the register sequence here is the shipped image's
//! own, quoted value for value out of a `--trace-block RMT` run of it, and
//! the host writes it through the same bus decode the guest would.

use lp_emu_esp32v3::machine::{
    BootMode, Esp32V3Builder, FrameSink, Machine, PinLogSink, StopCondition, StripConfig,
};
use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::regnames::RegNames;
use lp_emu_esp32v3::periph::rmt;
use lp_emu_esp32v3::{memmap, regs};

// ---- the wire, restated inside the emulator's fence ----------------------

// `just lint-emu-fence`: nothing under `lp-emu/` imports a product crate for
// its *values*. These are `lp_ws281x::ChannelTiming::WS2812` — 400/850 ns for
// a zero, 800/450 for a one, a 300 µs latch — through `ns_to_ticks` at 80 MHz,
// the only RMT clock esp-hal's classic `validate_clock` accepts. P2's test
// carries the same five numbers and the same citation.
const T0H: u16 = 32;
const T0L: u16 = 68;
const T1H: u16 = 64;
const T1L: u16 = 36;
const LATCH: u16 = 24_000;

/// One RMT tick at `div_cnt = 1`: `CPU_HZ / APB_HZ`, computed, never a
/// literal 3 — the classic's is three where the C6's is two.
const CYCLES_PER_TICK: u64 = memmap::CPU_HZ / 80_000_000;

/// The pad the DOM-Z-102 calls "data channel 1", and the wire M4's walk uses.
const GPIO18: u8 = 18;

/// `chNconf1.tx_start`, bit 0. The view keeps it private (it is a strobe that
/// reads back 0); the number is the PAC's.
const CONF1_TX_START: u32 = 1 << 0;

fn word(l1: bool, d1: u16, l2: bool, d2: u16) -> u32 {
    let half = |level: bool, ticks: u16| (u32::from(level) << 15) | u32::from(ticks & 0x7fff);
    half(l1, d1) | (half(l2, d2) << 16)
}

fn bit_word(one: bool) -> u32 {
    if one {
        word(true, T1H, false, T1L)
    } else {
        word(true, T0H, false, T0L)
    }
}

/// The word stream for `bytes`: one word per bit, MSB first, then the latch
/// and the all-zero end marker.
fn frame_words(bytes: &[u8]) -> Vec<u32> {
    let mut out: Vec<u32> = bytes
        .iter()
        .flat_map(|b| (0..8).rev().map(move |i| bit_word(b & (1 << i) != 0)))
        .collect();
    out.push(word(false, LATCH, false, 0));
    out.push(0);
    out
}

// ---- absolute addresses, out of the generated tables ----------------------

fn off(names: &RegNames, name: &str) -> u32 {
    names
        .entries
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(o, _)| *o)
        .unwrap_or_else(|| panic!("{} has no register called {name}", names.block))
}

fn rmt_reg(name: &str) -> u32 {
    memmap::periph::RMT + off(&regs::RMT, name)
}

fn conf0(ch: usize) -> u32 {
    rmt_reg("ch0conf0") + 8 * ch as u32
}

fn conf1(ch: usize) -> u32 {
    rmt_reg("ch0conf1") + 8 * ch as u32
}

fn tx_lim(ch: usize) -> u32 {
    rmt_reg("ch0_tx_lim") + 4 * ch as u32
}

fn ram(word: u32) -> u32 {
    memmap::periph::RMT_RAM + 4 * word
}

fn gpio_reg(name: &str) -> u32 {
    memmap::periph::GPIO + off(&regs::GPIO, name)
}

// ---- the rig -------------------------------------------------------------

/// A whole machine with the peripherals registered, booting the mask ROM with
/// a blank flash.
///
/// The ROM is running on core 0 the whole time and core 1 is held by DPORT,
/// exactly as they are at power-on. That is deliberate: the point of this file
/// is the **machine's** drain, so the run loop has to be a real one with
/// windows, events and an idle skip, rather than a `Sandbox` stepped by hand.
/// Nothing the ROM does touches the RMT or `func18_out_sel_cfg`; the test
/// asserts the routing it wrote is still there at the end, so a ROM that
/// started to would fail here rather than confuse a frame.
fn rig(dump: FrameSink, pin_log: PinLogSink) -> Machine {
    Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .rmt_logs(true)
        .dump_frames(dump)
        .pin_log(pin_log)
        .build()
        .expect("a machine with no --elf boots the vendored ROM")
}

/// Route IO18 to `RMT_SIG_0` and configure channel 0 the way the shipped
/// image's own `--trace-block RMT` run does (M4 P2's PR body quotes the run
/// line for line).
fn arm_channel_0(m: &mut Machine) {
    assert!(m.poke_word(gpio_reg("enable_w1ts"), 1 << GPIO18));
    assert!(m.poke_word(
        gpio_reg("func18_out_sel_cfg"),
        u32::from(rmt::RMT_SIG_0)
    ));

    assert!(m.poke_word(rmt_reg("apb_conf"), 0x0000_0001)); // Rmt::new
    assert!(m.poke_word(conf1(0), 0x0002_0f20)); // configure_clock: APB
    assert!(m.poke_word(conf0(0), 0x0210_0001)); // mem_size 2, div_cnt 1
    assert!(m.poke_word(conf1(0), 0x000a_0f20)); // idle_out_en, idle low
    assert!(m.poke_word(rmt_reg("apb_conf"), 0x0000_0003)); // the GLOBAL wrap bit
}

const WINDOW: usize = 128;
const HALF: usize = 64;

/// Fill both halves, arm the threshold at the half, and start the channel.
fn start_frame(m: &mut Machine, stream: &[u32]) {
    for (w, v) in stream.iter().take(WINDOW).enumerate() {
        assert!(m.poke_word(ram(w as u32), *v));
    }
    assert!(m.poke_word(tx_lim(0), HALF as u32));
    assert!(m.poke_word(conf1(0), 0x000a_0f20 | CONF1_TX_START));
}

/// Run the machine forward in short windows, performing the refill the
/// firmware's ISR performs, until the channel raises `tx_end`.
///
/// Returns how many refills it took. The steps are **cycles**, never emulated
/// microseconds, and nothing here is gated on one (PD9).
fn run_the_frame(m: &mut Machine, stream: &[u32], step: u64) -> usize {
    let mut next = WINDOW;
    let mut refills = 0usize;
    let mut half = 0usize;
    for _ in 0..4_000 {
        let raw = m
            .peek_word(rmt_reg("int_raw"))
            .expect("the RMT answers a host read");
        if raw & rmt::int_tx_end_bit(0) != 0 {
            break;
        }
        if raw & rmt::int_thr_bit(0) != 0 {
            assert!(m.poke_word(rmt_reg("int_clr"), rmt::int_thr_bit(0)));
            assert!(m.poke_word(tx_lim(0), HALF as u32));
            for k in 0..HALF {
                let v = stream.get(next + k).copied().unwrap_or(0);
                assert!(m.poke_word(ram((half * HALF + k) as u32), v));
            }
            next += HALF;
            half ^= 1;
            refills += 1;
        }
        let until = m.clock() + step;
        m.run_until(&StopCondition {
            stop_cycle: Some(until),
            ..Default::default()
        });
    }
    refills
}

/// The 24 bytes the frame carries: eight LEDs, nothing near a colour anyone
/// could mistake for an oracle's.
fn payload() -> Vec<u8> {
    (0..24u8).map(|i| i.wrapping_mul(11) ^ 0x5a).collect()
}

// ---- the first waveform, through the machine ------------------------------

/// **A frame reaches a pad at all**: the routing is real, the engine pumps the
/// signal, the fabric carries it, and the machine's own decoder reads it.
///
/// The four assertions are in the order that makes each failure distinct:
///
/// 1. `routed_pads()` carries `(PadId(18), RMT_SIG_0)` — a failure here is the
///    GPIO matrix, not the RMT;
/// 2. `rmt_frames_ended(0) > 0` — a failure here is the RMT engine, not the
///    fabric;
/// 3. the first frame is whole, zero-error and closed by a reset — a failure
///    here is the fabric or the decoder's thresholds at 240 MHz;
/// 4. the bytes are the bytes that went in.
#[test]
fn a_frame_reaches_the_routed_pad() {
    let bytes = payload();
    let stream = frame_words(&bytes);
    assert_eq!(stream.len(), 194, "192 bit words, a latch, an end marker");

    let mut m = rig(FrameSink::Memory, PinLogSink::Off);
    arm_channel_0(&mut m);
    start_frame(&mut m, &stream);
    let refills = run_the_frame(&mut m, &stream, 2_000);
    assert!(refills >= 2, "the ping-pong refill ran ({refills} refills)");

    // (1) The matrix.
    assert_eq!(
        m.routed_pads()
            .into_iter()
            .find(|(pad, _)| *pad == PadId(GPIO18)),
        Some((
            PadId(GPIO18),
            RouteSource::Signal(SignalId(rmt::RMT_SIG_0), false)
        )),
        "the guest routed IO18 to RMT_SIG_0 and the drain saw the epoch change"
    );

    // (2) The engine.
    assert_eq!(m.rmt_frames_ended(0), 1, "the transmission ended");

    // (3) The fabric and the decoder. The frame is still open until the
    // machine is told the run is over — the latch is a long low, and a low
    // that has not been followed by anything is not yet a reset.
    m.flush_frames();
    let frames = m.frames(GPIO18);
    assert_eq!(frames.len(), 1, "one frame on the wire, one decoded");
    let frame = &frames[0];
    assert_eq!(frame.error_count, 0, "every pulse was a zero or a one");
    assert_eq!(frame.bits, bytes.len() * 8, "24 bytes of bits");
    assert_eq!(frame.trailing_bits, 0);
    assert_eq!(frame.pad, PadId(GPIO18));
    assert_eq!(frame.leds(), 8);

    // (4) The bytes. `wire` is what the wire carried; nothing here unpermutes
    // it, because this phase compares the frame to what was *transmitted* and
    // not to any renderer's idea of a colour.
    assert_eq!(
        frame.wire, bytes,
        "the frame off the pad is the frame that went in"
    );

    // Two edges a bit, and the latch adds none: it holds the low the last bit
    // already left on the wire, which is what a latch is.
    assert_eq!(m.pin_edges(GPIO18), 2 * (bytes.len() as u64 * 8));
    assert_eq!(m.pin_edges(2), 0, "a pad nothing routed sees nothing");
    assert!(m.frames(2).is_empty());
}

/// A frame the run ended in the middle of is reported **incomplete**, not
/// invented: `flush_frames` closes it with no reset, and `is_complete()` is
/// false. The whole frame above is complete only because its latch was
/// followed by the flush at a cycle 300 µs past the last edge.
#[test]
fn a_frame_still_in_flight_is_flushed_incomplete() {
    let bytes = payload();
    let stream = frame_words(&bytes);

    let mut m = rig(FrameSink::Memory, PinLogSink::Off);
    arm_channel_0(&mut m);
    start_frame(&mut m, &stream);
    // Half a frame: enough bits for the decoder to be mid-frame, nowhere near
    // the latch.
    let until = m.clock() + 40 * 300 * CYCLES_PER_TICK;
    m.run_until(&StopCondition {
        stop_cycle: Some(until),
        ..Default::default()
    });
    assert!(
        m.frames(GPIO18).is_empty(),
        "no reset has closed anything yet"
    );
    assert!(m.pin_edges(GPIO18) > 0, "but the wire has been busy");

    m.flush_frames();
    let frames = m.frames(GPIO18);
    assert_eq!(frames.len(), 1);
    assert!(frames[0].reset_cycles.is_none(), "no latch closed it");
    assert!(!frames[0].is_complete(), "and so it is not complete");
}

/// The two sinks write what the accessors hold, and the `ws281x-frame` record
/// names the signal rather than its number.
#[test]
fn the_sinks_write_the_frame_and_the_edges() {
    let dir = tempdir("sinks");
    let frames_path = dir.join("v3.frames.jsonl");
    let pins_path = dir.join("v3.pins.log");

    let bytes = payload();
    let stream = frame_words(&bytes);
    let mut m = rig(
        FrameSink::File(frames_path.clone()),
        PinLogSink::File(pins_path.clone()),
    );
    arm_channel_0(&mut m);
    start_frame(&mut m, &stream);
    run_the_frame(&mut m, &stream, 2_000);
    m.flush_frames();
    drop(m);

    let dump = std::fs::read_to_string(&frames_path).expect("the dump was written");
    let lines: Vec<&str> = dump.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "one line per decoded frame:\n{dump}");
    let record = lines[0];
    assert!(record.starts_with("{\"kind\":\"ws281x-frame\","), "{record}");
    assert!(record.contains("\"pad\":18,"), "{record}");
    assert!(
        record.contains("\"signal\":\"RMT_SIG_0\""),
        "the record names the signal, not sig87: {record}"
    );
    assert!(record.contains("\"leds\":8,"), "{record}");
    assert!(record.contains("\"errors\":0,"), "{record}");
    assert!(record.contains("\"complete\":true"), "{record}");
    // Both readings are in the record on purpose: GRB on the wire against RGB
    // as the driver was handed it, so a wrong order assumption is a visible
    // difference between two fields rather than a silent one inside `rgb`.
    let wire = hex(&bytes);
    assert!(record.contains(&format!("\"wire\":\"{wire}\"")), "{record}");
    assert!(
        !record.contains(&format!("\"rgb\":\"{wire}\"")),
        "GRB is not RGB, so the two fields must differ: {record}"
    );

    let pins = std::fs::read_to_string(&pins_path).expect("the pin log was written");
    assert!(
        pins.contains("# route gpio18 <- RMT_SIG_0 (out_sel=87 inv=0)"),
        "the log says which signal drives the pad:\n{}",
        &pins[..pins.len().min(400)]
    );
    let edges: Vec<&str> = pins.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(
        edges.len(),
        2 * bytes.len() * 8,
        "one line per edge, and only edges"
    );
    // `<us> <pad> <level> cyc=<cycle>`: the cycle is the number anything may
    // compute with; the microseconds are for a human (PD9).
    let first = edges[0];
    assert!(first.contains(" gpio18 1 cyc="), "{first}");
    assert!(edges[1].contains(" gpio18 0 cyc="), "{}", edges[1]);
    let cycle_of = |line: &str| -> u64 {
        line.rsplit_once("cyc=")
            .map(|(_, c)| c.parse().expect("a cycle"))
            .expect("every line stamps a cycle")
    };
    assert!(
        edges.windows(2).all(|w| cycle_of(w[0]) <= cycle_of(w[1])),
        "the edges are in guest-cycle order, whatever the window boundaries"
    );
}

/// **Determinism**: two runs of the same waveform write byte-identical dumps,
/// and a third at a different `--core-quantum` writes the same one again.
///
/// The quantum is a run parameter and two quanta are two interleavings, so
/// their *counters* may differ legitimately — what may not differ is anything
/// observable on the wire. A frame that moved with the quantum would be a race
/// the model was hiding.
#[test]
fn two_runs_and_two_quanta_decode_the_same_frame() {
    let dir = tempdir("determinism");
    let bytes = payload();
    let stream = frame_words(&bytes);

    let run = |name: &str, quantum: u64| -> Vec<u8> {
        let path = dir.join(name);
        let mut m = Esp32V3Builder::new()
            .boot_mode(BootMode::RomUp)
            .core_quantum(quantum)
            .dump_frames(FrameSink::File(path.clone()))
            .build()
            .expect("a machine with no --elf boots the vendored ROM");
        arm_channel_0(&mut m);
        start_frame(&mut m, &stream);
        run_the_frame(&mut m, &stream, 2_000);
        m.flush_frames();
        drop(m);
        std::fs::read(&path).expect("the dump was written")
    };

    let a = run("a.jsonl", 256);
    let b = run("b.jsonl", 256);
    assert_eq!(a, b, "two runs at one quantum are byte-identical");
    let c = run("c.jsonl", 64);
    assert_eq!(
        a, c,
        "and the quantum is a run parameter the wire cannot see"
    );
    assert!(!a.is_empty());
}

/// A decoder caught **mid-bit** carries real state — the partial byte, the bit
/// count, the cycle the pulse started — and it rides the snapshot.
///
/// The shape of the proof: run half a frame, snapshot, run the rest and keep
/// the frame; then restore and run the rest again. Both halves must decode to
/// the same bytes. A snapshot that dropped the decoder would resume a frame
/// that never existed, and the give-away is a single bad pulse at the join.
#[test]
fn the_decoders_ride_the_snapshot() {
    let bytes = payload();
    let stream = frame_words(&bytes);

    let mut m = rig(FrameSink::Memory, PinLogSink::Off);
    arm_channel_0(&mut m);
    start_frame(&mut m, &stream);

    // Far enough in that the decoder is mid-frame with a partial byte.
    let until = m.clock() + 37 * 300 * CYCLES_PER_TICK;
    m.run_until(&StopCondition {
        stop_cycle: Some(until),
        ..Default::default()
    });
    let mid = m.pin_state().clone();
    let decoder = mid
        .decoders
        .get(&GPIO18)
        .expect("the pad is being decoded");
    assert!(
        decoder.is_mid_frame(),
        "the snapshot is taken with a frame open"
    );
    assert_eq!(decoder.cpu_hz(), memmap::CPU_HZ, "240 MHz, not the C6's 160");
    let snap = m.snapshot();
    assert_eq!(snap.pins.decoders, mid.decoders, "the decoders are in it");

    let finish = |m: &mut Machine| -> Vec<u8> {
        run_the_frame(m, &stream, 2_000);
        m.flush_frames();
        let frames = m.frames(GPIO18);
        assert_eq!(frames.len(), 1, "one frame, whichever path got here");
        assert_eq!(frames[0].error_count, 0, "no bad pulse at the join");
        frames[0].wire.clone()
    };

    let straight_through = finish(&mut m);
    assert_eq!(straight_through, bytes);

    m.restore(&snap);
    assert_eq!(
        m.pin_state().decoders,
        mid.decoders,
        "restore puts the half-shifted bits back"
    );
    assert!(
        m.frames(GPIO18).is_empty(),
        "and the frame the first pass completed is gone with it"
    );
    let after_restore = finish(&mut m);
    assert_eq!(
        after_restore, straight_through,
        "the second half decodes identically from the restored decoder"
    );
}

/// `pin_report()` is one line per routed pad, and it is what the CLI prints.
#[test]
fn the_pin_report_names_every_routed_pad() {
    let bytes = payload();
    let stream = frame_words(&bytes);
    let mut m = rig(FrameSink::Memory, PinLogSink::Off);
    assert!(m.pin_report().is_empty(), "nothing is routed at power-on");

    arm_channel_0(&mut m);
    start_frame(&mut m, &stream);
    run_the_frame(&mut m, &stream, 2_000);
    m.flush_frames();

    let report = m.pin_report();
    assert_eq!(report.len(), 1, "{report:?}");
    assert_eq!(
        report[0],
        format!(
            "pin gpio18: 1 frames, 1 complete, 0 errors, 8 leds, {} edges",
            2 * bytes.len() * 8
        )
    );
    assert_eq!(m.strip(), StripConfig::default(), "WS2812, GRB on the wire");
}

// ---- small helpers -------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// A scratch directory under `target/`, named for the test, removed and
/// recreated so a rerun never reads the last run's file.
fn tempdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-esp32v3-pin-frames-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}
