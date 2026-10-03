//! The shipped classic (v3) image's chip gates that need a host on its link —
//! the host-side twins of `lp-emu-esp32v3`'s `boot_idle`, `uart_socket`,
//! `project_load_survives`, `shader_oracle_pin` and `five_wires` (plan
//! `lp2025/2026-09-28-2015-classic-uart-on-lp-link`, P5; the C6's move is
//! `tests/emu_usb_link_gates.rs`, the S3's `tests/emu_s3_link_gates.rs`).
//!
//! Since wire proto 32 the classic's UART0 speaks lp-link: past the boot
//! text, everything is frames, the board's log lines ride the link's log
//! channel, and its hello, replies and heartbeats go out only once a host has
//! brought the link up. The emulator's own tests stop at the boot text
//! because nothing under `lp-emu/` may host a link (the MIT fence); what they
//! used to read off the wire, and every claim that needed a request or an
//! upload on it, is read here, through lp-cli's in-process host
//! (`lp_cli::commands::emu::link_host`: the product's own
//! `lpc_wire::WireLinkPort` on `LinkConfig::uart()`, serviced between 250 us
//! slices of emulated time, with every wire message rendered back as its
//! `M!{json}` console line and every log record as `[LEVEL] target: text`).
//!
//! - **G2**, both boot paths: the idle heartbeat triple, elicited by a
//!   `stopAllProjects` over the link, with the hello and the reply — and the
//!   two paths report the same memory figures (was `boot_idle.rs`).
//! - **R6**: three requests after the boot settles, three answers in order
//!   (was `uart_socket.rs`).
//! - **The project load survives** and the board keeps rendering (was
//!   `project_load_survives.rs`).
//! - **A frame three ways** on the `frame-dump` image: the firmware's own
//!   `[OUT] dump` (delivered over the link), the pad and the host oracle
//!   (was `shader_oracle_pin.rs`).
//! - **Five wires over four slots**: routing, the per-wire checksums against
//!   the guest's own summary lines, and determinism across two runs and two
//!   quanta (was `five_wires.rs`).
//!
//! What changed with the move, for every test here: the conversation is the
//! product's own (a deploy over the link, requests sent once the hello has
//! arrived) rather than a committed `M!` byte script replayed on
//! `[INIT] I/O task spawned` or a boot line, so it lands earlier in the boot
//! and the runs are shorter. And the console is **no longer repaired**: the
//! old files carried a `deinterleave` that rejoined records another task's
//! records had been spliced into (the PR #300 interleaving defect,
//! `docs/defects/2026-08-02-serial-line-interleaving.md`). On the link every
//! record arrives whole, so these tests read the console as it came, and an
//! interleaved record would now fail them rather than be mended.
//!
//! `#[ignore]`d: they need `LP_EMU_ESP32V3_ELF`, `LP_EMU_ESP32V3_MERGED` and
//! `LP_EMU_ESP32V3_FRAME_DUMP_ELF`, which `just test-emu-esp32v3-boot` builds
//! and exports before it runs this. Numbers printed are `lp-emu:esp32v3:t1`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{EmuLinkHost, V3Board};
use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::{Frame, unpermute};
use lp_emu_esp_figures::Figures;
use lp_emu_esp32v3::flash::FlashBacking;
use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, FrameSink, PinLogSink, TimeGrade, hex,
};
use lp_emu_esp32v3::periph::rmt;
use lp_emu_esp32v3::test_support;
use lp_ws281x::ColorOrder;
use lpc_wire::{ClientMessage, ClientRequest};
use sha2::{Digest, Sha256};

/// One fixed host nonce, so a run is a function of the image and the
/// conversation.
const NONCE: u32 = 0x5E55_0333;

/// Emulated microseconds the hello may take: the boot reaches the server loop
/// at ~0.12 s on a direct load and ~0.38 s ROM-up.
const HELLO_BUDGET_US: u64 = 3_000_000;

/// The dual-core line — M4 P1's standing guard: the RMT ISR belongs to the
/// APP core. Boot text, so it is on the console whatever the link does.
const APP_CORE_ISR: &str = "[INIT] RMT ISR on APP core";
/// The Q5 fallback arm, which a run with core 1 running must not print.
const SINGLE_CORE_LINE: &str = "[INIT] APP core unavailable; RMT ISR on PRO core";

// ---------------------------------------------------------------------------
// G2: the idle heartbeat, both boot paths
// ---------------------------------------------------------------------------

/// The three lines G2 calls the **idle heartbeat**, in the order the board
/// prints them. Log records on the link since proto 32, so a line carries a
/// `[INFO] fw_esp32v3…: ` prefix and the marker is found anywhere in it.
const HEARTBEAT: &[&str] = &["[stack] heartbeat: ", "[MEM] free=", "[JIT] used="];

/// A figure: the main stack's size, `_stack_start − _stack_end` (the residual
/// of RWDATA after the statics), as every `[stack]` line's `of <n> B` prints
/// it. `lp-emu-esp32v3/tests/boot_idle.rs` pins the same key off the boot
/// line.
const MAIN_STACK_KEY: &str = "main_stack_bytes";

/// A POSITIONAL figure: the ROM-up run's main-stack high-water minus the
/// direct load's. Not a memory-transfer field — it is how deep an interrupt
/// happened to land, and the two paths reach the stop-all at different
/// points in the pacer's phase — so the record holds CI's value
/// (`Figures::positional_int`). Since proto 32 the request goes over the
/// link after the hello, not 1 ms after `[INIT] I/O task spawned`, so this
/// is not the pre-lp-link value.
const PATH_HIGH_WATER_GAP_KEY: &str = "boot_idle.path_high_water_gap";

