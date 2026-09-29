//! The shipped C6 image's chip gates that need a host on its link — the
//! host-side twins of `lp-emu-esp32c6`'s `usb_attached`, `usb_control` and
//! `shader_oracle_pin` (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`,
//! D11).
//!
//! Since wire proto 30 the image speaks lp-link on USB-Serial-JTAG: past the
//! boot text, everything is frames, the board's log lines ride the log
//! channel, and its hello and heartbeats go out only once a host has brought
//! the link up. The emulator's own gates stop at the port because nothing
//! under `lp-emu/` may host a link (the MIT fence); what they used to read
//! off the wire is read here, through lp-cli's in-process host
//! (`lp_cli::commands::emu::link_host`: the product's own
//! `lpc_wire::WireLinkPort`, serviced between 250 us slices of emulated time,
//! with every wire message rendered back as its `M!{json}` console line).
//!
//! - **G2-1** attached and draining from boot: the boot, the link coming up,
//!   the hello, the boot lines, one heartbeat and the stack line, in order,
//!   with the `hello.proto` and `heartbeat.total_bytes` figures
//!   (`lp-emu/esp/figures/esp32c6.json`, re-recorded by `just bless-chips
//!   esp32c6`).
//! - **G2-4** two hosted runs are the same run.
//! - **G3-1** the cable comes out at 6 s and goes back in at 9 s: the link
//!   carries on by itself (measured: the session survives a 3.5 s unplug
//!   with nothing in flight — no reset, no resend), the 10 s heartbeat
//!   arrives, and a request after the replug is answered, with no message
//!   reaching the app damaged.
//! - **G4-1** `projects/test/shader-oracle`, uploaded over the link as
//!   `lp-cli upload` deploys it: the first lit frame off gpio18 is the host
//!   oracle's frame, under t1 and t2, and two runs dump the same frames.
//!
//! `#[ignore]`d: they need a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`),
//! and `just test-emu-c6-cli` runs them. Numbers printed are
//! `lp-emu:esp32c6:t1` (or `:t2`).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::pins::{PadId, RouteSource, SignalId};
use lp_emu_esp_common::strip::ws281x::{Frame, unpermute};
use lp_emu_esp_figures::Figures;
use lp_emu_esp32c6::control::ControlCommand;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::machine::{AppSource, Esp32C6Builder, TimeGrade, UsbHost, hex};
use lp_emu_esp32c6::memmap;
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lp_ws281x::ColorOrder;
use lpc_wire::{ClientMessage, ClientRequest};

/// One fixed host nonce, so two runs are the same run.
const NONCE: u32 = 0x4057_C6A7;

