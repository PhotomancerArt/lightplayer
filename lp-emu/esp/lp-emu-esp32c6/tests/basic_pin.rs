//! M5 P4's second gate (G4-2): `examples/basic` on the pad — structure and
//! determinism, never content.
//!
//! `walks/examples-basic.script` is M4's walk: `lp-cli upload examples/basic`
//! as it stood at `d6cfaa205`, replayed here over the USB link on the shipped
//! reference image of that commit (`upload_walk_usb.rs`'s pair). The project
//! is 241 LEDs on D10 = gpio18, with interpolation and the LUT **on** — it is
//! clock-driven, so frame N under an emulated clock is not frame N on
//! silicon and its bytes are not compared with anything (M5 discovery §7,
//! DD34 Q3). What the pad can still be held to is the shape of the wire:
//! every frame 241 pixels of 24 bits, no bad pulse, latched by a reset at
//! least the driver's 300 µs long, a frame period that does not wander, and
//! two scripted runs writing byte-identical `--dump-frames` files.
//!
//! This is also the **shipped two-channel plan** (`ch0_half_words=24`), the
//! one the chase harness does not exercise, under a real project.
//!
//! `#[ignore]`d for the usual reason (`test_support`); `just test-emu-c6`
//! runs it.

use std::path::{Path, PathBuf};

use lp_emu_esp_common::strip::ws281x::Frame;
use lp_emu_esp32c6::control::parse_byte_script;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{
    AppSource, Esp32C6Builder, Esp32C6Machine, FrameSink, Outcome, StopCondition, TimeGrade,
    UsbHost, hex,
};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{ReferenceImage, reference_image, skip_notice};
use sha2::{Digest, Sha256};

