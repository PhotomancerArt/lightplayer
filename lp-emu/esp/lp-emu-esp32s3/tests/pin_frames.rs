//! **The pad**: the WS281x decoders on the whole machine, at 240 MHz, fed
//! from the signal fabric by the machine's slice drain.
//!
//! `rmt_registers.rs` proves the register file is whole. This file is the
//! waveform through the **machine**: the run loop's own window boundaries,
//! the fabric's routing epoch, the machine's decoders, the `ws281x-frame`
//! dump and the pin log. What it adds is everything between the block and a
//! reader — that a pad becomes observed when something routes it, that an
//! open frame is flushed *incomplete* rather than invented, and that none of
//! it moves with `--core-quantum`.
//!
//! ⚠️ **This file compares pixels to nothing.** It proves a waveform exists,
//! is whole, is named, and is deterministic. The oracle comparison is the
//! walk's, and it lives in the PR body rather than here: mixing the two would
//! make a decoder-threshold bug at 240 MHz look like a renderer bug.
//!
//! ⚠️ **All three readings are ours.** The words the RMT fetched, the edges on
//! the fabric and the bytes decoded here are one model read three ways, not a
//! measurement of silicon. **No S3 silicon has been read at all.**
//!
//! # Why the host is the one driving
//!
//! The guest *does* drive this chip's RMT — the walk in `walks/` loads a
//! project and the strip lights up, and the PR body carries that frame three
//! ways. But a walk needs a firmware image, and its timing rides the link;
//! this file needs neither, so a plain `cargo test --workspace` runs it. The
//! register sequence is esp-hal's own `configure_tx`, the same one
//! `src/periph/rmt.rs`'s unit tests use, written through the same bus decode
//! the guest writes through.

use lp_emu_esp_common::ip::rmt::{
    CONF_APB_MEM_RST, CONF_CONF_UPDATE, CONF_DIV_CNT_SHIFT, CONF_MEM_RD_RST,
    CONF_MEM_TX_WRAP_EN, CONF_TX_START, Dir, IntKind,
};
use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::regnames::RegNames;
use lp_emu_esp32s3::machine::{
    Esp32S3Builder, FrameSink, Machine, PinLogSink, StopCondition, StripConfig,
};
use lp_emu_esp32s3::periph::rmt::{self, CONFIG};
use lp_emu_esp32s3::{memmap, regs};

// ---- the wire, restated inside the emulator's fence ----------------------

// `just lint-emu-fence`: nothing under `lp-emu/` imports a product crate for
// its *values*. These are `lp_ws281x::ChannelTiming::WS2812` — 400/850 ns for
// a zero, 800/450 for a one, a 300 µs latch — through `ns_to_ticks` at the
// 80 MHz APB source `sys_conf` selects below. The classic's `pin_frames.rs`
// carries the same five numbers: the RMT tick is 80 MHz on both chips even
// though the two blocks are different IP and reach it through different
// registers.
const T0H: u16 = 32;
const T0L: u16 = 68;
const T1H: u16 = 64;
const T1L: u16 = 36;
const LATCH: u16 = 24_000;

/// One RMT tick at `div_cnt = 1`: `CPU_HZ / APB_HZ`, computed, never a
/// literal 3 — the S3's and the classic's is three where the C6's is two.
const CYCLES_PER_TICK: u64 = memmap::CPU_HZ / 80_000_000;

/// One bit word on the wire, in channel ticks: a zero is `T0H + T0L` and a one
/// is `T1H + T1L`, and WS2812 makes those the same 1.25 µs. Having the number
/// once is what keeps "n words" from being written as a cycle count with the
/// tick conversion applied twice — 37 words is 37 × 100 × 3 cycles, and
/// `37 * 300 * CYCLES_PER_TICK` is 111 words, two thresholds further on.
const WORD_TICKS: u64 = (T0H + T0L) as u64;

/// Cycles for `n` bit words at `div_cnt = 1`.
fn words_in_cycles(n: u64) -> u64 {
    n * WORD_TICKS * CYCLES_PER_TICK
}

/// **D10 on the XIAO ESP32-S3, and no project retarget.**
/// `projects/test/shader-oracle` names `ws281x:local:D10`; the checked-in
/// `seeed/xiao-esp32-s3-plus` profile maps `D10` to `/gpio/9`. The classic
/// needed a scratch copy of the project (`D10 → IO18`) because the DOM-Z-102
/// has no D10. This chip renders the committed project unmodified, so the pad
/// is named here rather than in a comment.
const D10: u8 = 9;

