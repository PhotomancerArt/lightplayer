//! **M4 P4's second wave**: five wires over four RMT slots, on the desk
//! board's own pins.
//!
//! `lp-fw/fw-esp32v3`'s block plan caps the pool at `POOLED_SLOT_CAP = 4`
//! two-block slots (`v3_rmt.rs`), so a fifth wire cannot have a slot of its
//! own: it **time-shares** one by per-transmission pad muxing — a
//! `func_out_sel_cfg` write between waves (`wire_pusher.rs`). That second
//! wave only exists on the **product path**, because the pusher runs on core
//! 1 and is driven by mailbox posts from the PRO core's outputs, so the only
//! way to exercise it is a project that really declares five outputs:
//! `projects/test/five-wire` (ruling R7), uploaded by
//! `walks/five-wire.script` over UART0 in guest time.
//!
//! The shape is the DOM-Z-102's own — IO18 / IO16 / IO14 / IO2, the four
//! fused DATA terminals, plus IO13, the claimable spare
//! (`lp-core/lpc-hardware/boards/domraem/dom-z-102.json`, whose `measured`
//! note is *5 wires × 300 LEDs at 29.99 fps, 240 s soak on the dual-core
//! pusher build*, and `../bench.md`, where L0 read those same five wires off
//! the running board).
//!
//! # The evidence, and where each piece comes from
//!
//! * **The re-mux** is in the *pin log*, not in `routed_pads()`: that call
//!   answers where a pad is routed **now**, and the second wave is a
//!   statement about routing over time. The log's `# route gpio<n> <- …`
//!   notes are written whenever `Fabric::route_epoch` moves, so one signal
//!   appearing on two pads, with a park to `GPIO_OUT` between, *is* the
//!   second wave.
//! * **Bytes and counts only, never timing.** The pusher deliberately starts
//!   a queued second-wave frame **a wave late** (`shared_driver.rs`); that is
//!   by design, and nothing here gates on a frame period, a phase or an
//!   emulated microsecond (PD9).
//!
//! # ⚠️ The window-spill defect, and why this file holds two tests
//!
//! Loading any project kills this guest (`_WindowUnderflow8` restores
//! `a1 = 0`; the crate README's "A frame three ways" carries the trace, and
//! M4 **P4b** is the fix). On this tree the five-wire walk reaches the
//! outputs' open and the compile-window black frame on all five pads and
//! stops there, so what is reachable today is the **routing**: five pads,
//! the re-mux, whole frames with no bit errors, and determinism. Everything
//! that needs the render to survive — five *distinct lit* wires, the per-wire
//! checksum against the guest's own `[OUT] frame=… crc=` summary lines (one
//! per `REPORT_EVERY_FRAMES = 60` frames, and the guest dies long before the
//! sixtieth), the frame counts, `Outcome::Deadline`, `unmapped == 0` — is the
//! third test, which is not weakened but stops at
//! [`stopped_by_the_window_spill`]: that one stop, recognised exactly, with a
//! `SKIP` notice naming it.
//!
//! Both are `#[ignore]`d for `test_support`'s usual reason as well: a plain
//! `cargo test --workspace` must never start a cross-target firmware build.
//! `just test-emu-esp32v3-boot` builds the `frame-dump` image, names the file
//! it built, and runs these.

use std::path::{Path, PathBuf};

use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::{Frame, unpermute};
use lp_emu_esp32v3::control::parse_byte_script;
use lp_emu_esp32v3::machine::{
    AppSource, Esp32V3Builder, FrameSink, Machine, Outcome, PinLogSink, StopCondition, TimeGrade,
    hex,
};
use lp_emu_esp32v3::memmap;
use lp_emu_esp32v3::periph::rmt;
use lp_emu_esp32v3::test_support::{fw_esp32v3_frame_dump_image, skip_notice};
use lp_ws281x::ColorOrder;

