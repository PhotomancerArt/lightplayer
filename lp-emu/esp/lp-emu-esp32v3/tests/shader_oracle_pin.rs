//! **M4 P4's gate**: the first lit frame the product firmware puts on the
//! classic's IO18 is the host oracle's frame — a frame read three ways.
//!
//! `projects/test/shader-oracle` renders the same 64 pixels every frame — no
//! clock, no interpolation, no dithering, no LUT — and
//! `lp-app/lpa-server/tests/shader_oracle_frame.rs` prints the bytes the
//! device's WS281x driver must receive, from two host engines that share
//! nothing below the project file (wasmtime, and `lpvm-native`'s rv32
//! `rt_emu`). The walk is `walks/shader-oracle.script` — `lp-cli upload
//! projects/test/shader-oracle` captured live and replayed in guest time over
//! **UART0**, the only link this chip has.
//!
//! # The three readings
//!
//! | reading | source |
//! |---|---|
//! | (a) the firmware's own | `[OUT] dump frame=… rgb=…` on the UART0 console — RGB, **as the driver's `write(data)` received it** |
//! | (b) off the pad | `frames(18)` → `unpermute(&f.wire, ColorOrder::Grb)` — the wire carries GRB; the driver's input is RGB |
//! | (c) the host oracle | [`ORACLE_RGB`] / [`ORACLE_CRC`], pinned from a run in this worktree |
//!
//! All three must be the same 384 hex characters. A difference from
//! `[ORACLE]` alone is the walk's TRIAGE case (native codegen vs wasmtime — a
//! compiler finding); a difference from both is a machine finding.
//!
//! # The three subtleties, each of which will bite
//!
//! **(i) The dumped frame is not the first lit frame.**
//! `frame_dump.rs`'s `LIT_DUMP_DELAY_FRAMES = 30`: the first lit frame *arms*
//! the full dump and it fires **30 frames later**, because the instant the
//! shader finishes compiling is also the instant the UART writer queue floods
//! (the PR #300 interleaving defect). On a clock-free project those are the
//! same bytes — which is exactly why `projects/test/shader-oracle` is
//! clock-free — and the "every later frame is the same frame" assertion below
//! is what makes comparing reading (a) to reading (b) legitimate rather than
//! assumed.
//!
//! **(ii) The first *lit* frame, not the first frame.** The frames before it
//! are the compile-window black fallback (ADR
//! `2026-08-03-memory-pressure-at-compile-safe-points`). Comparing an
//! open-time black frame to a lit oracle is the walk's own documented trap.
//!
//! **(iii) The order rule.** The shipped classic driver opens every channel
//! WS2812-class (GRB) and `lp-ws281x` permutes at encode time, so the wire
//! carries GRB and `unpermute(wire, Grb)` is the driver's input. If that ever
//! came out as a **per-pixel byte swap** of the oracle the order assumption
//! would be wrong and this test would say so — **the decoder is not to be
//! "fixed" to match** (the harness's own double swap in `LedChannel` is the
//! standing example, and it is not the shipped path).
//!
//! # ⚠️ The channel is not fixed at open; the pad is
//!
//! The classic's driver binds a **wire** index, not a slot — its open line
//! ends *"(slot per transmission)"*, and `wire_pusher.rs` chooses an RMT slot
//! per transmission and routes the pad to it with a `func_out_sel_cfg` write.
//! So the channel a frame goes out on can change between frames, and a reader
//! who expects a fixed channel will misread a trace. The decoder is keyed on
//! the **pad**, which is why none of that matters here.
//!
//! # ⚠️ The window-spill defect, and why this file holds two tests
//!
//! Loading any project kills this guest: a level-1 interrupt's
//! `save_context` declares a frame spilled whose registers never reach
//! memory, the `retw` that follows takes `_WindowUnderflow8` and restores
//! `a1 = 0`, and the handler walks its spills down through unmapped memory.
//! It is `lp-emu/lp-xt-emu`'s window machinery (M1) and it is being fixed in
//! M4 **P4b**; the crate README's "A frame three ways" carries the trace.
//!
//! The gate is therefore **two** tests. Everything that can be read off the
//! pad is [`the_first_lit_frame_off_io18_is_the_host_oracles_frame`];
//! everything that needs the run to reach its own deadline — reading (a),
//! whose deferred dump is 30 frames past the first lit one, `unmapped == 0`,
//! and the two-run `--dump-frames` sha256 — is
//! [`the_firmwares_own_dump_is_the_same_frame_and_the_run_reaches_its_deadline`].
//! Neither is weakened; both stop at [`stopped_by_the_window_spill`], which
//! recognises that one stop exactly and prints a `SKIP` notice naming it.
//!
//! ⚠️ **On this tree neither has run green**, and not only because of where
//! the defect lands: at the committed script's 30 ms chunk gap the guest dies
//! *inside `loadProject`*, before any output opens, so no frame reaches any
//! pad at all. A 15 ms copy of the same script — scratch, never committed,
//! and the pacing the script's header says was measured — renders 56 whole
//! frames off IO18 with one distinct lit byte string, equal to [`ORACLE_RGB`]
//! with FNV-1a [`ORACLE_CRC`]. So the claim below holds and its reachability
//! is the defect's, not the walk's. The script is P4b's to commit
//! byte-identical, so it is not re-paced here.
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
    AppSource, Esp32V3Builder, FrameSink, Machine, Outcome, StopCondition, TimeGrade, hex,
};
use lp_emu_esp32v3::memmap;
use lp_emu_esp32v3::periph::rmt;
use lp_emu_esp32v3::test_support::{fw_esp32v3_frame_dump_image, skip_notice};
use lp_ws281x::ColorOrder;
use sha2::{Digest, Sha256};

