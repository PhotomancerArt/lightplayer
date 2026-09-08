//! M5 P4's pin claim on the **shipped** image (G4-1): the first lit frame
//! the product firmware puts on gpio18 is the host oracle's frame.
//!
//! `projects/test/shader-oracle` renders the same 64 pixels every frame — no
//! clock, no interpolation, no dithering, no LUT — and
//! `lp-app/lpa-server/tests/shader_oracle_frame.rs` prints the bytes the
//! device's WS281x driver must receive, from two host engines that share
//! nothing below the project file (wasmtime, and `lpvm-native`'s rv32
//! `rt_emu`). `scripts/m4-hardware-walk.sh` compares an S3's frame-dump line
//! against those; the C6 has no frame-dump line, so here the frame is read
//! back off the **pad** by the decoder instead, and the walk is
//! `walks/shader-oracle.script` — `lp-cli upload projects/test/shader-oracle`
//! captured live and replayed in guest time over the USB link the product
//! ships with (`upload_walk_usb.rs`'s shape).
//!
//! The frame to compare is the **first lit** one. The frames before it are
//! the compile-window black fallback (ADR
//! `2026-08-03-memory-pressure-at-compile-safe-points`: the shader compiles
//! one frame after the load, and that frame samples black), and comparing an
//! open-time black frame to a lit oracle is the walk's own documented trap.
//! Every frame after it must be the same bytes, because the project is
//! clock-free — that is what makes a byte difference a finding and not a
//! phase.
//!
//! What is compared, exactly: the oracle's `rgb=` is the frame **as the
//! driver's `write(data)` received it**, RGB order; the shipped C6 driver
//! opens every channel WS2812-class (GRB) and `lp-ws281x` permutes at encode
//! time, so the wire carries GRB and `unpermute(wire, Grb)` is the driver's
//! input. If that ever came out as a per-pixel byte swap of the oracle the
//! order assumption would be wrong and this test would say so — the decoder
//! is not to be "fixed" to match (the brief's rule; the harness's own double
//! swap in `LedChannel` is the standing example, and it is not the shipped
//! path).
//!
//! The oracle constants below are pinned from one run and carry the command
//! that produces them; a difference from `[ORACLE]` alone is the walk's
//! TRIAGE case (native codegen vs wasmtime — a compiler finding), a
//! difference from both is a machine finding.
//!
//! `#[ignore]`d for the usual reason (`test_support`); `just test-emu-c6`
//! runs it.

use std::path::{Path, PathBuf};

use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::{Frame, unpermute};
use lp_emu_esp32c6::control::parse_byte_script;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, FrameSink, Outcome, StopCondition, TimeGrade,
    UsbHost, hex,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image, skip_notice};
use lp_ws281x::ColorOrder;
use sha2::{Digest, Sha256};

/// `cargo test -p lpa-server --test shader_oracle_frame -- --nocapture`,
/// run in this worktree at `7e043ae2d` after `just ci-prereqs`, 2026-09-07:
///
/// ```text
/// [ORACLE] leds=64 shown=64 crc=0x55772254 lit=64
/// [ORACLE] rgb=324a02…4c2d05
/// [ORACLE-RV32] leds=64 shown=64 crc=0x55772254 lit=64
/// [ORACLE-RV32] rgb=324a02…4c2d05
/// [ORACLE-DIFF] wasmtime vs rv32-emu: 0 differing bytes of 192
/// ```
///
/// The two engines agreed on every byte, so one constant stands for both;
/// `ORACLE_CRC` is the oracle's own FNV-1a over the same 192 bytes.
const ORACLE_RGB: &str = "324a0208376a1c2889007668098b4b0375544602631253162b0f7051068a838b000097890b63b208a1601b30951b1c72660069af481900a49554e3212b48e41955cdad4d154f047b103e90441ec10ed47200bcb627f657019fb523c13e3794161c952e04a8743b36e681e90e225ef47f09d1174ebc035c8009447f3fb11b6112ca048dc419dd5fae02903ab21f015c6f026047006a750aa45b69b20834c32b7e8f0012913c086a360144365600567c064d430e9127239632148702475f4c2d05";
const ORACLE_CRC: u32 = 0x5577_2254;