/// **G2, both halves, and G2's memory figures side by side.** The shipped
/// image on the merged chip under `--strict-bus`, direct-loaded and ROM-up:
/// each brings the link up, says hello, answers a `stopAllProjects` (the
/// smallest request that reaches `log_memory`, so the smallest that elicits
/// the triple — see `lp-emu-esp32v3/tests/boot_idle.rs`), and prints the
/// triple, with zero unmapped accesses and core 1 running. The two paths
/// then report the same memory figures: `[MEM]` and `[JIT]` exactly, the
/// stack's size exactly, and the stack's high-water within the recorded
/// positional gap.
///
/// What changed with the move: the request goes out once the board's hello
/// has arrived on the link, not 1 ms after `[INIT] I/O task spawned`; the
/// host does not ask for packed replies, so `[MEM]` is the heap ratchet's
/// figure (no learned table); and the reply and the triple are link traffic.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_ELF and LP_EMU_ESP32V3_MERGED; run through `just test-emu-esp32v3-boot`"]
fn g2_both_boot_paths_reach_the_idle_heartbeat_over_the_link() {
    let test = "g2_both_boot_paths_reach_the_idle_heartbeat_over_the_link";
    let (Some(elf), Some(merged)) = (shipped(test), merged(test)) else {
        return;
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let direct = idle_heartbeat(
        "direct",
        Esp32V3Builder::new()
            .boot_mode(BootMode::Direct)
            .app(AppSource::Path(elf))
            .flash(FlashBacking::Copy(merged.clone()))
            .flash_len(len)
            .strict(true),
    );
    let rom_up = idle_heartbeat(
        "rom-up",
        Esp32V3Builder::new()
            .boot_mode(BootMode::RomUp)
            .flash(FlashBacking::Copy(merged))
            .flash_len(len)
            .strict(true),
    );
    // The bootloader's own log is in front of the ROM-up run's heartbeat,
    // including the `E` that is correct (`lp-emu-esp32v3/tests/rom_up_boot.rs`
    // compares it line for line).
    let text = rom_up.console.join("\n");
    assert!(text.contains("ets Jul 29 2019 12:21:46"), "the ROM banner");
    assert!(
        text.contains("Image contains multiple DROM segments"),
        "the DROM-segments line the desk board prints on every boot"
    );

    let (a, b) = (&direct.triple, &rom_up.triple);
    println!("direct (lp-emu:esp32v3:t1):\n  {}", a.join("\n  "));
    println!("rom-up (lp-emu:esp32v3:t1):\n  {}", b.join("\n  "));
    // ⚠️ The `[stack]` line is compared apart from the other two: with core
    // 1 running, the two paths agree exactly on every memory-transfer field
    // and differ on the main stack's high-water (module docs of
    // `boot_idle.rs`, M4 P1).
    assert_eq!(
        a[1..],
        b[1..],
        "the two boot paths report the same memory figures"
    );
    let (hw_direct, hw_rom_up) = (number(&a[0], "high-water "), number(&b[0], "high-water "));
    assert_eq!(
        a[0].split("high-water").next(),
        b[0].split("high-water").next(),
        "and it is the same stack, reported the same way"
    );
    assert_eq!(
        number(&a[0], " B of "),
        number(&b[0], " B of "),
        "the stack's size is the same on both paths: {a:?} vs {b:?}"
    );
    let mut figures = Figures::new(
        "esp32v3",
        "emu_v3_link_gates::g2_both_boot_paths_reach_the_idle_heartbeat_over_the_link",
    );
    figures.int(MAIN_STACK_KEY, number(&a[0], " B of "));
    figures.positional_int(PATH_HIGH_WATER_GAP_KEY, hw_rom_up as i64 - hw_direct as i64);
    figures.verify();
    assert!(
        direct
            .console
            .iter()
            .any(|l| l.contains("heap=15072+112640+98304+15536=241552")),
        "the heap arithmetic is the desk board's"
    );
}

/// What one G2 run heard.
struct Heartbeat {
    console: Vec<String>,
    /// `[stack]`, `[MEM]`, `[JIT]`, each from its marker to the line's end.
    triple: Vec<String>,
}

/// One boot path to the elicited triple, asserting what G2 (a) asserts of
/// either path.
fn idle_heartbeat(path: &str, builder: Esp32V3Builder) -> Heartbeat {
    let board = V3Board::build(builder).expect("the shipped image builds a machine");
    let mut host = EmuLinkHost::new(board, NONCE, false);
    let hello = host
        .wait_for_line("\"hello\":{", HELLO_BUDGET_US)
        .expect("the boot");
    assert!(
        hello.is_some(),
        "{path}: no hello:\n{}",
        tail(host.console())
    );
    const ID: u64 = 1;
    host.send(&ClientMessage {
        id: ID,
        msg: ClientRequest::StopAllProjects,
    })
    .expect("the link takes the request");
    let answer = host
        .wait_for_line(&format!("M!{{\"id\":{ID},"), 2_000_000)
        .expect("the run");
    assert!(
        answer.is_some(),
        "{path}: the stop-all was never answered:\n{}",
        tail(host.console())
    );
    // The handler's own log lines ride the log channel and can trail the
    // reply; the triple is among them.
    let done = host
        .wait_for_line("Stopped all projects", 1_000_000)
        .expect("the run");
    assert!(done.is_some(), "{path}:\n{}", tail(host.console()));
    let m = &host.board.machine;
    assert!(
        m.first_strict_violation().is_none(),
        "{path}: no strict refusal anywhere in the boot: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "{path}: G2's first binary condition — zero unmapped accesses"
    );
    let text = host.console().join("\n");
    for line in HEARTBEAT {
        assert!(text.contains(line), "{path}: no `{line}` in:\n{text}");
    }
    assert!(
        text.contains(APP_CORE_ISR) && !text.contains(SINGLE_CORE_LINE),
        "{path}: core 1 runs (M4 P1), so the firmware prints silicon's `{APP_CORE_ISR}` and \
         not the Q5 fallback:\n{text}"
    );
    assert!(
        text.contains("M!{\"id\":0,\"msg\":{\"hello\""),
        "{path}: the hello on the link:\n{text}"
    );
    assert!(
        text.contains("M!{\"id\":1,\"msg\":\"stopAllProjects\"}"),
        "{path}: the request is answered:\n{text}"
    );
    assert_eq!(host.link_errors, 0, "{path}: {text}");
    assert_eq!(host.counters().payload_errors, 0);
    let triple = host
        .console()
        .iter()
        .filter_map(|l| {
            HEARTBEAT
                .iter()
                .find_map(|h| l.find(h))
                .map(|at| l[at..].to_string())
        })
        .take(3)
        .collect::<Vec<_>>();
    assert_eq!(triple.len(), 3, "{path}: the triple: {triple:?}");
    Heartbeat {
        console: host.console().to_vec(),
        triple,
    }
}

// ---------------------------------------------------------------------------
// R6: three requests after the boot settles
// ---------------------------------------------------------------------------

/// The line the server loop prints on its first successful tick — a log
/// record, so it reaches the host once the link is up. A request sent after
/// it lands on a board that has left the busy boot behind, which is where R6
/// lived.
const BOOT_COMPLETE: &str = "[RECOVERY] boot complete (first frame served)";

/// **R6, pinned on the link.** Three requests, each sent on its own — the
/// first a millisecond after `[RECOVERY] boot complete` reaches the host,
/// the next two fifty milliseconds apart — and three answers, in order.
/// Before M4 P3b the first was swallowed: the guest's thread-mode executor
/// parked for ever at the end of its first iteration (the crate README's
/// "The link, after the boot settles").
///
/// What changed with the move (was `uart_socket.rs`'s
/// `a_script_with_three_requests_is_answered_three_times`): three link
/// messages from the product's host end instead of three `M!` lines from a
/// byte script, on the same schedule relative to the same line — which the
/// host now reads off the log channel rather than off the raw wire.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_ELF; run through `just test-emu-esp32v3-boot`"]
fn three_requests_after_the_boot_settles_are_answered_three_times() {
    let Some(elf) = shipped("three_requests_after_the_boot_settles_are_answered_three_times")
    else {
        return;
    };
    let board = V3Board::build(
        Esp32V3Builder::new()
            .boot_mode(BootMode::Direct)
            .app(AppSource::Path(elf))
            .strict(true),
    )
    .expect("the shipped image builds a machine");
    let mut host = EmuLinkHost::new(board, NONCE, true);
    let settled = host
        .wait_for_line(BOOT_COMPLETE, HELLO_BUDGET_US)
        .expect("the boot");
    assert!(
        settled.is_some(),
        "no `{BOOT_COMPLETE}` on the link:\n{}",
        tail(host.console())
    );
    for (id, gap_us) in [(1u64, 1_000u64), (2, 50_000), (3, 50_000)] {
        let until = host.board.machine.micros() + gap_us;
        host.run_until(until, None).expect("the run");
        host.send(&ClientMessage {
            id,
            msg: ClientRequest::StopAllProjects,
        })
        .expect("the link takes the request");
    }
    let reply = |id: u64| format!("M!{{\"id\":{id},\"msg\":\"stopAllProjects\"}}");
    let third = host.wait_for_line(&reply(3), 500_000).expect("the run");
    let text = host.console().join("\n");
    assert!(third.is_some(), "the third answer never came:\n{text}");
    let m = &host.board.machine;
    assert!(m.first_strict_violation().is_none(), "no strict refusal");
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "zero unmapped accesses"
    );
    let mut last = text.find(BOOT_COMPLETE).expect("seen above");
    for id in 1..=3 {
        last += text[last..]
            .find(&reply(id))
            .unwrap_or_else(|| panic!("request {id} was not answered after byte {last}:\n{text}"));
    }
    assert_eq!(
        text.matches("\"msg\":\"stopAllProjects\"").count(),
        3,
        "each request answered exactly once:\n{text}"
    );
    assert_eq!(host.link_errors, 0, "{text}");
}

// ---------------------------------------------------------------------------
// The shipped image survives a project load
// ---------------------------------------------------------------------------