/// `cargo test -p lpa-server --test shader_oracle_frame -- --nocapture`, run
/// **in this worktree** on branch `claude/xt-m4-p4-frame-three-ways` after
/// `just ci-prereqs`, 2026-09-11:
///
/// ```text
/// [ORACLE] leds=64 shown=64 crc=0x55772254 lit=64
/// [ORACLE] rgb=324a0208376a1c28…4c2d05
/// [ORACLE-RV32] leds=64 shown=64 crc=0x55772254 lit=64
/// [ORACLE-RV32] rgb=324a0208376a1c28…4c2d05
/// [ORACLE-DIFF] wasmtime vs rv32-emu: 0 differing bytes of 192
/// ```
///
/// The two engines agreed on every byte, so one constant stands for both.
/// These are the **classic's own** oracle numbers: the project and the
/// engines are the same as the C6's, so the 384 characters are the same 384
/// characters — but that was checked here rather than copied, because a
/// copied constant that happens to be right teaches nothing when it is
/// wrong.
const ORACLE_RGB: &str = "324a0208376a1c2889007668098b4b0375544602631253162b0f7051068a838b000097890b63b208a1601b30951b1c72660069af481900a49554e3212b48e41955cdad4d154f047b103e90441ec10ed47200bcb627f657019fb523c13e3794161c952e04a8743b36e681e90e225ef47f09d1174ebc035c8009447f3fb11b6112ca048dc419dd5fae02903ab21f015c6f026047006a750aa45b69b20834c32b7e8f0012913c086a360144365600567c064d430e9127239632148702475f4c2d05";
/// The oracle's own `crc=`, over the same 192 bytes.
const ORACLE_CRC: u32 = 0x5577_2254;

/// The DOM-Z-102's first fused DATA terminal — the walk's scratch copy
/// rewrites the project's `ws281x:local:D10` (the XIAO S3's pad, which this
/// board does not have) to `ws281x:local:IO18`.
const PAD: u8 = 18;
const LEDS: usize = 64;
const BITS: usize = LEDS * 24;

/// The walk's whole cost in emulated time. The upload lands its project a
/// few seconds in (twelve requests, 64 B every 30 ms of guest time) and the
/// shader then compiles incrementally over many render ticks; 30 s is room
/// for the deferred dump 30 frames past the first lit frame, and the run
/// ends earlier than that today on the window-spill defect.
const GATE_US: u64 = 30_000_000;