const PAD: u8 = 18;
const LEDS: usize = 241;
const BITS: usize = LEDS * 24;
/// The driver's latch (`ChannelTiming::WS2812.latch_us`), which is the least
/// a reset between two of its frames can be.
const LATCH_US: u64 = 300;
/// The walk lands the load at about 2.9 s of guest time (M5 P3 paced the
/// script at 20 ms a chunk); six seconds holds well over a hundred frames
/// past it at the project's 61 fps.
const GATE_US: u64 = 6_000_000;
const OPEN_LINE: &str = "Esp32C6RmtWs281xDriver::open: endpoint=esp32c6-rmt-ws281x:ws281x:local:D10 \
     gpio=/gpio/18 ws281x_ch=0 rmt_slot=0 bytes=723";

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("examples-basic.script")
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("lp-emu-m5-p4-basic-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
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
        .expect("the reference image builds a machine");
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

fn sha256_of(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("the dump file");
    hex(&Sha256::digest(&bytes))
}

/// G4-2 on one time grade: the shape of the wire, and nothing about its
/// colours.
fn the_wire_has_the_projects_shape(grade: TimeGrade) {
    let elf = match reference_image(&ReferenceImage::SHIPPED_USB) {
        Ok(path) => path,
        Err(reason) => return skip_notice("basic_pin", &reason),
    };
    let dir = scratch(&format!("{grade:?}"));
    let r = run(&elf, grade, &dir);

    assert!(
        matches!(r.outcome, Outcome::Deadline { .. }),
        "{:?}\n{}",
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
    assert!(r.text.contains(OPEN_LINE), "{}", r.text);
    assert!(
        r.text.contains("\"id\":12,\"seq\":2,"),
        "the walk never reached the end of projectRead:\n{}",
        r.text
    );

    let frames = &r.frames;
    assert!(frames.len() >= 20, "only {} frames on gpio18", frames.len());
    // The last frame is the deadline's, not the driver's: `flush_frames`
    // closes a frame whose low has already run past the decoder's 50 µs
    // reset, and the "reset" it records is the low up to the deadline —
    // which may be shorter than the 300 µs latch that had not finished
    // (the first run of this test found one at 259 µs). A frame cut before
    // even that stays open. Either way it is not evidence about the wire,
    // and it can only be the last one.
    let whole: Vec<&Frame> = frames[..frames.len() - 1]
        .iter()
        .filter(|f| f.reset_cycles.is_some())
        .collect();
    assert_eq!(
        whole.len(),
        frames.len() - 1,
        "a frame other than the last is open: {frames:?}"
    );
    let latch = LATCH_US * memmap::CYCLES_PER_US;
    for f in &whole {
        assert_eq!(f.leds(), LEDS, "frame {}", f.n);
        assert_eq!(f.bits, BITS, "frame {}", f.n);
        assert_eq!(f.trailing_bits, 0, "frame {}", f.n);
        assert_eq!(f.error_count, 0, "frame {}: {:?}", f.n, f.errors);
        assert!(f.is_complete(), "frame {}: {f:?}", f.n);
        let reset = f.reset_cycles.expect("whole");
        assert!(
            reset >= latch,
            "frame {}: a {} us reset is shorter than the driver's {LATCH_US} us latch",
            f.n,
            reset / memmap::CYCLES_PER_US
        );
    }

    // A stable period: the engine paces the project at its frame rate, so
    // consecutive frame starts are one frame time apart and do not wander
    // from the median — except where the server is head-down and rendering
    // stops: the shader compile behind frame 0 (the compile-window black
    // frame; ADR 2026-08-03-memory-pressure-at-compile-safe-points) and the
    // `projectRead` answer streaming after it. Those are stalls, not jitter,
    // and there are a handful of them; every other period sits inside ±25 %
    // of the median. The band is wide on purpose — this is a claim about
    // shape, not about milliseconds (D13/PD9) — and every outlier is named.
    let mut periods: Vec<u64> = whole.windows(2).map(|w| w[1].start - w[0].start).collect();
    let median = {
        let mut sorted = periods.clone();
        sorted.sort_unstable();
        sorted[sorted.len() / 2]
    };
    let us = |c: u64| c as f64 / memmap::CYCLES_PER_US as f64;
    let outliers: Vec<String> = periods
        .iter()
        .enumerate()
        .filter(|(_, p)| (**p as f64) < 0.75 * median as f64 || (**p as f64) > 1.25 * median as f64)
        .map(|(i, p)| format!("{}→{}: {:.1} us", whole[i].n, whole[i + 1].n, us(*p)))
        .collect();
    println!(
        "basic_pin[{grade:?}]: {} period outliers against a median of {:.1} us: [{}]",
        outliers.len(),
        us(median),
        outliers.join(", ")
    );
    assert!(
        outliers.len() <= 4,
        "{} periods outside ±25 % of the median — more than the compile and the read \
         stream account for: [{}]",
        outliers.len(),
        outliers.join(", ")
    );
    periods.sort_unstable();
    let reset_min = whole
        .iter()
        .filter_map(|f| f.reset_cycles)
        .min()
        .unwrap_or(0);
    println!(
        "basic_pin[{grade:?}]: {} frames on gpio18 ({} whole), {LEDS} leds each, 0 errors; \
         period min {:.1} / median {:.1} / max {:.1} us; reset min {:.1} us; first frame at \
         {:.3} ms; {} us emulated, {} instructions",
        frames.len(),
        whole.len(),
        us(periods[0]),
        us(median),
        us(periods[periods.len() - 1]),
        us(reset_min),
        us(whole[0].start) / 1_000.0,
        r.m.micros(),
        r.m.instructions(),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-c6`"]
fn examples_basic_puts_whole_latched_241_led_frames_on_gpio18_under_t1() {
    the_wire_has_the_projects_shape(TimeGrade::T1);
}

#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-c6`"]
fn examples_basic_puts_whole_latched_241_led_frames_on_gpio18_under_t2() {
    the_wire_has_the_projects_shape(TimeGrade::T2);
}

/// Two scripted runs write byte-identical `--dump-frames` files: the
/// clock-driven frames are the machine's own clock's, and that clock is the
/// scheduler's.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-c6`"]
fn two_scripted_walks_dump_identical_frames() {
    let elf = match reference_image(&ReferenceImage::SHIPPED_USB) {
        Ok(path) => path,
        Err(reason) => return skip_notice("basic_pin", &reason),
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
        "basic_pin: two runs, {} frames each, dump sha256 {sa}",
        a.frames.len()
    );
    assert_eq!(sa, sb, "the two dump files differ");
    let _ = std::fs::remove_dir_all(&da);
    let _ = std::fs::remove_dir_all(&db);
}