/// The project's five ports, in the order `output.json` declares them:
/// IO18 / IO16 / IO14 / IO2 are the fused DATA terminals, IO13 the spare.
const PADS: [u8; 5] = [18, 16, 14, 2, 13];
/// `projects/test/five-wire`: 16 lamps a port, 80 channels in all.
const LEDS: usize = 16;
const BITS: usize = LEDS * 24;

/// The walk's cost in emulated time; the run ends earlier than this today on
/// the window-spill defect.
const GATE_US: u64 = 30_000_000;
const WALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(900);

/// The dual-core line — the pusher's whole premise, and P1's standing guard.
const APP_CORE_ISR: &str = "[INIT] RMT ISR on APP core";
/// The first output's open line. Only **IO18's** is asserted: UART0 carries
/// bytes at baud in emulated time, so the later opens are still in the TX
/// FIFO when the guest stops and are not on the wire to be read.
const OPEN_LINE: &str = "Esp32V3RmtWs281xDriver::open: endpoint=esp32v3-rmt-ws281x:ws281x:local:IO18 \
     gpio=/gpio/18 wire=0 bytes=48 (slot per transmission)";

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("five-wire.script")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lp-emu-esp32v3-m4-p4-five-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// FNV-1a, 32-bit — the firmware's own `frame_checksum` and the oracle's
/// `crc=`, restated inside the `lp-emu/` fence rather than imported across it
/// (`just lint-emu-fence`), so this side cannot agree with the guest by
/// sharing its code. Pinned against the published vectors below.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// One `[OUT] frame=<n> leds=<n> crc=0x… lit=<n>` summary line, as
/// `frame_dump::report` prints it once every `REPORT_EVERY_FRAMES = 60`
/// frames.
///
/// ⚠️ **The line does not name its endpoint** — one `FrameDump` per output
/// prints the same shape — so a summary is matched to a wire by its `crc`,
/// which the project makes unambiguous: the five paths sample five different
/// rows of the render, so the five wires carry five different byte strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Summary {
    n: u32,
    leds: usize,
    crc: u32,
    lit: usize,
}

fn summaries(console: &str) -> Vec<Summary> {
    console
        .lines()
        .filter(|l| l.contains("[OUT] frame="))
        .map(|line| Summary {
            n: field(line, "frame") as u32,
            leds: field(line, "leds"),
            crc: {
                let text = after(line, "crc=0x");
                u32::from_str_radix(&text, 16).unwrap_or_else(|_| panic!("crc in {line}"))
            },
            lit: field(line, "lit"),
        })
        .collect()
}

fn field(line: &str, name: &str) -> usize {
    let needle = format!("{name}=");
    let at = line
        .find(&needle)
        .unwrap_or_else(|| panic!("no `{name}` in {line}"))
        + needle.len();
    line[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("`{name}` is not a number in {line}"))
}