/// Host-side safety net, so a wedged run fails the suite instead of hanging
/// it.
const WALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(900);

/// The dual-core line. This test is also the standing guard that M4 P1 has
/// not regressed: the RMT ISR belongs to the APP core.
const APP_CORE_ISR: &str = "[INIT] RMT ISR on APP core";

/// The driver's open line, as `esp32v3_rmt_ws281x_driver.rs` prints it —
/// `wire=<n>` and "slot per transmission", because this chip binds a wire and
/// chooses the slot per frame.
const OPEN_LINE: &str = "Esp32V3RmtWs281xDriver::open: endpoint=esp32v3-rmt-ws281x:ws281x:local:IO18 \
     gpio=/gpio/18 wire=0 bytes=192 (slot per transmission)";
/// `frame_dump::log_open`'s companion — the readout's own announcement, and
/// the proof this is the `frame-dump` image and not the shipped one.
const OPEN_DUMP_LINE: &str = "[OUT] open endpoint=esp32v3-rmt-ws281x:ws281x:local:IO18 \
     bytes=192 leds=64 (frame-dump build)";

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("walks")
        .join("shader-oracle.script")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lp-emu-esp32v3-m4-p4-oracle-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// FNV-1a, 32-bit — the oracle's `crc=` (`shader_oracle_frame::fnv1a`) and
/// the firmware's own `frame_checksum`, restated inside the `lp-emu/` fence
/// rather than imported across it (`just lint-emu-fence`). Pinned against the
/// published vectors at the bottom of this file, because a gate that computed
/// a *different* function from the guest's would look healthy while comparing
/// nothing.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

struct Run {
    m: Machine,
    outcome: Outcome,
    text: String,
    frames: Vec<Frame>,
    dump: PathBuf,
}

/// One scripted walk on the `frame-dump` image, `t1`, strict.
fn run(elf: &Path, quantum: u64, dir: &Path) -> Run {
    let text = std::fs::read_to_string(script_path()).expect("the walk script is committed");
    let script = parse_byte_script(&text).expect("the committed walk parses");
    let dump = dir.join(format!("frames-q{quantum}.jsonl"));
    let mut m = Esp32V3Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .uart0_script(script)
        .dump_frames(FrameSink::File(dump.clone()))
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
    // ⚠️ A frame is not closed until something follows its latch: the decoder
    // reports an open frame *incomplete* rather than inventing a reset gap.
    m.flush_frames();
    let text = m.uart0().text();
    let frames = m.frames(PAD).to_vec();
    Run {
        m,
        outcome,
        text,
        frames,
        dump,
    }
}

/// **The window-spill defect, recognised exactly.**
///
/// `true` — with a `SKIP` notice naming it — when the run ended on M4 P4b's
/// finding: a strict-bus stop inside one of the ROM's window handlers at an
/// address a null `a1` produces. Anything else is a failure and is left to
/// the assertions.
///
/// This is an early-out and **not** an `#[ignore]`, because an `#[ignore]` is
/// not a skip here: `just test-emu-esp32v3-boot` — what CI's `Emulator
/// ESP32v3 (x64)` job runs — runs `cargo test -- --include-ignored`, so an
/// `#[ignore]`d test still runs and still fails. The attribute's job is to
/// keep a bare `cargo test` away from the firmware image; this is what keeps
/// a known, named, in-flight defect from turning the job red while the fix is
/// being written on another branch. When P4b lands this stops matching and
/// every assertion below runs for real.
///
/// `tests/five_wires.rs` carries the same function, and deliberately: a test
/// file that imported its skip condition from another one would skip for a
/// reason its reader cannot see.
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
             frames this gate reads never happen. Branch \
             claude/xt-m4-p4b-window-underflow; the crate README's \"A frame three ways\" \
             has the trace.",
            violation.access, violation.address, violation.pc, violation.cycle,
        ),
    );
    true
}

/// The index of the first decoded frame with any non-zero byte on the wire.
fn first_lit(frames: &[Frame]) -> Option<usize> {
    frames.iter().position(|f| f.wire.iter().any(|b| *b != 0))
}