/// M4 P4b's classic-level pin, on the shipped image: loading a project used
/// to kill the guest a few emulated seconds in (CALL0/CALLX0 zeroing
/// `PS.CALLINC`; the crate README's "The window, across a context save").
/// The run reaches its deadline, nothing is unmapped, the project loaded,
/// the server loop's heartbeat still arrives after the load, and at least
/// sixty whole frames came off pad 18. **Counted in frames, never in
/// emulated microseconds.**
///
/// What changed with the move (was `project_load_survives.rs`): the upload
/// is a deploy over the link rather than the committed
/// `walks/shader-oracle.script`, so the project lands at ~0.5 s instead of
/// seconds later, and the same 6 s deadline gives rendering more of it.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_ELF; run through `just test-emu-esp32v3-boot`"]
fn the_shipped_image_survives_a_project_load_and_keeps_rendering() {
    const DEADLINE_US: u64 = 6_000_000;
    const WHOLE_FRAME_BYTES: usize = LEDS * 3;
    const MIN_WHOLE_FRAMES: usize = 60;
    let Some(elf) = shipped("the_shipped_image_survives_a_project_load_and_keeps_rendering") else {
        return;
    };
    let board = V3Board::build(
        Esp32V3Builder::new()
            .boot_mode(BootMode::Direct)
            .app(AppSource::Path(elf))
            .strict(true),
    )
    .expect("the shipped image builds a machine");
    let mut host = EmuLinkHost::new(board, NONCE, true);
    deploy(&mut host, &oracle_project("survives"));
    host.run_until(DEADLINE_US, None)
        .expect("the run reaches its deadline rather than stopping");
    let text = host.console().join("\n");
    let m = &mut host.board.machine;
    assert!(
        m.first_strict_violation().is_none(),
        "no strict-bus violation: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "zero unmapped accesses"
    );
    let loaded_at = text
        .find("\"loadProject\":{\"handle\":1}")
        .unwrap_or_else(|| panic!("the project loaded\n{text}"));
    assert!(
        text[loaded_at..].contains("\"msg\":{\"heartbeat\":{"),
        "the server loop's heartbeat still arrives after the load:\n{}",
        &text[loaded_at..]
    );
    m.flush_frames();
    let frames = m.frames(PAD);
    let whole = frames
        .iter()
        .filter(|f| f.is_complete() && f.wire.len() == WHOLE_FRAME_BYTES)
        .count();
    let bit_errors: u64 = frames.iter().map(|f| f.error_count).sum();
    assert_eq!(bit_errors, 0, "no bit errors in {} frames", frames.len());
    assert!(
        whole >= MIN_WHOLE_FRAMES,
        "at least {MIN_WHOLE_FRAMES} whole frames on pad {PAD}: {whole} whole of {}",
        frames.len()
    );
    assert_eq!(host.link_errors, 0);
    println!(
        "survives (lp-emu:esp32v3:t1): {} frames on pad {PAD}, {whole} whole, to {} us",
        frames.len(),
        DEADLINE_US
    );
}

// ---------------------------------------------------------------------------
// A frame three ways (the frame-dump image)
// ---------------------------------------------------------------------------

/// `cargo test -p lpa-server --test shader_oracle_frame -- --nocapture`: the
/// same constant `lp-emu-esp32v3/tests/shader_oracle_pin.rs` pinned (wasmtime
/// and rv32-emu agreed on every byte, 2026-09-11).
const ORACLE_RGB: &str = "324a0208376a1c2889007668098b4b0375544602631253162b0f7051068a838b000097890b63b208a1601b30951b1c72660069af481900a49554e3212b48e41955cdad4d154f047b103e90441ec10ed47200bcb627f657019fb523c13e3794161c952e04a8743b36e681e90e225ef47f09d1174ebc035c8009447f3fb11b6112ca048dc419dd5fae02903ab21f015c6f026047006a750aa45b69b20834c32b7e8f0012913c086a360144365600567c064d430e9127239632148702475f4c2d05";
/// The oracle's own `crc=`, over the same 192 bytes.
const ORACLE_CRC: u32 = 0x5577_2254;
/// The DOM-Z-102's first fused DATA terminal; the scratch copy of the
/// oracle project is retargeted `D10` -> `IO18`.
const PAD: u8 = 18;
const LEDS: usize = 64;
const BITS: usize = LEDS * 24;
/// The driver's open line — `wire=<n>` and "slot per transmission", because
/// this chip binds a wire and chooses the slot per frame.
const OPEN_LINE: &str = "Esp32V3RmtWs281xDriver::open: endpoint=esp32v3-rmt-ws281x:ws281x:local:IO18 \
     gpio=/gpio/18 wire=0 bytes=192 (slot per transmission)";
/// `frame_dump::log_open`'s companion, and the proof this is the frame-dump
/// image and not the shipped one.
const OPEN_DUMP_LINE: &str = "[OUT] open endpoint=esp32v3-rmt-ws281x:ws281x:local:IO18 \
     bytes=192 leds=64 (frame-dump build)";
/// The walk's deadline. Measured: the project lands at ~0.52 s (direct), the
/// deferred lit dump fires 30 frames after the first lit one, and 4 s leaves
/// well over a thousand frames for "every later frame is the same frame". A
/// run parameter, not a tolerance — a clock-free project renders the same
/// bytes for ever.
const ORACLE_US: u64 = 4_000_000;