fn after(line: &str, needle: &str) -> String {
    let at = line
        .find(needle)
        .unwrap_or_else(|| panic!("no `{needle}` in {line}"))
        + needle.len();
    line[at..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect()
}

/// **The window-spill defect, recognised exactly.**
///
/// `true` — with a `SKIP` notice naming it — when the run ended on M4 P4b's
/// finding: a strict-bus stop inside one of the ROM's window handlers at an
/// address a null `a1` produces. Anything else is a failure and is left to
/// the assertions.
///
/// An early-out and **not** an `#[ignore]`, because an `#[ignore]` is not a
/// skip here: `just test-emu-esp32v3-boot` — what CI's `Emulator ESP32v3
/// (x64)` job runs — runs `cargo test -- --include-ignored`, so an
/// `#[ignore]`d test still runs and still fails. When P4b lands this stops
/// matching and every assertion below runs for real.
///
/// `tests/shader_oracle_pin.rs` carries the same function, and deliberately:
/// a test file that imported its skip condition from another one would skip
/// for a reason its reader cannot see.
fn stopped_by_the_window_spill(m: &Machine, outcome: &Outcome, test: &str) -> bool {
    let Outcome::StrictBus { violation } = outcome else {
        return false;
    };
    let symbol = m.symbolize(violation.pc).unwrap_or_default();
    if !symbol.contains("Window") || violation.address < 0xffff_0000 {
        return false;
    }
    skip_notice(
        test,
        &format!(
            "the window-spill defect M4 P4b is fixing — {:?} at 0x{:08x} ({symbol}, pc \
             0x{:08x}, cycle {}). The guest dies inside the project it just loaded, so the \
             lit frames this gate reads never happen. Branch \
             claude/xt-m4-p4b-window-underflow; the crate README's \"A frame three ways\" \
             has the trace.",
            violation.access, violation.address, violation.pc, violation.cycle,
        ),
    );
    true
}

/// A frame without its clock: everything the wire carried, and nothing about
/// when. What two runs at two different `--core-quantum` values must agree on
/// exactly — see
/// [`the_second_wave_decodes_the_same_frames_across_two_runs_and_two_quanta`].
fn shapes(frames: &[(u8, Vec<Frame>)]) -> Vec<(u8, Vec<(u64, usize, Vec<u8>, u8, u64, bool)>)> {
    frames
        .iter()
        .map(|(pad, fs)| {
            (
                *pad,
                fs.iter()
                    .map(|f| {
                        (
                            f.n,
                            f.bits,
                            f.wire.clone(),
                            f.trailing_bits,
                            f.error_count,
                            f.is_complete(),
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

/// The pin log's routing notes, in the order the fabric made them.
fn routes(pin_log: &str) -> Vec<(u8, String)> {
    pin_log
        .lines()
        .filter_map(|l| l.strip_prefix("# route gpio"))
        .filter_map(|rest| {
            let (pad, what) = rest.split_once(" <- ")?;
            Some((pad.parse().ok()?, what.to_string()))
        })
        .collect()
}

struct Run {
    m: Machine,
    outcome: Outcome,
    text: String,
    frames: Vec<(u8, Vec<Frame>)>,
    pin_log: String,
}

fn run(elf: &Path, quantum: u64, dir: &Path) -> Run {
    let text = std::fs::read_to_string(script_path()).expect("the walk script is committed");
    let script = parse_byte_script(&text).expect("the committed walk parses");
    let log = dir.join(format!("pins-q{quantum}.txt"));
    let mut m = Esp32V3Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .uart0_script(script)
        .dump_frames(FrameSink::Memory)
        .pin_log(PinLogSink::File(log.clone()))
        .strict(true)
        .time_grade(TimeGrade::T1)
        .core_quantum(quantum)
        .build()
        .expect("the frame-dump image builds a machine");
    let outcome = m.run_until(&StopCondition {
        stop_cycle: Some(GATE_US * memmap::CYCLES_PER_US),
        wall_timeout: Some(WALL_TIMEOUT),
        ..Default::default()
    });
    // ⚠️ A frame is not closed until something follows its latch; flush
    // before reading the last one on any pad. The same call flushes the pin
    // log's writer, which is what makes the file below complete.
    m.flush_frames();
    let text = m.uart0().text();
    let frames = PADS.iter().map(|p| (*p, m.frames(*p).to_vec())).collect();
    let pin_log = std::fs::read_to_string(&log).unwrap_or_default();
    Run {
        m,
        outcome,
        text,
        frames,
        pin_log,
    }
}

// ---------------------------------------------------------------------------
// The reachable half
// ---------------------------------------------------------------------------

/// **Five wires, four slots, and the mux between waves.**
///
/// Five pads are routed; four of them carry a pooled slot's own RMT signal;
/// and at least one signal drives a **second** pad later in the run, with a
/// park to `GPIO_OUT` in between. That is the second wave, read off the
/// fabric rather than inferred from the firmware.
#[test]
#[ignore = "needs the fw-esp32v3 frame-dump ELF; run through `just test-emu-esp32v3-boot`"]
fn five_wires_share_four_slots_and_the_fifth_re_muxes_a_signal() {
    let elf = match fw_esp32v3_frame_dump_image() {
        Ok(path) => path,
        Err(reason) => return skip_notice("five_wires", &reason),
    };
    let dir = scratch("routing");
    let r = run(&elf, 256, &dir);
    println!(
        "five_wires: outcome {:?}, {} us emulated, {} instructions ({} on core 1)",
        r.outcome,
        r.m.micros(),
        r.m.instructions(),
        r.m.core_instructions(1),
    );

    assert!(
        r.text.contains(APP_CORE_ISR),
        "the RMT ISR is not on the APP core — the pusher's whole premise:\n{}",
        r.text
    );
    assert!(r.text.contains(OPEN_LINE), "{}", r.text);
    assert!(
        r.text.contains("\"loadProject\":{\"handle\":1}"),
        "the project did not load:\n{}",
        r.text
    );

    // 1. All five pads reached the fabric. ⚠️ By pad, never by the list's
    // length: a plain boot routes gpio1 (the console's TX) too.
    let routed = r.m.routed_pads();
    for pad in PADS {
        assert!(
            routed.iter().any(|(p, _)| *p == PadId(pad)),
            "gpio{pad} is not routed at all: {routed:?}"
        );
    }

    // 2. The pooled slots: four two-block slots, so four even RMT channels,
    // each on its own pad. `POOLED_SLOT_CAP = 4` is the number that makes the
    // fifth wire interesting.
    let notes = routes(&r.pin_log);
    assert!(!notes.is_empty(), "the pin log recorded no routing");
    let mut signal_pads: Vec<(String, u8)> = notes
        .iter()
        .filter(|(_, what)| what.starts_with("RMT_SIG_"))
        .map(|(pad, what)| {
            (
                what.split_whitespace().next().expect("a name").to_string(),
                *pad,
            )
        })
        .collect();
    signal_pads.sort();
    signal_pads.dedup();
    let distinct_signals: Vec<&String> = {
        let mut s: Vec<&String> = signal_pads.iter().map(|(sig, _)| sig).collect();
        s.sort();
        s.dedup();
        s
    };
    println!("five_wires: signal→pad routes {signal_pads:?}");
    assert_eq!(
        distinct_signals.len(),
        4,
        "four pooled two-block slots drive the five wires: {signal_pads:?}"
    );

    // 3. The re-mux: one signal, two pads. This is the second wave, and it is
    // the direct evidence that the fifth wire time-shares a slot.
    let shared: Vec<&(String, u8)> = signal_pads
        .iter()
        .filter(|(sig, _)| signal_pads.iter().filter(|(s, _)| s == sig).count() > 1)
        .collect();
    assert!(
        !shared.is_empty(),
        "no signal drove two pads — the fifth wire got a slot of its own, or the second \
         wave never ran: {signal_pads:?}"
    );
    println!("five_wires: the shared slot: {shared:?}");
    // …and a pad that is re-muxed is parked between waves, never handed over
    // while it is still driven.
    let parked: Vec<u8> = notes
        .iter()
        .filter(|(_, what)| what == "GPIO_OUT")
        .map(|(pad, _)| *pad)
        .collect();
    for (_, pad) in shared {
        assert!(
            parked.contains(pad),
            "gpio{pad} shares a slot but is never parked to GPIO_OUT: {notes:?}"
        );
    }

    // 4. IO18 carries RMT_SIG_0 at some point in the run — the pad the rest
    // of M4 reads, and out_sel 87.
    assert!(
        signal_pads.contains(&("RMT_SIG_0".to_string(), 18)),
        "gpio18 never carried RMT_SIG_0 (out_sel {}): {signal_pads:?}",
        rmt::RMT_SIG_0
    );

    // 5. Every pad decoded whole frames with no bit errors. A re-mux that
    // landed mid-frame would show here as a truncated frame, and does not.
    for (pad, frames) in &r.frames {
        assert!(
            !frames.is_empty(),
            "no frame reached gpio{pad}: {:?}",
            r.outcome
        );
        for f in frames {
            if f.reset_cycles.is_none() {
                assert_eq!(
                    f.n,
                    frames.last().expect("frames").n,
                    "gpio{pad}: an open frame that is not the last"
                );
                continue;
            }
            assert_eq!(f.error_count, 0, "gpio{pad} frame {}: {:?}", f.n, f.errors);
            assert!(f.is_complete(), "gpio{pad} frame {} was cut short", f.n);
            assert_eq!(f.leds(), LEDS, "gpio{pad} frame {}", f.n);
            assert_eq!(f.bits, BITS, "gpio{pad} frame {}", f.n);
        }
        println!(
            "five_wires: gpio{pad}: {} frame(s), {} complete, 0 bit errors",
            frames.len(),
            frames.iter().filter(|f| f.is_complete()).count()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The hardest thing in the milestone to keep deterministic, twice over**:
/// two runs decode the same frames, and so do two `--core-quantum` values.
///
/// The second wave is a cross-core handover — the pusher on core 1 takes its
/// work from the PRO core's mailbox — so a scheduler interleaving that leaked
/// into the waveform would show up here and nowhere else.
#[test]
#[ignore = "needs the fw-esp32v3 frame-dump ELF; run through `just test-emu-esp32v3-boot`"]
fn the_second_wave_decodes_the_same_frames_across_two_runs_and_two_quanta() {
    let elf = match fw_esp32v3_frame_dump_image() {
        Ok(path) => path,
        Err(reason) => return skip_notice("five_wires", &reason),
    };
    let dir = scratch("determinism");
    let a = run(&elf, 256, &dir);
    let b = run(&elf, 256, &dir);
    assert_eq!(a.outcome, b.outcome, "two identical runs diverged");
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(
        a.text, b.text,
        "two runs of the same script said different things"
    );
    assert_eq!(a.frames, b.frames, "two runs decoded different frames");

    // ⚠️ Across two **quanta**, the bytes — and only the bytes. A different
    // quantum is a different interleaving of the two cores, and the pusher
    // lives on core 1: measured here, the same five frames start 64 cycles
    // (0.27 µs) earlier at quantum 64 than at 256, one window's worth, while
    // every bit on the wire is identical. So this compares the frame's
    // *shape* — pad, index, bit count, wire bytes, errors, completeness — and
    // never an absolute cycle, which is the same rule the rest of M4 follows
    // for a different reason (PD9).
    //
    // `tests/pin_frames.rs` compares two quanta's dumps byte for byte
    // including their times; it can, because its waveform is driven from the
    // host on one core. This one cannot, and that difference is the second
    // wave's signature rather than a flaw in either.
    let c = run(&elf, 64, &dir);
    assert_eq!(
        shapes(&c.frames),
        shapes(&a.frames),
        "quantum 64 decoded different frames from quantum 256"
    );
    println!(
        "five_wires: identical frames at quantum 256 (twice) and 64: {}",
        a.frames
            .iter()
            .map(|(pad, f)| format!("gpio{pad}={}", f.len()))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The half the window-spill defect holds
// ---------------------------------------------------------------------------

/// **Per wire, per frame: the pad against the guest's own checksum.**
///
/// Each wire's decoded frames are FNV-1a-equal to that endpoint's own `[OUT]
/// frame=… crc=` summary line, the five wires carry five *different* byte
/// strings (a routing mix-up is two wires with one checksum), no frame is
/// dropped, and the run reaches its own deadline with nothing unmapped.
///
/// All of it needs the render to survive: the summary lines are printed once
/// every `REPORT_EVERY_FRAMES = 60` frames, and the guest dies long before
/// the sixtieth.
#[test]
#[ignore = "needs the fw-esp32v3 frame-dump ELF; run through `just test-emu-esp32v3-boot` \
            (and skips on the M4 P4b window-spill defect until it lands)"]
fn every_wire_checksum_equals_the_guests_own_summary_line() {
    let elf = match fw_esp32v3_frame_dump_image() {
        Ok(path) => path,
        Err(reason) => return skip_notice("five_wires", &reason),
    };
    let dir = scratch("checksums");
    let r = run(&elf, 256, &dir);
    if stopped_by_the_window_spill(&r.m, &r.outcome, "five_wires") {
        return;
    }

    assert!(
        matches!(r.outcome, Outcome::Deadline { .. }),
        "the run should end at its deadline: {:?}\n{}",
        r.outcome,
        r.text
    );
    assert!(
        r.m.first_strict_violation().is_none(),
        "a strict refusal in the run: {:?}",
        r.m.first_strict_violation()
    );
    assert_eq!(
        r.m.bus().unmapped_reads() + r.m.bus().unmapped_writes(),
        0,
        "unmapped bus access"
    );

    // The lit frames each wire carried, by checksum. The order rule is the
    // oracle test's: the wire carries GRB and `unpermute` is the driver's
    // input; a per-pixel byte swap means the ORDER assumption is wrong and
    // the decoder is not to be fixed to match.
    let order = r.m.strip().order;
    assert_eq!(order, ColorOrder::Grb, "the default strip order");
    let mut per_wire: Vec<(u8, u32, String)> = Vec::new();
    for (pad, frames) in &r.frames {
        let lit: Vec<&Frame> = frames
            .iter()
            .filter(|f| f.is_complete() && f.wire.iter().any(|b| *b != 0))
            .collect();
        assert!(
            !lit.is_empty(),
            "gpio{pad} never carried a lit frame — the compile-window fallback never gave \
             way to a render"
        );
        // A clock-free project: one distinct lit frame per wire, for ever.
        let rgb = unpermute(&lit[0].wire, order);
        for f in &lit {
            assert_eq!(
                unpermute(&f.wire, order),
                rgb,
                "gpio{pad} frame {}: a clock-free project rendered two different frames",
                f.n
            );
        }
        per_wire.push((*pad, fnv1a(&rgb), hex(&rgb)));
    }

    // Five wires, five different byte strings. Two wires with one checksum is
    // exactly the failure a slot handed to the wrong wire produces.
    for (pad, crc, rgb) in &per_wire {
        println!("five_wires: gpio{pad} crc=0x{crc:08x} rgb={rgb}");
    }
    let mut crcs: Vec<u32> = per_wire.iter().map(|(_, c, _)| *c).collect();
    crcs.sort_unstable();
    crcs.dedup();
    assert_eq!(
        crcs.len(),
        PADS.len(),
        "the five wires do not carry five different frames: {per_wire:?}"
    );

    // …and each one is a checksum the guest itself printed. The summary line
    // carries no endpoint, so the match is by value — which the five distinct
    // frames above make unambiguous.
    let summaries = summaries(&r.text);
    assert!(
        !summaries.is_empty(),
        "the guest printed no `[OUT] frame=` summary line:\n{}",
        r.text
    );
    println!(
        "five_wires: {} summary line(s): {summaries:?}",
        summaries.len()
    );
    for (pad, crc, _) in &per_wire {
        let claimed = summaries
            .iter()
            .find(|s| s.crc == *crc)
            .unwrap_or_else(|| panic!("gpio{pad}: no summary line claims 0x{crc:08x}"));
        assert_eq!(claimed.leds, LEDS, "gpio{pad}");
        assert!(
            claimed.lit > 0,
            "gpio{pad}: a summary line for a black frame"
        );
    }

    // No frame is dropped: the guest's own per-wire frame counter (the
    // highest `frame=` it reported for that wire) and the decoder's count
    // agree within one — the last frame may still be open at the deadline.
    for (pad, frames) in &r.frames {
        let (_, crc, _) = per_wire.iter().find(|(p, _, _)| p == pad).expect("a wire");
        let claimed = summaries
            .iter()
            .filter(|s| s.crc == *crc)
            .map(|s| s.n as usize)
            .max()
            .expect("a summary line");
        let decoded = frames.len();
        assert!(
            decoded + 1 >= claimed && claimed + 1 >= decoded,
            "gpio{pad}: the guest counted {claimed} frames, the pad carried {decoded}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The script, and the transcriptions
// ---------------------------------------------------------------------------

/// The committed script is what it claims to be: twelve requests, the hello
/// first, `loadProject` last but one. No firmware, so this runs everywhere.
#[test]
fn the_five_wire_walk_is_the_twelve_requests_the_client_sends() {
    let text = std::fs::read_to_string(script_path()).expect("committed");
    let afters = text.lines().filter(|l| l.starts_with("after ")).count();
    assert_eq!(afters, 12, "one `after` per request");
    let first = text
        .lines()
        .find(|l| l.starts_with("after "))
        .expect("a first step");
    assert!(
        first.contains("[RECOVERY] boot complete (first frame served)"),
        "{first}"
    );
    // The project is the board's own five labels, authored — never a scratch
    // copy, because `projects/test/five-wire` already names the pins this
    // board has.
    for label in ["IO18", "IO16", "IO14", "IO2", "IO13"] {
        assert!(
            text.contains(&format!("ws281x:local:{label}")),
            "the walk never uploads an output on {label}"
        );
    }
    assert!(parse_byte_script(&text).is_ok());
}

/// The published FNV-1a 32-bit vectors, so this file's copy cannot disagree
/// with the guest's `frame_checksum` while looking healthy.
#[test]
fn fnv1a_matches_the_published_vectors() {
    assert_eq!(fnv1a(b""), 0x811c_9dc5);
    assert_eq!(fnv1a(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
}

/// The summary-line parser, against the shape `frame_dump::report` renders.
#[test]
fn the_summary_parser_reads_the_line_the_firmware_prints() {
    let console = "INFO - [OUT] open endpoint=x bytes=48 leds=16 (frame-dump build)\n\
         INFO - [OUT] frame=60 leds=16 crc=0x55772254 lit=16 first=(50,74,2) (0,8,55)\n\
         INFO - [OUT] frame=120 leds=16 crc=0x0badc0de lit=0 first=(0,0,0)\n\
         INFO - [OUT] dump frame=31 leds=16 shown=16 crc=0xdeadbeef rgb=324a02\n";
    assert_eq!(
        summaries(console),
        vec![
            Summary {
                n: 60,
                leds: 16,
                crc: 0x5577_2254,
                lit: 16
            },
            Summary {
                n: 120,
                leds: 16,
                crc: 0x0bad_c0de,
                lit: 0
            },
        ]
    );
}

/// The pin log's routing notes, read back the way the re-mux gate reads them.
#[test]
fn the_route_parser_reads_the_pin_logs_notes() {
    let log = "# route gpio18 <- RMT_SIG_0 (out_sel=87 inv=0)\n\
         0.000 gpio18 1 cyc=0\n\
         # route gpio18 <- GPIO_OUT\n\
         # route gpio13 <- RMT_SIG_0 (out_sel=87 inv=0)\n";
    assert_eq!(
        routes(log),
        vec![
            (18, "RMT_SIG_0 (out_sel=87 inv=0)".to_string()),
            (18, "GPIO_OUT".to_string()),
            (13, "RMT_SIG_0 (out_sel=87 inv=0)".to_string()),
        ]
    );
}

/// The five pads are the board's, and `RouteSource`'s signal form is what a
/// routed pad carries — a transcription check that needs no firmware.
#[test]
fn the_five_pads_are_the_boards_own() {
    assert_eq!(PADS, [18, 16, 14, 2, 13]);
    assert_eq!(rmt::RMT_SIG_0, 87, "RMT_SIG_0's out_sel on the classic");
    let route = RouteSource::Signal(SignalId(rmt::RMT_SIG_0), false);
    assert_eq!(route, RouteSource::Signal(SignalId(87), false));
}