/// `sys_conf` with the APB source selected and the divider at 1:1 —
/// `sclk_active` (bit 26), `sclk_sel` = 1 (bits 24:25, `Apb`), `sclk_div_num`
/// = 0 (bits 4:11). The S3's divider lives in the block's own `sys_conf`;
/// the C6 reaches the same 80 MHz through `PCR.rmt_sclk_conf`.
const SYS_CONF_80MHZ: u32 = (1 << 26) | (1 << 24) | 1;

/// The four TX blocks the driver claims, two 48-word blocks each.
const MEM_SIZE: u32 = 2;
const WINDOW: usize = 2 * 48;
const HALF: usize = 48;

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

fn rmt_reg(off_in_block: u32) -> u32 {
    memmap::periph::RMT + off_in_block
}

fn ram(word_index: u32) -> u32 {
    memmap::periph::RMT_RAM + 4 * word_index
}

fn gpio_reg(name: &str) -> u32 {
    memmap::periph::GPIO + off(&regs::GPIO, name)
}

// ⚠️ `Config::int_bit` answers a bit **position**, not a mask — it is the
// argument of a shift in the view's own `raise`. These two are the masks an
// ISR tests `int_raw` and writes `int_clr` with, so the shift belongs here.
// Getting it wrong is silent on channel 0's `tx_end` (position 0 masks
// nothing) and reads exactly like a channel that wraps and never refills.
fn tx_end_mask(ch: usize) -> u32 {
    1 << (CONFIG.int_bit)(Dir::Tx, IntKind::End, ch)
}

fn tx_thr_mask(ch: usize) -> u32 {
    1 << (CONFIG.int_bit)(Dir::Tx, IntKind::Thr, ch)
}

// ---- the rig -------------------------------------------------------------

/// A whole machine with the peripherals registered.
///
/// Core 0 runs the mask ROM the whole time and core 1 is held, exactly as
/// they are at power-on. That is deliberate: the point of this file is the
/// **machine's** drain, so the run loop has to be a real one with windows,
/// events and an idle skip, rather than a `Sandbox` stepped by hand. Nothing
/// the ROM does touches the RMT or `func9_out_sel_cfg`.
fn rig(dump: FrameSink, pin_log: PinLogSink) -> Machine {
    Esp32S3Builder::new()
        .rmt_logs(true)
        .dump_frames(dump)
        .pin_log(pin_log)
        .build()
        .expect("a machine with no app still has a bus and a ROM")
}

/// Route D10 to `RMT_SIG_0` and configure channel 0 the way esp-hal's
/// `configure_tx` does.
fn arm_channel_0(m: &mut Machine) {
    assert!(m.poke_word(gpio_reg("enable_w1ts"), 1 << D10));
    assert!(m.poke_word(
        gpio_reg("func9_out_sel_cfg"),
        u32::from(rmt::signal_of(0).0)
    ));

    // ⚠️ The clock first: an engine with no clock consumes no words, and the
    // S3 reads its divider out of the block's own `sys_conf`.
    assert!(m.poke_word(rmt_reg(CONFIG.sys_conf), SYS_CONF_80MHZ));
}

/// Fill both halves, arm the threshold at the half, and start the channel.
fn start_frame(m: &mut Machine, stream: &[u32]) {
    for (w, v) in stream.iter().take(WINDOW).enumerate() {
        assert!(m.poke_word(ram(w as u32), *v));
    }
    assert!(m.poke_word(rmt_reg(CONFIG.ch_tx_lim[0]), HALF as u32));

    let conf = (1 << CONF_DIV_CNT_SHIFT) | (MEM_SIZE << CONFIG.conf_mem_size_shift) | CONF_MEM_TX_WRAP_EN;
    assert!(m.poke_word(rmt_reg(CONFIG.ch_tx_conf0[0]), conf));
    assert!(m.poke_word(rmt_reg(CONFIG.ch_tx_conf0[0]), conf | CONF_CONF_UPDATE));
    assert!(m.poke_word(
        rmt_reg(CONFIG.ch_tx_conf0[0]),
        conf | CONF_MEM_RD_RST | CONF_APB_MEM_RST | CONF_TX_START,
    ));
    assert!(m.poke_word(rmt_reg(CONFIG.ch_tx_conf0[0]), conf | CONF_CONF_UPDATE));
}