const PAD: u8 = 18;
const LEDS: usize = 64;
const BITS: usize = LEDS * 24;
/// `RMT_SIG_0` (`regs::output_signals`).
const RMT_SIG_0: u16 = 71;
/// The walk lands the load a little over a second in; three seconds of
/// guest time holds hundreds of frames past it.
const GATE_US: u64 = 3_000_000;
/// The last frame of the `projectRead` answer: the walk's own end (request
/// 11 — the oracle project has no `clock.json`, so one file fewer than
/// `basic`'s twelve).
const READ_END: &str = "\"id\":11,\"seq\":2,";
/// The driver's open line for the oracle project: 64 LEDs on D10 = gpio18.
const OPEN_LINE: &str = "Esp32C6RmtWs281xDriver::open: endpoint=esp32c6-rmt-ws281x:ws281x:local:D10 \
     gpio=/gpio/18 ws281x_ch=0 rmt_slot=0 bytes=192";

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("shader-oracle.script")
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("lp-emu-m5-p4-oracle-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// FNV-1a, 32-bit — the oracle's `crc=` (`shader_oracle_frame::fnv1a`),
/// restated inside the fence.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

struct Run {
    m: Esp32C6Machine,
    outcome: Outcome,
    text: String,
    frames: Vec<Frame>,
    dump: PathBuf,
}

fn run(elf: &Path, grade: TimeGrade, dir: &Path) -> Run {
    let text = std::fs::read_to_string(script_path()).expect("the walk script is committed");
    let script = parse_byte_script(&text).expect("the committed walk parses");
    let dump = dir.join(format!("frames-{grade:?}.jsonl"));
    let mut m = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .usb_host(UsbHost::Attached { draining: true })
        .usb_script_source(script)
        .flash(FlashBacking::Blank)
        .dump_frames(FrameSink::File(dump.clone()))
        .strict(true)
        .time_grade(grade)
        .build()
        .expect("the shipped image builds a machine");
    let outcome = m.run_until(&StopCondition::after_micros(GATE_US));
    m.flush_frames();
    let text = String::from_utf8_lossy(&m.usb_sj().bytes()).into_owned();
    let frames = m.frames(PAD).to_vec();
    Run {
        m,
        outcome,
        text,
        frames,
        dump,
    }
}

/// The index of the first decoded frame with any non-zero byte on the wire.
fn first_lit(frames: &[Frame]) -> Option<usize> {
    frames.iter().position(|f| f.wire.iter().any(|b| *b != 0))
}

fn sha256_of(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("the dump file");
    hex(&Sha256::digest(&bytes))
}

/// G4-1 on one time grade.
fn the_first_lit_frame_is_the_oracles(grade: TimeGrade) {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => return skip_notice("shader_oracle_pin", &reason),
    };
    let dir = scratch(&format!("{grade:?}"));
    let r = run(&elf, grade, &dir);

    assert!(
        matches!(r.outcome, Outcome::Deadline { .. }),
        "the run should end at its deadline: {:?}\n{}",
        r.outcome,
        r.text
    );
    assert_eq!(
        r.m.bus.unmapped_reads() + r.m.bus.unmapped_writes(),
        0,
        "unmapped"
    );
    assert!(
        !r.text.contains("dropping unparseable"),
        "the guest lost bytes:\n{}",
        r.text
    );
    assert!(
        r.text.contains("\"loadProject\":{\"handle\":1}"),
        "the project did not load:\n{}",
        r.text
    );
    assert!(r.text.contains(OPEN_LINE), "{}", r.text);
    assert!(
        r.text.contains(READ_END),
        "the walk never reached the end of projectRead:\n{}",
        r.text
    );

    // The pad is gpio18, and it carries RMT channel 0 — the shipped
    // manifest's D10, routed by esp-hal's `with_pin` at the driver's open.
    let routed = r.m.routed_pads();
    assert!(
        routed.contains(&(PadId(PAD), RouteSource::Signal(SignalId(RMT_SIG_0), false))),
        "gpio18 is not routed to RMT_SIG_0: {routed:?}"
    );
    assert_eq!(
        r.m.strip().order,
        ColorOrder::Grb,
        "the default strip order"
    );

    let frames = &r.frames;
    assert!(!frames.is_empty(), "no frame reached gpio18:\n{}", r.text);
    let lit = first_lit(frames).unwrap_or_else(|| {
        panic!(
            "{} frames on gpio18 and every one of them black — the compile-window \
             fallback never gave way to a render",
            frames.len()
        )
    });
    let dark: Vec<String> = frames[..lit]
        .iter()
        .map(|f| format!("n={} bits={} complete={}", f.n, f.bits, f.is_complete()))
        .collect();
    println!(
        "shader_oracle_pin[{grade:?}]: {} frames on gpio18, first lit is n={} at {:.3} ms; \
         {} dark frame(s) before it: [{}]",
        frames.len(),
        frames[lit].n,
        frames[lit].start as f64 / memmap::CYCLES_PER_US as f64 / 1_000.0,
        lit,
        dark.join(", ")
    );

    // The first lit frame: whole, clean, latched — and the oracle's bytes.
    let f = &frames[lit];
    assert_eq!(f.leds(), LEDS, "frame {}", f.n);
    assert_eq!(f.bits, BITS, "frame {}", f.n);
    assert_eq!(f.error_count, 0, "frame {}: {:?}", f.n, f.errors);
    assert!(f.is_complete(), "frame {} was cut short: {f:?}", f.n);
    let rgb = unpermute(&f.wire, ColorOrder::Grb);
    let decoded = hex(&rgb);
    println!("shader_oracle_pin[{grade:?}]: decoded rgb={decoded}");
    println!("shader_oracle_pin[{grade:?}]: [ORACLE]    rgb={ORACLE_RGB}");
    println!(
        "shader_oracle_pin[{grade:?}]: wire (as carried, GRB) ={}",
        hex(&f.wire)
    );
    assert_eq!(
        decoded,
        ORACLE_RGB,
        "frame {}: the first lit frame off gpio18 is not the oracle's frame \
         (wire as carried: {}) — a per-pixel byte swap means the order assumption is \
         wrong, anything else is a compiler or machine finding; see the module docs",
        f.n,
        hex(&f.wire)
    );
    assert_eq!(
        fnv1a(&rgb),
        ORACLE_CRC,
        "frame {}: the decoded frame's FNV-1a is not the oracle's crc",
        f.n
    );

    // Every later frame is the same frame. A frame the deadline cut mid-way
    // is not evidence either way, and it can only be the last one.
    let mut later = 0;
    for g in &frames[lit + 1..] {
        if g.reset_cycles.is_none() {
            assert_eq!(
                g.n,
                frames.last().unwrap().n,
                "an open frame that is not the last"
            );
            continue;
        }
        assert_eq!(g.error_count, 0, "frame {}: {:?}", g.n, g.errors);
        assert!(g.is_complete(), "frame {} was cut short: {g:?}", g.n);
        assert_eq!(
            g.wire, f.wire,
            "frame {} differs from the first lit frame {}",
            g.n, f.n
        );
        later += 1;
    }
    assert!(later >= 10, "only {later} frames after the first lit one");

    // Reported, never gated (D13/PD9): the frame period and the reset gap.
    let lit_frames: Vec<&Frame> = frames[lit..]
        .iter()
        .filter(|g| g.reset_cycles.is_some())
        .collect();
    let mut periods: Vec<u64> = lit_frames
        .windows(2)
        .map(|w| w[1].start - w[0].start)
        .collect();
    periods.sort_unstable();
    let us = |c: u64| c as f64 / memmap::CYCLES_PER_US as f64;
    let resets = lit_frames.iter().filter_map(|g| g.reset_cycles);
    println!(
        "shader_oracle_pin[{grade:?}]: {} lit frames after n={}, all equal to it; frame \
         period min {:.1} / median {:.1} / max {:.1} us; reset min {:.1} us; {} us emulated, \
         {} instructions",
        later,
        f.n,
        us(periods.first().copied().unwrap_or(0)),
        us(periods.get(periods.len() / 2).copied().unwrap_or(0)),
        us(periods.last().copied().unwrap_or(0)),
        us(resets.min().unwrap_or(0)),
        r.m.micros(),
        r.m.instructions(),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs the shipped fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_first_lit_frame_off_gpio18_is_the_host_oracles_frame_under_t1() {
    the_first_lit_frame_is_the_oracles(TimeGrade::T1);
}

#[test]
#[ignore = "needs the shipped fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn the_first_lit_frame_off_gpio18_is_the_host_oracles_frame_under_t2() {
    the_first_lit_frame_is_the_oracles(TimeGrade::T2);
}

/// Two scripted runs write byte-identical `--dump-frames` files (M4's
/// determinism claim, on the pad).
#[test]
#[ignore = "needs the shipped fw-esp32c6 ELF; run through `just test-emu-c6`"]
fn two_scripted_walks_dump_identical_frames() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => return skip_notice("shader_oracle_pin", &reason),
    };
    let (da, db) = (scratch("a"), scratch("b"));
    let a = run(&elf, TimeGrade::T1, &da);
    let b = run(&elf, TimeGrade::T1, &db);
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.m.cycles(), b.m.cycles());
    assert_eq!(a.m.instructions(), b.m.instructions());
    assert_eq!(a.text, b.text, "two runs of the same script diverged");
    assert_eq!(a.frames.len(), b.frames.len());
    let (sa, sb) = (sha256_of(&a.dump), sha256_of(&b.dump));
    println!(
        "shader_oracle_pin: two runs, {} frames each, dump sha256 {sa}",
        a.frames.len()
    );
    assert_eq!(sa, sb, "the two dump files differ");
    let _ = std::fs::remove_dir_all(&da);
    let _ = std::fs::remove_dir_all(&db);
}