/// **M4 P4's gate, on the link: the first lit frame the product firmware puts
/// on IO18 is the host oracle's frame, read three ways** — (a) the
/// firmware's own `[OUT] dump`, now log records delivered over the link, (b)
/// the pad, decoded by a decoder that never spoke to the firmware, (c) the
/// oracle constant. Every later frame is the same frame, the run reaches its
/// deadline with nothing unmapped, and a second run is the same run (its
/// `--dump-frames` file byte-identical). `lp-emu-esp32v3/tests/
/// shader_oracle_pin.rs`'s module docs are the three subtleties (the
/// deferred dump, the first LIT frame, the order rule); none of them moved.
///
/// What changed with the move: the upload is a deploy over the link, not the
/// committed `walks/shader-oracle.script`; the console is read as it arrived,
/// unrepaired (module docs); and "the guest heard the whole walk" is the
/// link's own count (no link error, no payload error) rather than the
/// absence of `dropping unparseable`.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_FRAME_DUMP_ELF; run through `just test-emu-esp32v3-boot`"]
fn the_frame_dump_walk_reads_the_oracles_frame_three_ways() {
    let Some(elf) = frame_dump("the_frame_dump_walk_reads_the_oracles_frame_three_ways") else {
        return;
    };
    let dir = scratch("oracle");
    let a = oracle_run(&elf, &dir.join("a.jsonl"));
    let text = a.console.join("\n");
    println!(
        "oracle (lp-emu:esp32v3:t1): {} us, {} instructions, {} frames on gpio{PAD}; upload at \
         {:.3} s; host link {}",
        a.micros,
        a.instructions,
        a.frames.len(),
        a.uploaded_at_s,
        a.link
    );
    // 1. The guest heard the whole conversation, and P1's dual core is there.
    assert!(
        text.contains(APP_CORE_ISR),
        "the RMT ISR is not on the APP core — M4 P1 has regressed:\n{text}"
    );
    assert!(
        text.contains("\"loadProject\":{\"handle\":1}"),
        "the project did not load:\n{text}"
    );
    // 2. The output opened, on the pad the scratch copy retargeted it to.
    assert!(text.contains(OPEN_LINE), "{text}");
    assert!(text.contains(OPEN_DUMP_LINE), "{text}");
    // 3. By value, never by length: a plain boot routes gpio1 (the console's
    // TX) too.
    assert!(
        a.routed.contains(&(
            PadId(PAD),
            RouteSource::Signal(SignalId(rmt::RMT_SIG_0), false)
        )),
        "gpio{PAD} is not routed to RMT_SIG_0: {:?}",
        a.routed
    );
    assert_eq!(a.order, ColorOrder::Grb, "the default strip order");

    // 4. Frames, and a lit one: whole, clean, latched, and the oracle's.
    let frames = &a.frames;
    let lit = frames
        .iter()
        .position(|f| f.wire.iter().any(|b| *b != 0))
        .unwrap_or_else(|| {
            panic!(
                "{} frames on gpio{PAD} and every one of them black — the compile-window \
                 fallback never gave way to a render",
                frames.len()
            )
        });
    let f = &frames[lit];
    assert_eq!(f.leds(), LEDS, "frame {}", f.n);
    assert_eq!(f.bits, BITS, "frame {}", f.n);
    assert_eq!(f.error_count, 0, "frame {}: {:?}", f.n, f.errors);
    assert!(f.is_complete(), "frame {} was cut short: {f:?}", f.n);
    let rgb = unpermute(&f.wire, ColorOrder::Grb);
    let pad_rgb = hex(&rgb);
    assert_eq!(
        pad_rgb,
        ORACLE_RGB,
        "frame {}: the first lit frame off gpio{PAD} is not the oracle's frame (wire as \
         carried: {}) — a per-pixel byte swap means the ORDER assumption is wrong and the \
         decoder is not to be fixed to match; anything else is a compiler or machine finding",
        f.n,
        hex(&f.wire)
    );
    assert_eq!(fnv1a(&rgb), ORACLE_CRC);

    // 5. Every later frame is the same frame; a frame the deadline cut can
    // only be the last one.
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

    // 6. Reading (a), off the link: the LAST whole dump, the deferred lit one.
    let dumped = last_dump_rgb(&text).unwrap_or_else(|| {
        panic!("the guest printed no whole `[OUT] dump frame=` — nothing rendered:\n{text}")
    });
    assert_eq!(
        dumped, pad_rgb,
        "the firmware's own dump and the pad disagree — the RMT encode, the colour order or \
         the refill path.\n  [OUT] dump: {dumped}\n  pad {PAD}:     {pad_rgb}"
    );
    assert_eq!(
        dumped, ORACLE_RGB,
        "the firmware's own dump is not the oracle's frame"
    );
    println!(
        "oracle: first lit n={} of {} frames, {later} identical after it; [OUT] dump == pad \
         {PAD} == [ORACLE] ({} hex characters)",
        f.n,
        frames.len(),
        dumped.len()
    );
    // 7. Two runs are the same run, down to the dump file's bytes.
    let b = oracle_run(&elf, &dir.join("b.jsonl"));
    assert_eq!((a.cycles, a.instructions), (b.cycles, b.instructions));
    assert_eq!(a.console, b.console, "two runs heard different consoles");
    assert_eq!(a.frames.len(), b.frames.len());
    let (sa, sb) = (sha256_of(&a.dump), sha256_of(&b.dump));
    println!(
        "oracle: two runs, {} frames each, dump sha256 {sa}",
        a.frames.len()
    );
    assert_eq!(sa, sb, "the two dump files differ");
    let _ = std::fs::remove_dir_all(&dir);
}