/// Run the machine forward in short windows, performing the refill the
/// firmware's ISR performs, until the channel raises `tx_end`.
///
/// Returns how many refills it took. The steps are **cycles**, never emulated
/// microseconds, and nothing here is gated on one (PD9).
fn run_the_frame(m: &mut Machine, stream: &[u32], step: u64) -> usize {
    let mut next = WINDOW;
    let mut refills = 0usize;
    // The half the reader has just finished, which is the half to refill: the
    // first threshold is the one `start_frame` armed at `HALF`, and by then
    // words `0..HALF` are behind the read pointer.
    let mut half = 0usize;
    // ⚠️ **`tx_lim` is a position in the window, not a repeating count** — the
    // shared view's module doc says so, and this is what it costs to forget.
    // The ISR flips it between `HALF` and `WINDOW` (the wrap back to word 0);
    // re-arming it at `HALF` every time fires one threshold a lap instead of
    // two, so one of the two halves is never refilled and replays the words it
    // has already sent. The symptom is a frame with a whole window too many
    // bits in it, and before the mask above was fixed it was a channel that
    // wrapped and never refilled at all.
    let mut lim = WINDOW;
    for _ in 0..4_000 {
        let raw = m
            .peek_word(rmt_reg(CONFIG.int_raw))
            .expect("the RMT answers a host read");
        if raw & tx_end_mask(0) != 0 {
            break;
        }
        if raw & tx_thr_mask(0) != 0 {
            assert!(m.poke_word(rmt_reg(CONFIG.int_clr), tx_thr_mask(0)));
            assert!(m.poke_word(rmt_reg(CONFIG.ch_tx_lim[0]), lim as u32));
            for k in 0..HALF {
                let v = stream.get(next + k).copied().unwrap_or(0);
                assert!(m.poke_word(ram((half * HALF + k) as u32), v));
            }
            next += HALF;
            half ^= 1;
            lim = if lim == WINDOW { HALF } else { WINDOW };
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

fn tempdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-esp32s3-pin-frames-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

// ---- the first waveform, through the machine ------------------------------

/// **A frame reaches a pad at all**: the routing is real, the engine pumps the
/// signal, the fabric carries it, and the machine's own decoder reads it.
///
/// The assertions are in the order that makes each failure distinct:
///
/// 1. the strip's pad carries `RMT_SIG_0` — a failure here is the GPIO matrix,
///    not the RMT;
/// 2. `rmt_frames_ended(0) > 0` — a failure here is the RMT engine, not the
///    fabric;
/// 3. the frame is whole, zero-error and closed by a reset — a failure here is
///    the fabric or the decoder's thresholds at 240 MHz;
/// 4. the bytes are the bytes that went in.
///
/// ⚠️ **`routed_pads()`'s length is deliberately not asserted.** A boot routes
/// pads this file has no interest in; the claim that means something is that
/// the strip's pad is there, and that no *other* RMT signal is.
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

    // (1) The matrix. `RMT_SIG_0` is 81 on this chip.
    let sig0 = rmt::signal_of(0);
    assert_eq!(sig0, SignalId(81), "the S3's RMT_SIG_0, from the metadata");
    let routed = m.routed_pads();
    assert_eq!(
        routed.iter().find(|(pad, _)| *pad == PadId(D10)).copied(),
        Some((PadId(D10), RouteSource::Signal(sig0, false))),
        "D10 (gpio9) follows RMT_SIG_0 and the drain saw the epoch change"
    );
    for ch in 1..rmt::TX_CHANNELS {
        let other = rmt::signal_of(ch);
        assert!(
            !routed
                .iter()
                .any(|(_, src)| matches!(src, RouteSource::Signal(s, _) if *s == other)),
            "no other RMT signal is routed ({other:?} is)"
        );
    }

    // (2) The engine.
    assert_eq!(m.rmt_frames_ended(0), 1, "the transmission ended");

    // (3) The fabric and the decoder. The frame is still open until the
    // machine is told the run is over — the latch is a long low, and a low
    // that has not been followed by anything is not yet a reset.
    m.flush_frames();
    let frames = m.frames(D10);
    assert_eq!(frames.len(), 1, "one frame on the wire, one decoded");
    let frame = &frames[0];
    assert_eq!(frame.error_count, 0, "every pulse was a zero or a one");
    assert_eq!(frame.bits, bytes.len() * 8, "24 bytes of bits");
    assert_eq!(frame.trailing_bits, 0);
    assert_eq!(frame.pad, PadId(D10));
    assert_eq!(frame.leds(), 8);

    // (4) The bytes. `wire` is what the wire carried; nothing here unpermutes
    // it, because this file compares the frame to what was *transmitted* and
    // not to any renderer's idea of a colour.
    assert_eq!(
        frame.wire, bytes,
        "the frame off the pad is the frame that went in"
    );

    // Two edges a bit, and the latch adds none: it holds the low the last bit
    // already left on the wire, which is what a latch is.
    assert_eq!(m.pin_edges(D10), 2 * (bytes.len() as u64 * 8));
    assert_eq!(m.pin_edges(2), 0, "a pad nothing routed sees nothing");
    assert!(m.frames(2).is_empty());
}

/// A frame the run ended in the middle of is reported **incomplete**, not
/// invented: `flush_frames` closes it with no reset, and `is_complete()` is
/// false.
///
/// This is the same shape the walk shows at the CLI: a run cut at 3.2 s
/// emulated reported `90 frames, 89 complete` because the ninetieth was still
/// on the wire.
#[test]
fn a_frame_still_in_flight_is_flushed_incomplete() {
    let bytes = payload();
    let stream = frame_words(&bytes);

    let mut m = rig(FrameSink::Memory, PinLogSink::Off);
    arm_channel_0(&mut m);
    start_frame(&mut m, &stream);
    // Enough bits for the decoder to be mid-frame, nowhere near the latch.
    let until = m.clock() + words_in_cycles(40);
    m.run_until(&StopCondition {
        stop_cycle: Some(until),
        ..Default::default()
    });
    assert!(
        m.frames(D10).is_empty(),
        "no reset has closed anything yet"
    );
    assert!(m.pin_edges(D10) > 0, "but the wire has been busy");

    m.flush_frames();
    let frames = m.frames(D10);
    assert_eq!(frames.len(), 1);
    assert!(frames[0].reset_cycles.is_none(), "no latch closed it");
    assert!(!frames[0].is_complete(), "and so it is not complete");
}

/// The two sinks write what the accessors hold, and the `ws281x-frame` record
/// names the signal rather than its number.
#[test]
fn the_sinks_write_the_frame_and_the_edges() {
    let dir = tempdir("sinks");
    let frames_path = dir.join("s3.frames.jsonl");
    let pins_path = dir.join("s3.pins.log");

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
    let line = dump.lines().next().expect("one frame");
    assert!(line.contains("\"kind\":\"ws281x-frame\""), "{line}");
    assert!(line.contains("\"pad\":9"), "{line}");
    assert!(
        line.contains("\"signal\":\"RMT_SIG_0\""),
        "the record names the signal, not its number: {line}"
    );
    // Both readings, as two fields: a wrong order assumption is a visible
    // difference between them rather than a silent one inside `rgb`.
    assert!(line.contains("\"wire\":\""), "{line}");
    assert!(line.contains("\"rgb\":\""), "{line}");

    let pins = std::fs::read_to_string(&pins_path).expect("the pin log was written");
    let mut lines = pins.lines();
    assert_eq!(
        lines.next(),
        Some("# route gpio9 <- RMT_SIG_0 (out_sel=81 inv=0)"),
        "the routing change is noted before the first edge"
    );
    let first = lines.next().expect("an edge");
    assert!(first.contains("gpio9 1 cyc="), "{first}");
    assert_eq!(
        pins.lines().filter(|l| !l.starts_with('#')).count(),
        2 * bytes.len() * 8,
        "two edges a bit"
    );
}

/// **Determinism**, in the two halves the phase owes separately.
///
/// Two runs at one quantum are byte-identical — dump and pin log both. A
/// third at a different quantum decodes the same frame **and starts it on the
/// same cycle**: with the host driving, nothing between the register write
/// and the pad reads a clock the quantum can move, so a frame that shifted
/// would be a race the model was hiding.
///
/// ⚠️ **This is the RMT-and-pad claim, and it is narrower than the walk's.**
/// A *guest*-driven frame's start cycle does move with the quantum, because
/// the guest's own schedule does — the walk's project loads at a different
/// cycle and every frame after it shifts with it. That is upstream of this
/// block; see the PR body. What is asserted here is that the pad path adds no
/// quantum sensitivity of its own.
#[test]
fn two_runs_and_two_quanta_decode_the_same_frame() {
    let dir = tempdir("determinism");
    let bytes = payload();
    let stream = frame_words(&bytes);

    let run = |name: &str, quantum: u64| -> (Vec<u8>, Vec<u64>) {
        let path = dir.join(name);
        let mut m = Esp32S3Builder::new()
            .core_quantum(quantum)
            .dump_frames(FrameSink::File(path.clone()))
            .strip(StripConfig::default())
            .build()
            .expect("a machine with no app still has a bus and a ROM");
        arm_channel_0(&mut m);
        start_frame(&mut m, &stream);
        run_the_frame(&mut m, &stream, 2_000);
        m.flush_frames();
        let starts: Vec<u64> = m.frames(D10).iter().map(|f| f.start).collect();
        drop(m);
        (
            std::fs::read(&path).expect("the dump was written"),
            starts,
        )
    };

    let (a, a_starts) = run("a.jsonl", 256);
    let (b, b_starts) = run("b.jsonl", 256);
    assert_eq!(a, b, "two runs at one quantum are byte-identical");
    assert_eq!(a_starts, b_starts);

    let (c, c_starts) = run("c.jsonl", 1024);
    assert_eq!(
        a, c,
        "and the quantum is a run parameter the wire cannot see"
    );
    assert_eq!(
        a_starts, c_starts,
        "the frame starts on the same cycle at both quanta"
    );
    assert!(!a.is_empty());
    assert_eq!(a_starts.len(), 1);
}

/// A decoder caught **mid-bit** carries real state — the partial byte, the bit
/// count, the cycle the pulse started — and it rides the snapshot.
///
/// A snapshot that dropped the decoder would resume a frame that never
/// existed, and the give-away is a single bad pulse at the join.
#[test]
fn the_decoders_ride_the_snapshot() {
    let bytes = payload();
    let stream = frame_words(&bytes);

    let mut m = rig(FrameSink::Memory, PinLogSink::Off);
    arm_channel_0(&mut m);
    start_frame(&mut m, &stream);

    let until = m.clock() + words_in_cycles(37);
    m.run_until(&StopCondition {
        stop_cycle: Some(until),
        ..Default::default()
    });
    let mid = m.pin_state().clone();
    let decoder = mid.decoders.get(&D10).expect("the pad is being decoded");
    assert!(
        decoder.is_mid_frame(),
        "the snapshot is taken with a frame open"
    );
    assert_eq!(
        decoder.cpu_hz(),
        memmap::CPU_HZ,
        "240 MHz, not the C6's 160"
    );
    assert_eq!(memmap::CPU_HZ, 240_000_000);
    let snap = m.snapshot();
    assert_eq!(snap.pins.decoders, mid.decoders, "the decoders are in it");

    run_the_frame(&mut m, &stream, 2_000);
    m.flush_frames();
    let straight_through = m.frames(D10)[0].wire.clone();
    assert_eq!(straight_through, bytes);

    let mut restored = rig(FrameSink::Memory, PinLogSink::Off);
    restored.restore(&snap);
    assert_eq!(
        restored.pin_state().decoders,
        mid.decoders,
        "the half-shifted bits came back"
    );
    run_the_frame(&mut restored, &stream, 2_000);
    restored.flush_frames();
    assert_eq!(
        restored.frames(D10)[0].wire,
        bytes,
        "and the frame finished as the same bytes"
    );
}

/// **The walk is the C6's captured bytes.** One stimulus over two chips is
/// what makes one comparison mean one question, so the S3's walk script is
/// the C6's payload byte for byte — the only difference is a trigger line,
/// and this test names it rather than tolerating any difference at all.
#[test]
fn the_walk_is_the_c6s_captured_bytes() {
    let s3 = include_str!("../walks/shader-oracle.script");
    let c6 = include_str!("../../lp-emu-esp32c6/walks/shader-oracle.script");
    let payload = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.starts_with('#'))
            .map(str::to_string)
            .collect()
    };
    let (a, b) = (payload(s3), payload(c6));
    assert_eq!(a.len(), b.len(), "the same number of request lines");

    let differing: Vec<usize> = (0..a.len()).filter(|i| a[*i] != b[*i]).collect();
    assert_eq!(
        differing.len(),
        1,
        "exactly one line differs; the rest are the C6's capture unchanged"
    );
    let i = differing[0];
    // ⚠️ The one difference, and the reason it exists. The C6 waits on the
    // `stopAllProjects` reply's own bytes; on this chip the link drops one
    // 64 B packet of that reply
    // (`docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md`),
    // so a walk that waited on them would stall for ever at request 2 — it
    // does, and that is how this was found. The handler's own log line is the
    // same event, one record earlier, and waiting on it hides nothing: the
    // request was served either way. **Do not widen this to hide the drop
    // anywhere else**, and re-point it at the reply the day the defect closes.
    assert!(
        b[i].contains(r#"\"id\":1,"#),
        "the C6 waits on the reply: {}",
        b[i]
    );
    assert!(
        a[i].contains("Stopped all projects"),
        "and the S3 waits on the handler's own log line: {}",
        a[i]
    );
    assert_eq!(
        a[i].replace("Stopped all projects", r#"\"id\":1,"#),
        b[i],
        "and nothing else on that line moved"
    );
}