/// The committed script is what it claims to be: twelve requests, the
/// hello first, `projectRead` last and waiting on the compile line — the
/// same check `upload_walk.rs` makes of `examples-basic.script`. No firmware.
#[test]
fn the_walk_script_is_the_twelve_frames_the_client_sends() {
    let text = std::fs::read_to_string(script_path()).expect("committed");
    let afters = text.lines().filter(|l| l.starts_with("after ")).count();
    assert_eq!(afters, 12, "one `after` per request");
    for id in [
        "18446744073709551615",
        "1",
        "2",
        "3",
        "4",
        "5",
        "6",
        "7",
        "8",
        "9",
        "10",
        "11",
    ] {
        assert!(
            text.contains(&format!("M!{{\\\"id\\\":{id},")),
            "request {id} is missing from the script"
        );
    }
    let first = text
        .lines()
        .find(|l| l.starts_with("after "))
        .expect("a first step");
    assert!(
        first.contains("[RECOVERY] boot complete (first frame served)"),
        "{first}"
    );
    let read = text
        .lines()
        .find(|l| l.contains("\\\"projectRead\\\""))
        .expect("the projectRead request");
    assert!(
        read.starts_with("after \"[shader-node] compilation succeeded\""),
        "projectRead must wait for the compile, not for loadProject's acknowledgement: {read}"
    );
    assert!(parse_byte_script(&text).is_ok());
}