/// After a reboot the board brings its saved project up by itself, with the
/// host already holding the link: the desk walk's round 2 (a `reboot` asked
/// over the link into the project `lp-cli upload` saved —
/// `scripts/m4-hardware-walk.sh`), and the load burst that cost the classic
/// its `[OUT] dump` (docs/defects/2026-09-29-the-classics-log-ring-drops-
/// records-under-a-project-load-burst.md).
///
/// The boot, the auto-load, the compile and both dumps land in the log ring
/// together while the UART drains it at line rate. The link task used to take
/// records off the ring that the link's two-slot datagram queue then refused
/// — each one lost with no notice a host could see — and the socket-hosted
/// walk lost the lit dump that way, three runs of three. A record now waits in
/// the ring until the link has a slot, so every dump of the second boot
/// arrives whole: the open-time black one and the deferred lit one, which is
/// the oracle's frame. What the ring itself drops while nothing drains it (the
/// boot's burst runs ahead of the first link-task pass) it drops oldest-first
/// and says so in-band; the count is printed, not gated.
///
/// Direct load with the merged chip image as the flash, so the project lands
/// in a real `lpfs` and survives the reboot.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_FRAME_DUMP_ELF and LP_EMU_ESP32V3_MERGED; run through `just test-emu-esp32v3-boot`"]
fn a_reboot_into_the_saved_project_delivers_every_dump_whole() {
    let test = "a_reboot_into_the_saved_project_delivers_every_dump_whole";
    let (Some(elf), Some(merged)) = (frame_dump(test), merged(test)) else {
        return;
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let board = V3Board::build(
        Esp32V3Builder::new()
            .boot_mode(BootMode::Direct)
            .app(AppSource::Path(elf))
            .flash(FlashBacking::Copy(merged))
            .flash_len(len)
            .strict(true)
            .time_grade(TimeGrade::T1),
    )
    .expect("the frame-dump image builds a machine");
    let mut host = EmuLinkHost::new(board, NONCE, true);
    deploy(&mut host, &oracle_project("reboot"));
    host.send(&ClientMessage {
        id: 9,
        msg: ClientRequest::Reboot,
    })
    .unwrap();
    let asked = host.board.machine.micros();
    let restarted = host
        .wait_for_line("[link] reset (PeerRestarted)", HELLO_BUDGET_US)
        .expect("the reboot");
    assert!(
        restarted.is_some(),
        "the host never saw the board restart:\n{}",
        tail(host.console())
    );
    let second_boot = host.console().len();
    host.run_until(asked + REBOOT_WATCH_US, None)
        .expect("the run reaches its deadline");
    assert_eq!(host.board.machine.reboots(), 1, "one software reboot");
    assert!(
        host.board.machine.first_strict_violation().is_none(),
        "a strict refusal: {:?}",
        host.board.machine.first_strict_violation()
    );
    // The reboot's own `PeerRestarted` is the one reset the host counts.
    assert_eq!(host.link_errors, 1, "{}", tail(host.console()));
    assert_eq!(host.counters().payload_errors, 0);

    let after = &host.console()[second_boot..];
    let text = after.join("\n");
    assert!(
        text.contains("Boot: auto-loaded project"),
        "the second boot did not load the saved project:\n{text}"
    );
    let dumps = dump_parts(&text);
    let ring_drops: Vec<&str> = after
        .iter()
        .filter(|l| l.contains(" log records dropped"))
        .map(String::as_str)
        .collect();
    println!(
        "reboot (lp-emu:esp32v3:t1): dumps after it (frame, parts arrived, of) {dumps:?}; \
         the ring's own notices {ring_drops:?}"
    );
    assert!(
        dumps.len() >= 2,
        "the black dump and the lit one after the reboot: {dumps:?}\n{text}"
    );
    for (frame, got, of) in &dumps {
        assert_eq!(
            got,
            of,
            "the frame={frame} dump lost parts on the way to the host ({got} of {of}) — a log \
             record taken off the ring and refused by the link: {dumps:?}\n{}",
            tail(host.console())
        );
    }
    assert_eq!(
        last_dump_rgb(&text).as_deref(),
        Some(ORACLE_RGB),
        "the second boot's lit dump is the oracle's frame"
    );
    let _ = std::fs::remove_dir_all(scratch("project-reboot"));
}

/// How long the host watches after asking for the reboot: the boot, the
/// auto-load, the compile and the deferred lit dump land in the first
/// ~0.2 s emulated; a run parameter, not a tolerance.
const REBOOT_WATCH_US: u64 = 1_500_000;

struct OracleRun {
    console: Vec<String>,
    frames: Vec<Frame>,
    routed: Vec<(PadId, RouteSource)>,
    order: ColorOrder,
    cycles: u64,
    instructions: u64,
    micros: u64,
    uploaded_at_s: f64,
    dump: PathBuf,
    link: String,
}

fn oracle_run(elf: &Path, dump: &Path) -> OracleRun {
    let board = V3Board::build(
        Esp32V3Builder::new()
            .app(AppSource::Path(elf.to_path_buf()))
            .dump_frames(FrameSink::File(dump.to_path_buf()))
            .strict(true)
            .time_grade(TimeGrade::T1),
    )
    .expect("the frame-dump image builds a machine");
    let mut host = EmuLinkHost::new(board, NONCE, true);
    let uploaded_at_s = deploy(&mut host, &oracle_project("oracle"));
    host.run_until(ORACLE_US, None)
        .expect("the run reaches its deadline");
    let m = &mut host.board.machine;
    m.flush_frames();
    assert!(
        m.first_strict_violation().is_none(),
        "a strict refusal: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "unmapped"
    );
    assert_eq!(host.link_errors, 0, "{}", tail(host.console()));
    let c = host.counters();
    assert_eq!(c.payload_errors, 0);
    OracleRun {
        console: host.console().to_vec(),
        frames: host.board.machine.frames(PAD).to_vec(),
        routed: host.board.machine.routed_pads(),
        order: host.board.machine.strip().order,
        cycles: host.board.machine.cycles(),
        instructions: host.board.machine.instructions(),
        micros: host.board.machine.micros(),
        uploaded_at_s,
        dump: dump.to_path_buf(),
        link: format!(
            "{} out / {} in, {} resent",
            c.frames_tx, c.frames_rx, c.resends
        ),
    }
}

/// The `rgb=` of the **last** whole `[OUT] dump` in a console — the deferred
/// lit dump, not the open-time black one. A dump is `part=i/n` lines (one log
/// record is cut at 200 bytes), so the parts' `rgb=` are joined in order and
/// only a dump whose parts all arrived counts. `scripts/frame-dump-hex.sh` is
/// the same reader for the walks.
fn last_dump_rgb(text: &str) -> Option<String> {
    let mut last = None;
    let mut current = String::new();
    let mut next_part = 0u32;
    for line in text.lines().filter(|l| l.contains("[OUT] dump frame=")) {
        let Some(part) = line
            .split_whitespace()
            .find_map(|w| w.strip_prefix("part="))
            .and_then(|p| p.split_once('/'))
            .and_then(|(i, n)| Some((i.parse::<u32>().ok()?, n.parse::<u32>().ok()?)))
        else {
            continue;
        };
        let Some(hex) = line.split("rgb=").nth(1).map(str::trim) else {
            continue;
        };
        if part.0 == 1 {
            current = hex.to_string();
            next_part = 2;
        } else if part.0 == next_part {
            current.push_str(hex);
            next_part += 1;
        } else {
            next_part = 0;
        }
        if part.0 == part.1 && next_part == part.1 + 1 {
            last = Some(current.clone());
        }
    }
    last
}

/// Every `[OUT] dump` in a console, in order: its `frame=`, how many of its
/// distinct `part=i/n` lines arrived, and `n`.
fn dump_parts(text: &str) -> Vec<(u64, u32, u32)> {
    let mut dumps: Vec<(u64, Vec<u32>, u32)> = Vec::new();
    for line in text.lines().filter(|l| l.contains("[OUT] dump frame=")) {
        let word = |key: &str| {
            line.split_whitespace()
                .find_map(|w| w.strip_prefix(key))
                .map(str::to_string)
        };
        let (Some(frame), Some(part)) = (word("frame="), word("part=")) else {
            continue;
        };
        let (Ok(frame), Some((Ok(i), Ok(n)))) = (
            frame.parse::<u64>(),
            part.split_once('/')
                .map(|(i, n)| (i.parse::<u32>(), n.parse::<u32>())),
        ) else {
            continue;
        };
        match dumps.last_mut() {
            Some((f, parts, _)) if *f == frame => {
                if !parts.contains(&i) {
                    parts.push(i);
                }
            }
            _ => dumps.push((frame, vec![i], n)),
        }
    }
    dumps
        .into_iter()
        .map(|(f, parts, n)| (f, parts.len() as u32, n))
        .collect()
}

// ---------------------------------------------------------------------------
// Five wires over four slots (the frame-dump image)
// ---------------------------------------------------------------------------

/// The project's five ports, in the order `output.json` declares them:
/// IO18 / IO16 / IO14 / IO2 are the DOM-Z-102's fused DATA terminals, IO13
/// the spare.
const FIVE_PADS: [u8; 5] = [18, 16, 14, 2, 13];
/// `projects/test/five-wire`: 16 lamps a port.
const FIVE_LEDS: usize = 16;
const FIVE_BITS: usize = FIVE_LEDS * 24;
/// IO18's open line (the others are the same shape on their own pads) — a
/// log record, which the log ring may drop under the load's burst (see
/// [`five_wires_are_routed_over_four_slots`]).
const FIVE_OPEN_LINE: &str = "Esp32V3RmtWs281xDriver::open: endpoint=esp32v3-rmt-ws281x:ws281x:local:IO18 \
     gpio=/gpio/18 wire=0 bytes=48 (slot per transmission)";
/// The walk's deadline: the project lands at ~0.5 s and the five outputs
/// open right after it, which leaves several `REPORT_EVERY_FRAMES = 60`
/// summary lines per wire. A run parameter, not a tolerance.
const FIVE_US: u64 = 4_000_000;
/// How often `frame_dump::report` prints a wire's summary line.
const REPORT_EVERY_FRAMES: usize = 60;

/// **Five wires over four RMT slots, on the desk board's own pins**
/// (`lp-emu-esp32v3/tests/five_wires.rs`'s module docs are the why: the fifth
/// wire time-shares a slot by per-transmission pad muxing, which only the
/// product path exercises). One run, read three ways, as before:
///
/// 1. **routing** — five pads routed, four pooled RMT signals, one signal on
///    two pads with a park to `GPIO_OUT` between (the second wave, off the
///    pin log), IO18 on `RMT_SIG_0`, and every frame on every pad whole;
/// 2. **determinism** — a second run at the same quantum decodes the same
///    frames and hears the same console, and a third at quantum 64 decodes
///    the same frame shapes (the counts may differ by the one frame a
///    deadline can land inside);
/// 3. **the bytes** — five distinct lit wires, each FNV-1a-equal to a
///    `[OUT] frame=… crc=` summary line the guest itself printed, and each pad
///    carrying every frame the guest counted and no more than one report
///    period beyond.
///
/// What changed with the move: a deploy over the link instead of
/// `walks/five-wire.script`; the console is whole records, so a summary line
/// can no longer be cut in flight by the deadline — but it can still be
/// waiting in the board's log ring, and [`reported`] lets the host keep
/// servicing the link until it arrives, exactly as it let UART0 finish a
/// burst before.
#[test]
#[ignore = "needs LP_EMU_ESP32V3_FRAME_DUMP_ELF; run through `just test-emu-esp32v3-boot`"]
fn five_wires_share_four_slots_and_each_matches_the_guests_own_checksum() {
    let Some(elf) =
        frame_dump("five_wires_share_four_slots_and_each_matches_the_guests_own_checksum")
    else {
        return;
    };
    let dir = scratch("five-wires");
    let mut a = five_wire_run(&elf, 256, &dir);
    five_wires_are_routed_over_four_slots(&a);
    // Determinism before the checksums: the checksum claim may run the host
    // past the deadline to let a report arrive ([`reported`]).
    let b = five_wire_run(&elf, 256, &dir);
    assert_eq!(
        (a.cycles, a.instructions),
        (b.cycles, b.instructions),
        "two identical runs diverged"
    );
    assert_eq!(a.console, b.console, "two runs said different things");
    assert_eq!(a.frames, b.frames, "two runs decoded different frames");
    let c = five_wire_run(&elf, 64, &dir);
    // Across two quanta, the bytes — and only the bytes, and only the frames
    // both runs finished (`five_wires.rs` says why: the same frame starts a
    // window apart, so the deadline can cut the last one differently).
    let (q256, q64) = (shapes(&a.frames), shapes(&c.frames));
    assert_eq!(q256.len(), q64.len(), "a pad went missing at quantum 64");
    for ((pad, fa), (_, fc)) in q256.iter().zip(q64.iter()) {
        let shared = fa.len().min(fc.len());
        let first_diff = fa.iter().zip(fc.iter()).position(|(x, y)| x != y);
        println!(
            "five_wires: gpio{pad}: q256 {} frame(s), q64 {} frame(s), first difference {:?} \
             of {shared} shared",
            fa.len(),
            fc.len(),
            first_diff
        );
        if let Some(at) = first_diff {
            assert!(
                at + 1 >= shared,
                "gpio{pad}: quantum 64 and quantum 256 decoded a different frame at index {at} \
                 of {shared} shared.\n  q256: {:?}\n  q64:  {:?}",
                fa[at],
                fc[at]
            );
        }
        assert!(
            fa.len().abs_diff(fc.len()) <= 1,
            "gpio{pad}: {} frames at quantum 256 and {} at quantum 64",
            fa.len(),
            fc.len()
        );
    }
    every_wire_matches_the_guests_own_summary_line(&mut a);
    let _ = std::fs::remove_dir_all(&dir);
}

struct FiveWireRun {
    host: EmuLinkHost<V3Board>,
    console: Vec<String>,
    frames: Vec<(u8, Vec<Frame>)>,
    pin_log: String,
    cycles: u64,
    instructions: u64,
}

fn five_wire_run(elf: &Path, quantum: u64, dir: &Path) -> FiveWireRun {
    let log = dir.join(format!("pins-q{quantum}.txt"));
    let board = V3Board::build(
        Esp32V3Builder::new()
            .app(AppSource::Path(elf.to_path_buf()))
            .dump_frames(FrameSink::Memory)
            .pin_log(PinLogSink::File(log.clone()))
            .strict(true)
            .time_grade(TimeGrade::T1)
            .core_quantum(quantum),
    )
    .expect("the frame-dump image builds a machine");
    let mut host = EmuLinkHost::new(board, NONCE, true);
    deploy(&mut host, &repo_root().join("projects/test/five-wire"));
    host.run_until(FIVE_US, None)
        .expect("the run reaches its deadline");
    let m = &mut host.board.machine;
    // A frame is not closed until something follows its latch; the same call
    // flushes the pin log's writer.
    m.flush_frames();
    assert!(
        m.first_strict_violation().is_none(),
        "a strict refusal: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "unmapped"
    );
    let frames = FIVE_PADS
        .iter()
        .map(|p| (*p, m.frames(*p).to_vec()))
        .collect();
    let (cycles, instructions) = (m.cycles(), m.instructions());
    assert_eq!(host.link_errors, 0, "{}", tail(host.console()));
    FiveWireRun {
        console: host.console().to_vec(),
        host,
        frames,
        pin_log: std::fs::read_to_string(&log).unwrap_or_default(),
        cycles,
        instructions,
    }
}

/// Claim 1: five pads, four pooled slots, the mux between waves.
fn five_wires_are_routed_over_four_slots(r: &FiveWireRun) {
    let text = r.console.join("\n");
    assert!(
        text.contains(APP_CORE_ISR),
        "the RMT ISR is not on the APP core — the pusher's whole premise:\n{text}"
    );
    assert!(
        text.contains("\"loadProject\":{\"handle\":1}"),
        "the project did not load:\n{text}"
    );
    // ⚠️ IO18's open line is a log record, and the classic's log ring (4 KiB,
    // `log_ring_logger`) keeps the newest records through the load's burst —
    // five opens, their `[OUT] open`s, the compile — and drops the oldest,
    // COUNTED: the link task says `[LINK] <n> log records dropped` when it
    // drains again (measured here: 24 records, IO18's open among them). So
    // the line is asserted, or the loss is: never silently absent. That the
    // output really opened on IO18 is the routing and the frames below,
    // which do not ride the log.
    println!(
        "five_wires (lp-emu:esp32v3:t1): IO18's open line arrived: {}; the ring's own notices {:?}",
        text.contains(FIVE_OPEN_LINE),
        text.lines()
            .filter(|l| l.contains(" log records dropped"))
            .collect::<Vec<_>>()
    );
    assert!(
        text.contains(FIVE_OPEN_LINE) || text.contains(" log records dropped"),
        "IO18's open line is missing and the log ring did not report dropping it:\n{text}"
    );
    let routed = r.host.board.machine.routed_pads();
    for pad in FIVE_PADS {
        assert!(
            routed.iter().any(|(p, _)| *p == PadId(pad)),
            "gpio{pad} is not routed at all: {routed:?}"
        );
    }
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
    let mut distinct: Vec<&String> = signal_pads.iter().map(|(s, _)| s).collect();
    distinct.sort();
    distinct.dedup();
    println!("five_wires: signal→pad routes {signal_pads:?}");
    assert_eq!(
        distinct.len(),
        4,
        "four pooled two-block slots drive the five wires: {signal_pads:?}"
    );
    let shared: Vec<&(String, u8)> = signal_pads
        .iter()
        .filter(|(sig, _)| signal_pads.iter().filter(|(s, _)| s == sig).count() > 1)
        .collect();
    assert!(
        !shared.is_empty(),
        "no signal drove two pads — the fifth wire got a slot of its own, or the second wave \
         never ran: {signal_pads:?}"
    );
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
    assert!(
        signal_pads.contains(&("RMT_SIG_0".to_string(), 18)),
        "gpio18 never carried RMT_SIG_0: {signal_pads:?}"
    );
    for (pad, frames) in &r.frames {
        assert!(!frames.is_empty(), "no frame reached gpio{pad}");
        for f in frames {
            if f.reset_cycles.is_none() {
                assert_eq!(
                    f.n,
                    frames.last().unwrap().n,
                    "gpio{pad}: an open frame that is not the last"
                );
                continue;
            }
            assert_eq!(f.error_count, 0, "gpio{pad} frame {}: {:?}", f.n, f.errors);
            assert!(f.is_complete(), "gpio{pad} frame {} was cut short", f.n);
            assert_eq!(f.leds(), FIVE_LEDS, "gpio{pad} frame {}", f.n);
            assert_eq!(f.bits, FIVE_BITS, "gpio{pad} frame {}", f.n);
        }
    }
}

/// Claim 3: per wire, the pad against the guest's own checksum, and no frame
/// dropped between the driver and the wire.
fn every_wire_matches_the_guests_own_summary_line(r: &mut FiveWireRun) {
    let order = r.host.board.machine.strip().order;
    assert_eq!(order, ColorOrder::Grb, "the default strip order");
    let mut per_wire: Vec<(u8, u32, String)> = Vec::new();
    for (pad, frames) in &r.frames {
        let lit: Vec<&Frame> = frames
            .iter()
            .filter(|f| f.is_complete() && f.wire.iter().any(|b| *b != 0))
            .collect();
        assert!(!lit.is_empty(), "gpio{pad} never carried a lit frame");
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
    for (pad, crc, rgb) in &per_wire {
        println!("five_wires: gpio{pad} crc=0x{crc:08x} rgb={rgb}");
    }
    let mut crcs: Vec<u32> = per_wire.iter().map(|(_, c, _)| *c).collect();
    crcs.sort_unstable();
    crcs.dedup();
    assert_eq!(
        crcs.len(),
        FIVE_PADS.len(),
        "the five wires do not carry five different frames: {per_wire:?}"
    );
    let text = console_text(&r.console);
    let summaries = summaries(&text);
    assert!(
        !summaries.is_empty(),
        "no `[OUT] frame=` summary line:\n{text}"
    );
    for (pad, crc, _) in &per_wire {
        let claimed = summaries
            .iter()
            .find(|s| s.crc == *crc)
            .unwrap_or_else(|| panic!("gpio{pad}: no summary line claims 0x{crc:08x}"));
        assert_eq!(claimed.leds, FIVE_LEDS, "gpio{pad}");
        assert!(
            claimed.lit > 0,
            "gpio{pad}: a summary line for a black frame"
        );
    }
    let wires: Vec<u32> = per_wire.iter().map(|(_, crc, _)| *crc).collect();
    let carried = r.frames.iter().map(|(_, f)| f.len()).max().unwrap_or(0);
    let claimed = reported(r, &wires, carried);
    for (pad, frames) in &r.frames {
        let decoded = frames.len();
        println!(
            "five_wires (lp-emu:esp32v3:t1): gpio{pad}: the guest's last report was frame \
             {claimed}, the pad carried {decoded}"
        );
        assert!(
            decoded >= claimed,
            "gpio{pad}: the guest counted {claimed} frames and the pad carried only {decoded}"
        );
        assert!(
            decoded < claimed + REPORT_EVERY_FRAMES,
            "gpio{pad}: the pad carried {decoded} frames while the guest's last report was \
             frame {claimed} — more than one report period, so summary lines are being lost"
        );
    }
}

/// One `[OUT] frame=<n> leds=<n> crc=0x… lit=<n>` summary line. The line
/// does not name its endpoint, so a summary is matched to a wire by its
/// `crc`, which the five distinct wires make unambiguous.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Summary {
    n: u32,
    leds: usize,
    crc: u32,
    lit: usize,
}

/// Every complete summary line in `console`, in the order printed. The text
/// after the last newline is a line still in flight and is never read.
fn summaries(console: &str) -> Vec<Summary> {
    let mut segments: Vec<&str> = console.split('\n').collect();
    segments.pop();
    segments
        .into_iter()
        .filter(|l| l.contains("[OUT] frame="))
        .map(|line| Summary {
            n: field(line, "frame") as u32,
            leds: field(line, "leds"),
            crc: u32::from_str_radix(&after(line, "crc=0x"), 16)
                .unwrap_or_else(|_| panic!("crc in {line}")),
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

/// The frame every wire's guest counter had reached, read off the report
/// groups (one line per wire every `REPORT_EVERY_FRAMES` frames), or the
/// report that was lost. Every group but the last must be whole; the last may
/// be a prefix of the previous group's order (a burst still leaving the
/// board). `lp-emu-esp32v3/tests/five_wires.rs` had the same rule.
fn reached(summaries: &[Summary], wires: &[u32]) -> Result<usize, String> {
    let mut groups: Vec<(usize, Vec<u32>)> = Vec::new();
    for s in summaries.iter().filter(|s| wires.contains(&s.crc)) {
        let n = s.n as usize;
        match groups.last_mut() {
            Some((at, crcs)) if *at == n => crcs.push(s.crc),
            Some((at, _)) if n != *at + REPORT_EVERY_FRAMES => {
                return Err(format!(
                    "a report of frame {n} follows the group of frame {at}: groups are \
                     {REPORT_EVERY_FRAMES} frames apart, so a whole group was lost"
                ));
            }
            _ => groups.push((n, vec![s.crc])),
        }
    }
    let Some((last_n, last)) = groups.last() else {
        return Err("no summary line reports any of the wires".into());
    };
    let whole = |crcs: &[u32]| {
        let (mut a, mut b) = (crcs.to_vec(), wires.to_vec());
        a.sort_unstable();
        b.sort_unstable();
        a == b
    };
    for (n, crcs) in &groups[..groups.len() - 1] {
        if !whole(crcs) {
            return Err(format!(
                "frame {n}: the report group is {crcs:08x?}, not one line per wire \
                 {wires:08x?}, and later groups followed it — a report was lost"
            ));
        }
    }
    let order: &[u32] = match groups.len() {
        1 => wires,
        k => &groups[k - 2].1,
    };
    if !(whole(last) || order.starts_with(last)) {
        return Err(format!(
            "frame {last_n}: the last report group is {last:08x?}, neither whole nor the start \
             of a burst in order {order:08x?} — a report was lost, not cut off"
        ));
    }
    Ok(*last_n)
}

/// [`reached`], once the board has sent the report the deadline was waiting
/// for: when the pads are a report period past the last whole group, the
/// guest has counted that frame and its report is in the log ring or on the
/// wire, so the host keeps servicing the link — a slice at a time — until
/// the report arrives. Nothing but the console is read from the extra run;
/// the pads' counts stay the deadline's. A report really lost still fails
/// (the next burst arrives with the group before it missing), and the run
/// stops anyway once the pads carry two report periods past the last group.
fn reported(r: &mut FiveWireRun, wires: &[u32], carried: usize) -> usize {
    loop {
        let text = console_text(r.host.console());
        let claimed = reached(&summaries(&text), wires).unwrap_or_else(|lost| panic!("{lost}"));
        let live = FIVE_PADS
            .iter()
            .map(|p| r.host.board.machine.frames(*p).len())
            .max()
            .unwrap_or(0);
        if claimed + REPORT_EVERY_FRAMES > carried || live >= claimed + 2 * REPORT_EVERY_FRAMES {
            return claimed;
        }
        r.host
            .step()
            .expect("the run past the deadline, waiting for a report");
    }
}

/// A frame without its clock: what two runs at two quanta must agree on.
#[allow(
    clippy::type_complexity,
    reason = "a tuple mirroring Frame's own fields reads clearer here than a named type"
)]
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

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

/// Wait for the board's hello, then deploy `dir` over the link as `lp-cli
/// upload` deploys it. The emulated second the deploy was acknowledged.
fn deploy(host: &mut EmuLinkHost<V3Board>, dir: &Path) -> f64 {
    let hello = host
        .wait_for_line("\"hello\":{", HELLO_BUDGET_US)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", tail(host.console()));
    let (uid, _) = lp_cli::commands::dev::validation::validate_local_project(&dir.to_path_buf())
        .unwrap_or_else(|e| panic!("{} validates: {e:#}", dir.display()));
    let files =
        lp_cli::commands::dev::collect_project_deploy_files(&lpfs::LpFsStd::new(dir.to_path_buf()))
            .expect("the project's files");
    host.set_queue_messages(true);
    {
        let mut client = lpa_client::LpClient::new(&mut *host);
        block_on(client.deploy_project_files(&uid, files))
            .unwrap_or_else(|e| panic!("the deploy failed: {e}"));
    }
    host.set_queue_messages(false);
    host.board_seconds()
}

/// `projects/test/shader-oracle`, retargeted `D10` -> `IO18` into a scratch
/// copy: the DOM-Z-102 has no D10, and an output whose endpoint the board
/// does not have never opens. The copy keeps the directory's basename, which
/// is the project's on-device name (`scripts/emu/m4-walk-esp32v3.sh`'s
/// `prepare_project`, by the same mechanism).
fn oracle_project(tag: &str) -> PathBuf {
    let from = repo_root().join("projects/test/shader-oracle");
    let to = scratch(&format!("project-{tag}")).join("shader-oracle");
    std::fs::create_dir_all(&to).expect("scratch project dir");
    for entry in std::fs::read_dir(&from).expect("the oracle project") {
        let path = entry.expect("an entry").path();
        std::fs::copy(&path, to.join(path.file_name().expect("a name"))).expect("a copy");
    }
    let output = to.join("output.json");
    let text = std::fs::read_to_string(&output).expect("output.json");
    let retargeted = text.replace("\"ws281x:local:D10\"", "\"ws281x:local:IO18\"");
    assert!(
        retargeted.contains("\"ws281x:local:IO18\""),
        "could not retarget the oracle project's endpoint to IO18"
    );
    std::fs::write(&output, retargeted).expect("output.json");
    to
}

fn shipped(test: &str) -> Option<PathBuf> {
    test_support::fw_esp32v3_image()
        .map_err(|reason| test_support::skip_notice(test, &reason))
        .ok()
}

fn merged(test: &str) -> Option<PathBuf> {
    test_support::merged_chip_image()
        .map_err(|reason| test_support::skip_notice(test, &reason))
        .ok()
}

fn frame_dump(test: &str) -> Option<PathBuf> {
    test_support::fw_esp32v3_frame_dump_image()
        .map_err(|reason| test_support::skip_notice(test, &reason))
        .ok()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lp-cli-emu-v3-link-gates-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}

/// A console as one text, every line newline-terminated (so the last line is
/// a complete one to [`summaries`]).
fn console_text(console: &[String]) -> String {
    let mut text = console.join("\n");
    text.push('\n');
    text
}

/// The decimal number following `key` in `line`.
fn number(line: &str, key: &str) -> u64 {
    let at = line
        .find(key)
        .unwrap_or_else(|| panic!("no `{key}` in {line}"))
        + key.len();
    line[at..]
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|d| d.parse().ok())
        .unwrap_or_else(|| panic!("no number after `{key}` in {line}"))
}

fn sha256_of(path: &Path) -> String {
    hex(&Sha256::digest(std::fs::read(path).expect("the dump file")))
}

/// FNV-1a, 32-bit — the oracle's `crc=` and the firmware's `frame_checksum`.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// The last lines of a console, for a failure message.
fn tail(console: &[String]) -> String {
    let from = console.len().saturating_sub(40);
    console[from..].join("\n")
}

/// Drive a future whose every await completes synchronously (the host steps
/// the board inside `receive`): tests are edges, and a null waker is enough.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}

// The report-group and pin-log readers' own tests, moved with them from
// `lp-emu-esp32v3/tests/five_wires.rs`.

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

/// The dump-parts reader, on the shape the socket-hosted walk lost: the
/// black dump whole, the lit one missing its first part.
#[test]
fn the_dump_parts_reader_counts_each_dumps_arrived_parts() {
    let console = "[INFO] f: [OUT] dump frame=1 leds=64 shown=64 crc=0x75a104c5 part=1/2 rgb=00\n\
         [INFO] f: [OUT] dump frame=1 leds=64 shown=64 crc=0x75a104c5 part=2/2 rgb=00\n\
         [INFO] f: [OUT] frame=60 leds=64 crc=0x55772254 lit=64 first=(50,74,2)\n\
         [INFO] f: [OUT] dump frame=31 leds=64 shown=64 crc=0x55772254 part=2/2 rgb=4c2d05\n";
    assert_eq!(dump_parts(console), vec![(1, 2, 2), (31, 1, 2)]);
}

/// **The defect's own stream**: the deadline lands inside frame 1980's
/// burst, after the first line and part-way through the second. The cut line
/// is not read, and every wire has reached 1980 — which a per-wire highest
/// line would have read as 1920 for three of them.
#[test]
fn a_report_burst_the_run_stopped_inside_is_in_flight_not_lost() {
    let mut console = groups_through(1920, &WIRES);
    console.push_str(&line(1980, WIRES[0]));
    // The second line of the burst, cut in its `first=` list …
    console.push_str(
        "[INFO] fw_esp32v3::output::rmt::frame_dump: [OUT] frame=1980 leds=16 \
         crc=0xda7f5d46 lit=16 first=(100,1,87)",
    );
    let parsed = summaries(&console);
    assert_eq!(parsed.len(), 32 * 5 + 1, "the cut line is not a report");
    assert_eq!(reached(&parsed, &WIRES), Ok(1980));
    // … and cut in its checksum, where reading it would invent a crc.
    let mut console = groups_through(1920, &WIRES);
    console.push_str(&line(1980, WIRES[0]));
    console.push_str("[INFO] f: [OUT] frame=1980 leds=16 crc=0xda7f");
    assert_eq!(reached(&summaries(&console), &WIRES), Ok(1980));
}

/// **The teeth.** A report missing from a group that later groups followed,
/// a line missing from the middle of the last burst, and a whole missing
/// group are all losses, and each is refused by name.
#[test]
fn a_withheld_report_is_still_a_lost_one() {
    // gpio14's frame-1920 line withheld, 1980's burst whole after it.
    let mut console = groups_through(1860, &WIRES);
    for crc in [WIRES[0], WIRES[1], WIRES[3], WIRES[4]] {
        console.push_str(&line(1920, crc));
    }
    for crc in WIRES {
        console.push_str(&line(1980, crc));
    }
    let lost = reached(&summaries(&console), &WIRES).unwrap_err();
    assert!(
        lost.contains("frame 1920") && lost.contains("a report was lost"),
        "{lost}"
    );

    // The last burst with its third line missing and the fourth present:
    // not a burst the run stopped inside.
    let mut console = groups_through(1920, &WIRES);
    for crc in [WIRES[0], WIRES[1], WIRES[3]] {
        console.push_str(&line(1980, crc));
    }
    let lost = reached(&summaries(&console), &WIRES).unwrap_err();
    assert!(
        lost.contains("frame 1980") && lost.contains("not cut off"),
        "{lost}"
    );

    // A whole group gone: 1860 then 1980.
    let mut console = groups_through(1860, &WIRES);
    for crc in WIRES {
        console.push_str(&line(1980, crc));
    }
    let lost = reached(&summaries(&console), &WIRES).unwrap_err();
    assert!(
        lost.contains("frame 1980 follows the group of frame 1860"),
        "{lost}"
    );

    // A wire reporting twice in one finished group is not "one line per wire".
    let mut console = groups_through(1860, &WIRES);
    for crc in [WIRES[0], WIRES[1], WIRES[1], WIRES[3], WIRES[4]] {
        console.push_str(&line(1920, crc));
    }
    console.push_str(&line(1980, WIRES[0]));
    assert!(reached(&summaries(&console), &WIRES).is_err());
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

#[test]
fn the_dump_reader_takes_the_deferred_lit_dump_and_not_the_open_one() {
    let text = "[OUT] dump frame=1 leds=2 shown=2 crc=0x0 part=1/2 rgb=000000\n\
                [OUT] dump frame=1 leds=2 shown=2 crc=0x0 part=2/2 rgb=000000\n\
                [OUT] dump frame=31 leds=2 shown=2 crc=0x1 part=1/2 rgb=abcdef\n\
                [OUT] dump frame=31 leds=2 shown=2 crc=0x1 part=2/2 rgb=123456\n";
    assert_eq!(last_dump_rgb(text).as_deref(), Some("abcdef123456"));
    let torn = "[OUT] dump frame=1 leds=2 shown=2 crc=0x0 part=1/2 rgb=000000\n";
    assert_eq!(
        last_dump_rgb(torn),
        None,
        "a dump whose parts did not all arrive"
    );
}

#[test]
fn fnv1a_matches_the_published_vectors() {
    assert_eq!(fnv1a(b""), 0x811c_9dc5);
    assert_eq!(fnv1a(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
}

/// The five checksums of the defect's run, in output order.
const WIRES: [u32; 5] = [
    0x19e6_e98d,
    0xda7f_5d46,
    0x9da2_5dce,
    0x373a_63e9,
    0xe2b7_45a3,
];

/// One complete summary line, as `frame_dump::report` prints it.
fn line(n: usize, crc: u32) -> String {
    format!(
        "[INFO] fw_esp32v3::output::rmt::frame_dump: [OUT] frame={n} leds=16 crc=0x{crc:08x} \
         lit=16 first=(1,2,3)\n"
    )
}

/// Every report group from frame 60 through `through`, whole and in order.
fn groups_through(through: usize, wires: &[u32]) -> String {
    (1..=through / REPORT_EVERY_FRAMES)
        .flat_map(|k| {
            wires
                .iter()
                .map(move |crc| line(k * REPORT_EVERY_FRAMES, *crc))
        })
        .collect()
}