/// G2-1's markers in the decoded console, in order. A marker with a figure
/// key is followed by that figure's digits.
const CONSOLE_IN_ORDER: &[(&str, Option<&str>)] = &[
    ("[INIT] Initializing board...", None),
    ("starting server loop... proto=", None),
    ("[link] up (session 0)", None),
    (
        "M!{\"id\":0,\"msg\":{\"hello\":{\"proto\":",
        Some("hello.proto"),
    ),
    ("\"boardId\":\"seeed/xiao-esp32-c6\"", None),
    ("\"baseMac\":\"a0:f2:62:87:b4:8c\"", None),
    (
        "Esp32C6RmtWs281xDriver: 2 WS281x channels for 2 declared",
        None,
    ),
    ("ESP-NOW radio ready", None),
    ("[RECOVERY] boot complete", None),
    ("M!{\"id\":0,\"msg\":{\"heartbeat\":{", None),
    ("\"totalBytes\":", Some("heartbeat.total_bytes")),
    ("[stack] heartbeat: high-water", None),
];

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn g2_1_a_host_on_the_link_hears_the_boot_the_hello_and_a_heartbeat_in_order() {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let mut host = hosted(&elf, TimeGrade::T1, Vec::new());
    host.run_until(5_500_000, None).expect("the run");
    let console = host.console().join("\n");

    let mut figures = Figures::new(
        "esp32c6",
        "emu_usb_link_gates::g2_1_a_host_on_the_link_hears_the_boot_the_hello_and_a_heartbeat_in_order",
    );
    let mut from = 0;
    for (marker, figure) in CONSOLE_IN_ORDER {
        let at = console[from..]
            .find(marker)
            .unwrap_or_else(|| panic!("{marker:?} not after byte {from} in:\n{console}"));
        from += at + marker.len();
        if let Some(key) = figure {
            figures.int(key, leading_int(&console[from..]));
        }
    }
    figures.verify();
    assert_eq!(
        console.matches("\"heartbeat\":{").count(),
        1,
        "one heartbeat in 5.5 s"
    );
    let counters = host.counters();
    assert_eq!(host.link_errors, 0, "{console}");
    assert_eq!(counters.resets.total, 0, "one session, start to end");
    assert_eq!(counters.payload_errors, 0);
    let free = console
        .split("\"freeBytes\":")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .expect("freeBytes in the heartbeat");
    println!(
        "G2-1 (lp-emu:esp32c6:t1): {} console lines; freeBytes at 5 s = {free} with the link \
         up and packed replies — recorded, not gated (the ratchet is heap-budget-check's); \
         host link {} frames out / {} in",
        host.console().len(),
        counters.frames_tx,
        counters.frames_rx
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn g2_4_two_hosted_runs_are_the_same_run() {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let run = || {
        let mut host = hosted(&elf, TimeGrade::T1, Vec::new());
        host.run_until(5_500_000, None).expect("the run");
        (
            host.console().to_vec(),
            host.board.machine.cycles(),
            host.board.machine.instructions(),
        )
    };
    let (a, b) = (run(), run());
    assert_eq!(a.0, b.0, "two hosted runs heard different consoles");
    assert_eq!((a.1, a.2), (b.1, b.2), "cycles and instructions");
    println!(
        "G2-4: {} console lines, {} cycles, {} instructions — both runs",
        a.0.len(),
        a.1,
        a.2
    );
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn g3_1_the_cable_comes_out_and_goes_back_in_and_the_link_comes_back() {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let ms = 1_000 * memmap::CYCLES_PER_US;
    let cable = vec![
        (6_000 * ms, ControlCommand::Detach),
        (9_000 * ms, ControlCommand::Attach),
        (9_500 * ms, ControlCommand::Open),
    ];
    let mut host = hosted(&elf, TimeGrade::T1, cable);
    host.run_until(5_900_000, None).expect("before the unplug");
    let before = host.console().len();
    assert!(
        host.console()
            .iter()
            .any(|l| l.contains("\"msg\":{\"heartbeat\":{")),
        "the 5 s heartbeat reached the host before the unplug"
    );
    host.run_until(12_000_000, None)
        .expect("through the unplug");
    let after = &host.console()[before..];
    let hellos = after
        .iter()
        .filter(|l| l.contains("\"msg\":{\"hello\":{"))
        .count();
    let beats = after
        .iter()
        .filter(|l| l.contains("\"msg\":{\"heartbeat\":{"))
        .count();
    println!(
        "G3-1 (lp-emu:esp32c6:t1): after the replug {hellos} hello(s), {beats} heartbeat(s); \
         link notes {:?}",
        after
            .iter()
            .filter(|l| l.starts_with("[link]"))
            .collect::<Vec<_>>()
    );
    assert!(
        beats >= 1,
        "no heartbeat reached the host after the replug:\n{}",
        after.join("\n")
    );

    // A request after the replug is answered: the link is whole again.
    const ID: u64 = 7;
    host.send(&ClientMessage {
        id: ID,
        msg: ClientRequest::Hello,
    })
    .expect("the link takes a request");
    let answer = host
        .wait_for_line(&format!("M!{{\"id\":{ID},"), 3_000_000)
        .expect("the run");
    assert!(
        answer.is_some_and(|l| l.contains("\"hello\":{")),
        "no answer to a hello sent after the replug:\n{}",
        host.console()[before..].join("\n")
    );
    // Nothing the board said reached the app damaged, whatever the link had
    // to do underneath (a reset is the link's own business: it is counted,
    // and a request in flight across one fails at once — D9 — but none was).
    let counters = host.counters();
    assert_eq!(counters.payload_errors, 0);
    println!(
        "G3-1: host link {} resent, {} damaged, {} resets ({} peer restarts, {} retry limit)",
        counters.resends,
        counters.damaged,
        counters.resets.total,
        counters.resets.peer_restarted,
        counters.resets.retry_limit
    );
}

/// `cargo test -p lpa-server --test shader_oracle_frame -- --nocapture`, the
/// same constant `lp-emu-esp32c6/tests/shader_oracle_pin.rs` pinned
/// (wasmtime and rv32-emu agreed on every byte).
const ORACLE_RGB: &str = "324a0208376a1c2889007668098b4b0375544602631253162b0f7051068a838b000097890b63b208a1601b30951b1c72660069af481900a49554e3212b48e41955cdad4d154f047b103e90441ec10ed47200bcb627f657019fb523c13e3794161c952e04a8743b36e681e90e225ef47f09d1174ebc035c8009447f3fb11b6112ca048dc419dd5fae02903ab21f015c6f026047006a750aa45b69b20834c32b7e8f0012913c086a360144365600567c064d430e9127239632148702475f4c2d05";
const ORACLE_CRC: u32 = 0x5577_2254;
const PAD: u8 = 18;
const LEDS: usize = 64;
/// `RMT_SIG_0` (`regs::output_signals`).
const RMT_SIG_0: u16 = 71;
/// The driver's open line for the oracle project: 64 LEDs on D10 = gpio18.
const OPEN_LINE: &str = "Esp32C6RmtWs281xDriver::open: endpoint=esp32c6-rmt-ws281x:ws281x:local:D10 \
     gpio=/gpio/18 ws281x_ch=0 rmt_slot=0 bytes=192";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn g4_1_the_first_lit_frame_off_gpio18_is_the_host_oracles_frame_under_t1() {
    the_first_lit_frame_is_the_oracles(TimeGrade::T1);
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn g4_1_the_first_lit_frame_off_gpio18_is_the_host_oracles_frame_under_t2() {
    the_first_lit_frame_is_the_oracles(TimeGrade::T2);
}

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn g4_1_two_uploads_render_identical_frames() {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let (a, b) = (
        oracle_run(&elf, TimeGrade::T1),
        oracle_run(&elf, TimeGrade::T1),
    );
    assert_eq!(a.console, b.console, "two uploads heard different consoles");
    assert_eq!(a.cycles, b.cycles);
    assert_eq!(a.frames.len(), b.frames.len());
    for (x, y) in a.frames.iter().zip(&b.frames) {
        assert_eq!((x.n, x.start, &x.wire), (y.n, y.start, &y.wire));
    }
    println!(
        "G4-1: two uploads, {} frames each, {} cycles",
        a.frames.len(),
        a.cycles
    );
}

/// G4-1 on one time grade: the oracle project, uploaded over the link, and
/// its first lit frame read back off the pad by the decoder.
fn the_first_lit_frame_is_the_oracles(grade: TimeGrade) {
    let Some(elf) = image(&FwImage::SHIPPED) else {
        return;
    };
    let r = oracle_run(&elf, grade);
    let text = r.console.join("\n");
    assert!(text.contains(OPEN_LINE), "{text}");
    assert!(
        r.routed
            .contains(&(PadId(PAD), RouteSource::Signal(SignalId(RMT_SIG_0), false))),
        "gpio18 is not routed to RMT_SIG_0: {:?}",
        r.routed
    );
    assert_eq!(r.order, ColorOrder::Grb, "the default strip order");

    let frames = &r.frames;
    assert!(!frames.is_empty(), "no frame reached gpio18:\n{text}");
    let lit = frames
        .iter()
        .position(|f| f.wire.iter().any(|b| *b != 0))
        .unwrap_or_else(|| {
            panic!(
                "{} frames on gpio18 and every one of them black — the compile-window \
                 fallback never gave way to a render",
                frames.len()
            )
        });
    let f = &frames[lit];
    assert_eq!(f.leds(), LEDS, "frame {}", f.n);
    assert_eq!(f.error_count, 0, "frame {}: {:?}", f.n, f.errors);
    assert!(f.is_complete(), "frame {} was cut short: {f:?}", f.n);
    let rgb = unpermute(&f.wire, ColorOrder::Grb);
    let decoded = hex(&rgb);
    assert_eq!(
        decoded,
        ORACLE_RGB,
        "frame {}: the first lit frame off gpio18 is not the oracle's frame (wire as \
         carried: {}) — a per-pixel byte swap means the order assumption is wrong, anything \
         else is a compiler or machine finding",
        f.n,
        hex(&f.wire)
    );
    assert_eq!(fnv1a(&rgb), ORACLE_CRC);

    // Every later frame is the same frame; a frame the deadline cut can
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
    // The board's own record of the same frame, over the log channel: the
    // `[OUT] dump` parts, joined, when the image carries `frame-dump`. The
    // shipped image does not, so this is reported only when present.
    println!(
        "G4-1[{grade:?}] (lp-emu:esp32c6): {} frames on gpio18, first lit n={} at {:.3} ms, \
         {later} identical after it; upload done at {:.3} s",
        frames.len(),
        f.n,
        f.start as f64 / memmap::CYCLES_PER_US as f64 / 1_000.0,
        r.uploaded_at_s
    );
}

struct OracleRun {
    console: Vec<String>,
    frames: Vec<Frame>,
    routed: Vec<(PadId, RouteSource)>,
    order: ColorOrder,
    cycles: u64,
    uploaded_at_s: f64,
}

/// Boot, wait for the hello, deploy `projects/test/shader-oracle` over the
/// link as `lp-cli upload` does, then render on for a second and a half.
fn oracle_run(elf: &Path, grade: TimeGrade) -> OracleRun {
    let mut host = hosted(elf, grade, Vec::new());
    let hello = host
        .wait_for_line("\"hello\":{", 3_000_000)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));

    let dir = repo_root().join("projects/test/shader-oracle");
    let (uid, _) = lp_cli::commands::dev::validation::validate_local_project(&dir)
        .expect("the oracle project validates");
    let files = lp_cli::commands::dev::collect_project_deploy_files(&lpfs::LpFsStd::new(dir))
        .expect("the oracle project's files");
    {
        let mut client = lpa_client::LpClient::new(&mut host);
        block_on(client.deploy_project_files(&uid, files))
            .unwrap_or_else(|e| panic!("the deploy failed: {e}"));
    }
    host.set_queue_messages(false);
    let uploaded_at_s = host.board_seconds();
    let until = host.board.machine.micros() + 1_500_000;
    host.run_until(until, None).expect("rendering on");
    host.board.machine.flush_frames();
    assert_eq!(host.link_errors, 0, "{}", host.console().join("\n"));
    assert_eq!(
        host.board.machine.bus.unmapped_reads() + host.board.machine.bus.unmapped_writes(),
        0,
        "unmapped"
    );
    OracleRun {
        console: host.console().to_vec(),
        frames: host.board.machine.frames(PAD).to_vec(),
        routed: host.board.machine.routed_pads(),
        order: host.board.machine.strip().order,
        cycles: host.board.machine.cycles(),
        uploaded_at_s,
    }
}

/// The shipped image on a strict machine, attached and draining from
/// power-on, with this process as the host on its link (a fixed nonce, so a
/// run is a function of the image and the script).
fn hosted(elf: &Path, grade: TimeGrade, cable: Vec<(u64, ControlCommand)>) -> EmuLinkHost<C6Board> {
    let mut builder = Esp32C6Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Blank)
        .strict(true)
        .time_grade(grade)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source();
    if !cable.is_empty() {
        builder = builder.usb_script(cable);
    }
    let machine = builder.build().expect("the shipped image builds a machine");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true)
}

fn image(image: &FwImage) -> Option<PathBuf> {
    match fw_esp32c6_image(image) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_usb_link_gates: skipped — {reason}");
            None
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}

/// The decimal number at the start of `s`.
fn leading_int(s: &str) -> i64 {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    s[..end]
        .parse()
        .unwrap_or_else(|_| panic!("no number at the start of {:?}", &s[..s.len().min(40)]))
}

/// FNV-1a, 32-bit — the oracle's `crc=`.
fn fnv1a(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in data {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
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