fn sha256_of(path: &Path) -> String {
    let bytes = std::fs::read(path).expect("the dump file");
    hex(&Sha256::digest(&bytes))
}

/// The `rgb=` of the **last** `[OUT] dump frame=` line in a console — the
/// deferred lit dump, not the open-time black one.
fn last_dump_rgb(text: &str) -> Option<String> {
    text.lines()
        .filter(|l| l.contains("[OUT] dump frame="))
        .filter_map(|l| l.split("rgb=").nth(1))
        .map(|rest| rest.trim().to_string())
        .next_back()
}

// ---------------------------------------------------------------------------
// The reachable half
// ---------------------------------------------------------------------------

/// **Reading (b) is reading (c)**: the first lit frame off IO18 is the host
/// oracle's frame, byte for byte, and every later frame this run got to is
/// the same frame.
#[test]
#[ignore = "needs the fw-esp32v3 frame-dump ELF; run through `just test-emu-esp32v3-boot` \
            (and skips on the M4 P4b window-spill defect until it lands)"]
fn the_first_lit_frame_off_io18_is_the_host_oracles_frame() {
    let elf = match fw_esp32v3_frame_dump_image() {
        Ok(path) => path,
        Err(reason) => return skip_notice("shader_oracle_pin", &reason),
    };
    let dir = scratch("t1");
    let r = run(&elf, 256, &dir);
    // The shape of the run, before any assertion — so a failure below is read
    // beside what the machine actually did rather than instead of it.
    println!(
        "shader_oracle_pin: outcome {:?}, {} us emulated, {} instructions, {} frames on \
         gpio{PAD}, unmapped {}",
        r.outcome,
        r.m.micros(),
        r.m.instructions(),
        r.frames.len(),
        r.m.bus().unmapped_reads() + r.m.bus().unmapped_writes(),
    );
    if stopped_by_the_window_spill(&r.m, &r.outcome, "shader_oracle_pin") {
        return;
    }

    // 1. The guest heard the whole walk, and P1's dual core is still there.
    assert!(
        !r.text.contains("dropping unparseable"),
        "the guest lost bytes:\n{}",
        r.text
    );
    assert!(
        r.text.contains(APP_CORE_ISR),
        "the RMT ISR is not on the APP core — M4 P1 has regressed:\n{}",
        r.text
    );
    assert!(
        r.text.contains("\"loadProject\":{\"handle\":1}"),
        "the project did not load:\n{}",
        r.text
    );

    // 2. The output opened, on the pad the scratch copy retargeted it to.
    assert!(r.text.contains(OPEN_LINE), "{}", r.text);
    assert!(r.text.contains(OPEN_DUMP_LINE), "{}", r.text);

    // 3. ⚠️ By value, never by length: a plain boot of any classic image
    // routes gpio1 (the console's TX) too, so `routed_pads()` is never a
    // list of one.
    let routed = r.m.routed_pads();
    assert!(
        routed.contains(&(
            PadId(PAD),
            RouteSource::Signal(SignalId(rmt::RMT_SIG_0), false)
        )),
        "gpio{PAD} is not routed to RMT_SIG_0 (out_sel 87): {routed:?}"
    );
    assert_eq!(
        r.m.strip().order,
        ColorOrder::Grb,
        "the default strip order"
    );

    // 4. Frames, and a lit one.
    let frames = &r.frames;
    assert!(
        !frames.is_empty(),
        "no frame reached gpio{PAD}:\n{}",
        r.text
    );
    let lit = first_lit(frames).unwrap_or_else(|| {
        panic!(
            "{} frames on gpio{PAD} and every one of them black — the compile-window \
             fallback never gave way to a render, which is a different bug from \"no frame \
             reached the pad\"",
            frames.len()
        )
    });
    let dark: Vec<String> = frames[..lit]
        .iter()
        .map(|f| format!("n={} bits={} complete={}", f.n, f.bits, f.is_complete()))
        .collect();
    println!(
        "shader_oracle_pin: {} frames on gpio{PAD}, first lit is n={}; {} dark frame(s) \
         before it: [{}]; outcome {:?}",
        frames.len(),
        frames[lit].n,
        lit,
        dark.join(", "),
        r.outcome,
    );

    // 5. The first lit frame: whole, clean, latched.
    let f = &frames[lit];
    assert_eq!(f.leds(), LEDS, "frame {}", f.n);
    assert_eq!(f.bits, BITS, "frame {}", f.n);
    assert_eq!(f.error_count, 0, "frame {}: {:?}", f.n, f.errors);
    assert!(f.is_complete(), "frame {} was cut short: {f:?}", f.n);

    // 6. …and it is the oracle's bytes.
    let rgb = unpermute(&f.wire, ColorOrder::Grb);
    let decoded = hex(&rgb);
    println!("shader_oracle_pin: pad {PAD}   rgb={decoded}");
    println!("shader_oracle_pin: [ORACLE]  rgb={ORACLE_RGB}");
    println!(
        "shader_oracle_pin: wire (as carried, GRB) ={}",
        hex(&f.wire)
    );
    assert_eq!(
        decoded,
        ORACLE_RGB,
        "frame {}: the first lit frame off gpio{PAD} is not the oracle's frame (wire as \
         carried: {}) — a per-pixel byte swap means the ORDER assumption is wrong and the \
         decoder is not to be fixed to match; anything else is a compiler or machine \
         finding. See the module docs.",
        f.n,
        hex(&f.wire)
    );
    assert_eq!(
        fnv1a(&rgb),
        ORACLE_CRC,
        "frame {}: the decoded frame's FNV-1a is not the oracle's crc",
        f.n
    );

    // 7. Every later frame is the same frame. A frame the run's end cut
    // mid-way is not evidence either way, and it can only be the last one.
    let mut later = 0;
    for g in &frames[lit + 1..] {
        if g.reset_cycles.is_none() {
            assert_eq!(
                g.n,
                frames.last().expect("frames").n,
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
    println!(
        "shader_oracle_pin: {later} lit frame(s) after n={}, all equal to it; {} us \
         emulated, {} instructions",
        f.n,
        r.m.micros(),
        r.m.instructions()
    );
    assert!(
        later >= 10,
        "only {later} frames after the first lit one — too few for \"every later frame is \
         the same frame\" to mean anything"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// The half the window-spill defect holds
// ---------------------------------------------------------------------------

/// **Reading (a) is readings (b) and (c)**, and the run reaches its own
/// deadline with nothing unmapped and two byte-identical dumps.
///
/// Everything here needs the guest to survive past the first lit frame: the
/// firmware's own dump is *deferred* `LIT_DUMP_DELAY_FRAMES = 30` frames
/// (`frame_dump.rs`), and the guest dies about 22 lit frames in.
#[test]
#[ignore = "needs the fw-esp32v3 frame-dump ELF; run through `just test-emu-esp32v3-boot` \
            (and skips on the M4 P4b window-spill defect until it lands)"]
fn the_firmwares_own_dump_is_the_same_frame_and_the_run_reaches_its_deadline() {
    let elf = match fw_esp32v3_frame_dump_image() {
        Ok(path) => path,
        Err(reason) => return skip_notice("shader_oracle_pin", &reason),
    };
    let (da, db) = (scratch("a"), scratch("b"));
    let a = run(&elf, 256, &da);
    if stopped_by_the_window_spill(&a.m, &a.outcome, "shader_oracle_pin") {
        return;
    }

    assert!(
        matches!(a.outcome, Outcome::Deadline { .. }),
        "the run should end at its deadline: {:?}\n{}",
        a.outcome,
        a.text
    );
    assert!(
        a.m.first_strict_violation().is_none(),
        "a strict refusal in the run: {:?}",
        a.m.first_strict_violation()
    );
    assert_eq!(
        a.m.bus().unmapped_reads() + a.m.bus().unmapped_writes(),
        0,
        "unmapped bus access"
    );

    // Reading (a) against readings (b) and (c), in one assertion, with all
    // three printed on failure. The LAST dump line, not the first: the first
    // is the open-time black frame, and the deferred lit dump fires 30 frames
    // after the first lit one.
    let lit = first_lit(&a.frames).expect("a lit frame");
    let pad_rgb = hex(&unpermute(&a.frames[lit].wire, ColorOrder::Grb));
    let dumped = last_dump_rgb(&a.text).unwrap_or_else(|| {
        panic!(
            "the guest printed no `[OUT] dump frame=` line — this is the frame-dump image, \
             so nothing rendered:\n{}",
            a.text
        )
    });
    println!("shader_oracle_pin: [OUT] dump rgb={dumped}");
    assert_eq!(
        dumped, pad_rgb,
        "the firmware's own dump and the pad disagree — the RMT encode, the colour order or \
         the refill path.\n  [OUT] dump: {dumped}\n  pad {PAD}:     {pad_rgb}\n  [ORACLE]:   \
         {ORACLE_RGB}"
    );
    assert_eq!(
        dumped, ORACLE_RGB,
        "the firmware's own dump is not the oracle's frame.\n  [OUT] dump: {dumped}\n  \
         [ORACLE]:   {ORACLE_RGB}"
    );

    // Two runs write byte-identical `--dump-frames` files.
    let b = run(&elf, 256, &db);
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

// ---------------------------------------------------------------------------
// The script, and the transcription
// ---------------------------------------------------------------------------

/// The committed script is what it claims to be: twelve requests, the hello
/// first, `projectRead` last. No firmware, so this runs everywhere.
///
/// ⚠️ M4 **P4b** commits this same file byte-identical at the same path, so
/// the two branches merge clean. Regenerating it here would break that.
#[test]
fn the_walk_script_is_the_twelve_requests_the_client_sends() {
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
        read.starts_with("after \"\\\"id\\\":10,\""),
        "projectRead must wait for the loadProject ANSWER, not fire on a timer: {read}"
    );
    // The endpoint the walk uploads is the board's own, never the project's
    // `D10` — an endpoint this board does not have never opens.
    assert!(
        text.contains("ws281x:local:IO18"),
        "the walk must upload the IO18-retargeted scratch copy"
    );
    assert!(parse_byte_script(&text).is_ok());
}

/// The published FNV-1a 32-bit vectors. If this fails, the copy in this file
/// disagrees with the firmware's `frame_checksum` and the oracle's `crc=` —
/// and the gates above would be comparing against a different function while
/// looking healthy.
#[test]
fn fnv1a_matches_the_published_vectors() {
    assert_eq!(fnv1a(b""), 0x811c_9dc5);
    assert_eq!(fnv1a(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
}

/// The oracle constant is 64 LEDs of RGB and its own checksum — a
/// transcription check, so a truncated paste fails here rather than as a
/// frame mismatch.
#[test]
fn the_oracle_constant_is_sixty_four_pixels_and_its_own_crc() {
    assert_eq!(ORACLE_RGB.len(), LEDS * 3 * 2, "384 hex characters");
    let bytes: Vec<u8> = (0..ORACLE_RGB.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&ORACLE_RGB[i..i + 2], 16).expect("hex"))
        .collect();
    assert_eq!(fnv1a(&bytes), ORACLE_CRC);
}

/// The dump reader takes the LAST `[OUT] dump` line, which is the deferred
/// lit one — the first is the open-time black frame.
#[test]
fn the_dump_reader_takes_the_deferred_lit_dump_and_not_the_open_one() {
    let console = "INFO - [OUT] open endpoint=x bytes=192 leds=64 (frame-dump build)\n\
         INFO - [OUT] dump frame=1 leds=64 shown=64 crc=0x00000000 rgb=0000\n\
         INFO - [OUT] frame=60 leds=64 crc=0x55772254 lit=64 first=(50,74,2)\n\
         INFO - [OUT] dump frame=31 leds=64 shown=64 crc=0x55772254 rgb=324a02\n";
    assert_eq!(last_dump_rgb(console).as_deref(), Some("324a02"));
    assert_eq!(last_dump_rgb("nothing here\n"), None);
}
