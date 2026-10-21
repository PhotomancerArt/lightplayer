//! The shipped S3 image's chip gates that need a host on its link — the
//! host-side twin of `lp-emu-esp32s3/tests/boot_idle.rs`'s elicited ledger
//! triple (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D11).
//!
//! Since wire proto 30 the S3 speaks lp-link on its USB-Serial-JTAG port,
//! which IS its console: a request only reaches the server once a host has
//! brought the link up, and nothing under `lp-emu/` may host one (the MIT
//! fence). So the claim that needs a request on the wire — the `[stack]` /
//! `[MEM]` / `[JIT]` triple, which this image prints only when asked (a
//! project load or unload, a stop-all, `runtime_status`; never on the
//! five-second heartbeat) — is made here, through lp-cli's in-process link
//! host (`lp_cli::commands::emu::link_host`, the product's own
//! `lpc_wire::WireLinkPort`).
//!
//! `#[ignore]`d: it needs `LP_EMU_ESP32S3_ELF` and `LP_EMU_ESP32S3_MERGED`,
//! which `just test-emu-esp32s3-boot` builds and exports before it runs
//! this. Numbers printed are `lp-emu:esp32s3:t1`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{EmuLinkHost, S3Board};
use lp_emu_esp_figures::Figures;
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{AppSource, Esp32S3Builder, UsbHost};
use lp_emu_esp32s3::test_support;
use lpc_wire::{ClientMessage, ClientRequest};

/// Long enough for the boot (the server loop by ~120 ms of guest time on a
/// formatting first boot), the link to come up and the stop-all's answer.
const GATE_US: u64 = 2_000_000;

/// **The ledger triple, elicited** by a `stopAllProjects` over the link, the
/// smallest request that reaches `handlers::handle_stop_all_projects`'s
/// `log_memory`.
///
/// Two facts about the triple are structural rather than measured, and are
/// asserted as such: `[MEM] … retry_saves=0` (this image has no OOM retry
/// allocator) and a `[JIT]` line of zeros (the S3 JITs out of the
/// `esp_alloc` heap through SRAM1's I-bus alias, so it has no reserved code
/// region and `cap=0` is literally true). The stack's total is a figure,
/// `stack_total_bytes` in `lp-emu/esp/figures/esp32s3.json`
/// (`just bless-chips esp32s3`).
///
/// What changed with the move (was `boot_idle.rs`'s
/// `the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire`): the request
/// goes out once the board's hello has arrived on the link, not 1 ms after
/// `[INIT] I/O task spawned` as a scripted `M!` line — the same request, a
/// little later in the boot; and the reply is a link message, so the
/// one-buffer mechanism (nothing refused) is `boot_idle.rs`'s claim on its
/// own paths rather than this one's.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn the_ledger_triple_is_elicited_by_a_stop_all_over_the_link() {
    let (elf, merged) = match (
        test_support::fw_esp32s3_image(),
        test_support::merged_chip_image(),
    ) {
        (Ok(elf), Ok(merged)) => (elf, merged),
        (Err(reason), _) | (_, Err(reason)) => {
            test_support::skip_notice(
                "the_ledger_triple_is_elicited_by_a_stop_all_over_the_link",
                &reason,
            );
            return;
        }
    };
    let builder = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .strict(true);
    let board = S3Board::build(builder, UsbHost::Attached { draining: true })
        .expect("the shipped image direct-loads");
    let mut host = EmuLinkHost::new(board, 0x5E55_0301, true);

    let hello = host.wait_for_line("\"hello\":{", GATE_US).expect("the run");
    assert!(
        hello.is_some(),
        "no hello on the link:\n{}",
        host.console().join("\n")
    );
    const ID: u64 = 1;
    host.send(&ClientMessage {
        id: ID,
        msg: ClientRequest::StopAllProjects,
    })
    .expect("the link takes the request");
    let answer = host
        .wait_for_line(&format!("M!{{\"id\":{ID},"), GATE_US)
        .expect("the run");
    let text = host.console().join("\n");
    assert!(answer.is_some(), "the stop-all was never answered:\n{text}");
    // The triple is raw esp-println text and arrives at once; the handler's
    // own log lines ride the log channel and can trail the reply.
    host.wait_for_line("Stopped all projects", 500_000)
        .expect("the run");
    let text = host.console().join("\n");
    assert!(
        host.board.machine.first_strict_violation().is_none(),
        "no strict stop"
    );

    // The request reached the server loop and was handled.
    assert!(
        text.contains("Stopping all projects (0 loaded)"),
        "the request was read and handled:\n{text}"
    );
    assert!(text.contains("Stopped all projects"), "{text}");

    // The triple, in the shapes P05 named — wherever a line starts (a raw
    // line or a log record's text) — with the numbers read, not pinned.
    let find = |prefix: &str| -> Vec<String> {
        host.console()
            .iter()
            .filter_map(|l| l.find(prefix).map(|at| l[at..].to_string()))
            .collect()
    };
    let stack = find("[stack] heartbeat: high-water ");
    let mem = find("[MEM] free=");
    let jit = find("[JIT] used=");
    // The probe logs a `[stack]` line only when the mark grows: once at the
    // first ledger, and again at the second when the stop-all's own path
    // went deeper than everything before it (since the S3's split door,
    // boot is no longer the deepest path).
    assert!(
        matches!(stack.len(), 1 | 2),
        "one [stack] line per ledger that raised the mark:\n{text}"
    );
    assert_eq!(mem.len(), 2, "[MEM] before and after the stop:\n{text}");
    assert_eq!(jit.len(), 2, "[JIT] before and after the stop:\n{text}");
    // `[stack] heartbeat: high-water <used> B of <total> B (<headroom> B headroom)`
    let parse = |line: &str| -> (u32, u32) {
        let words: Vec<&str> = line.split_whitespace().collect();
        let used: u32 = words[3].parse().expect("high-water bytes");
        assert_eq!(&words[4..6], &["B", "of"], "{line}");
        let total: u32 = words[6].parse().expect("the stack's total");
        let headroom: u32 = words[8]
            .trim_start_matches('(')
            .parse()
            .expect("headroom bytes");
        assert_eq!(used + headroom, total, "{line}");
        assert!(used > 0 && used < total, "{line}");
        (used, total)
    };
    let marks: Vec<(u32, u32)> = stack.iter().map(|l| parse(l)).collect();
    let total = marks[0].1;
    assert!(marks.iter().all(|(_, t)| *t == total), "{stack:?}");
    assert!(
        marks.windows(2).all(|w| w[1].0 > w[0].0),
        "a later line only when the mark grew: {stack:?}"
    );
    let mut figures = Figures::new(
        "esp32s3",
        "emu_s3_link_gates::the_ledger_triple_is_elicited_by_a_stop_all_over_the_link",
    );
    figures.int("stack_total_bytes", total);
    figures.verify();
    for line in &mem {
        assert!(
            line.contains(" used=") && line.contains(" largest_free="),
            "{line}"
        );
        assert!(
            line.ends_with(" retry_saves=0"),
            "structural: no OOM retry allocator in this image: {line}"
        );
    }
    for line in &jit {
        assert_eq!(
            line,
            "[JIT] used=0 peak=0 cap=0 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 \
             largest_free=0",
            "structural: no reserved code region; JIT residency is inside [MEM] used"
        );
    }
    // The one boot line P04b's note fixes: the `.bss` arena (HEAP_SIZE =
    // 240 * 1024) plus `dram2_seg` (73,744 B, RAM research E14), a two-region
    // sum where the classic prints a four-region one.
    assert!(
        text.contains("[INIT] chip=esp32s3 arch=xtensa heap=245760+73744=319504"),
        "HEAP_SIZE + dram2_seg: {text}"
    );
    assert!(
        !text.contains("[INIT] main stack"),
        "the S3 prints no `main stack` line; its total is in every `[stack]` line's \
         `of <total> B` instead"
    );
    let counters = host.counters();
    assert_eq!(host.link_errors, 0, "{text}");
    assert_eq!(counters.payload_errors, 0);
    println!(
        "LEDGER TRIPLE (lp-emu:esp32s3:t1), elicited by a stop-all over the link:\n  {}\n  {}\n  \
         {}\nhost link {} frames out / {} in, {} resent",
        stack[stack.len() - 1], mem[1], jit[1], counters.frames_tx, counters.frames_rx, counters.resends
    );
}

/// Hellos the boot check sends once the link is up.
const BOOT_REQUESTS: u64 = 10;

/// **Boot has two writers on the IN endpoint, and neither is refused**: the
/// S3's twin of the boot half of `emu_usb_free_lag.rs`.
///
/// Since the S3's link task got a thread of its own (`io-thread`, plan
/// `lp2025/2026-10-02-1918-io-thread-other-boards`, P3) the link thread
/// sends its SYNs while the main thread is still printing the boot text raw
/// through esp-println, so "one writer on the IN endpoint" holds only once
/// boot is over. The IN-endpoint gate (`fw_esp32_common::serial::in_endpoint`)
/// is what keeps a link packet off a buffer esp-println has filled, and the
/// model refuses — onto its `tried` stream — any byte written while the
/// buffer is not free. So, with the host attached from power-on: nothing is
/// refused over the boot and a short conversation, the boot text arrives
/// whole beside the link, the link never resets, no frame arrives damaged,
/// and every request is answered. The claims the C6's test makes of its
/// shipped image, no more.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn the_boot_text_and_the_link_thread_share_the_in_endpoint_cleanly() {
    let (elf, merged) = match (
        test_support::fw_esp32s3_image(),
        test_support::merged_chip_image(),
    ) {
        (Ok(elf), Ok(merged)) => (elf, merged),
        (Err(reason), _) | (_, Err(reason)) => {
            test_support::skip_notice(
                "the_boot_text_and_the_link_thread_share_the_in_endpoint_cleanly",
                &reason,
            );
            return;
        }
    };
    let builder = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .strict(true);
    let board = S3Board::build(builder, UsbHost::Attached { draining: true })
        .expect("the shipped image direct-loads");
    let mut host = EmuLinkHost::new(board, 0x5E55_0302, true);

    let hello = host.wait_for_line("\"hello\":{", GATE_US).expect("the run");
    assert!(
        hello.is_some(),
        "no hello on the link:\n{}",
        host.console().join("\n")
    );
    for n in 0..BOOT_REQUESTS {
        let id = 100 + n;
        host.send(&ClientMessage {
            id,
            msg: ClientRequest::Hello,
        })
        .expect("the link takes the request");
        let answer = host
            .wait_for_line(&format!("M!{{\"id\":{id},"), GATE_US)
            .expect("the run");
        assert!(
            answer.is_some(),
            "request {id} was never answered:\n{}",
            host.console().join("\n")
        );
    }

    let text = host.console().join("\n");
    assert!(
        host.board.machine.first_strict_violation().is_none(),
        "no strict stop"
    );
    // The boot text, whole, beside the link: the line the main thread prints
    // as it starts the thread, and the boot's last raw line, printed while
    // the thread is already running.
    for line in [
        "[INIT] io thread: stack 4096 B, priority 1, core 0",
        "[INIT] fw-esp32 initialized, starting server loop",
    ] {
        assert!(
            host.console().iter().any(|l| l.starts_with(line)),
            "boot line {line:?} missing or torn:\n{text}"
        );
    }
    let tried = host.board.machine.usb_sj_tried();
    assert!(
        tried.is_empty(),
        "{} B refused by the IN endpoint (a write onto a busy buffer): {:?}",
        tried.len(),
        String::from_utf8_lossy(&tried)
    );
    let counters = host.counters();
    assert_eq!(counters.damaged, 0, "{counters:?}");
    assert_eq!(counters.resets.total, 0, "the link reset: {counters:?}");
    assert_eq!(counters.payload_errors, 0, "{counters:?}");
    assert_eq!(host.link_errors, 0, "{text}");
    println!(
        "BOOT, TWO WRITERS (lp-emu:esp32s3:t1): 0 B refused, {BOOT_REQUESTS} of \
         {BOOT_REQUESTS} answered; host link {} frames out / {} in, {} resent, 0 damaged, \
         0 resets",
        counters.frames_tx, counters.frames_rx, counters.resends
    );
}

/// **A requested reboot reboots** (S3 radio M1): a `Reboot` over the link is
/// answered, then the firmware's `reset_now` → `esp_hal::system::software_reset`
/// → the ROM's `software_reset` writes `RTC_CNTL.options0.sw_sys_rst`, and the
/// machine performs it — before M1 the write was dropped and the guest spun
/// after it for ever.
///
/// What the second boot must show is what silicon gives: `reset_state`
/// reads `CoreSw` (3), which the firmware's recovery maps to
/// `cause=software-reset`, and RTC memory survived — the recovery ledger in
/// RTC fast memory says the first boot completed (`prior_boot_complete=true`
/// after a boot that served frames) and counts the reboot as a clean one
/// (`level=green`).
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF and LP_EMU_ESP32S3_MERGED; run through `just test-emu-esp32s3-boot`"]
fn a_requested_reboot_reboots_with_the_software_reset_cause() {
    let (elf, merged) = match (
        test_support::fw_esp32s3_image(),
        test_support::merged_chip_image(),
    ) {
        (Ok(elf), Ok(merged)) => (elf, merged),
        (Err(reason), _) | (_, Err(reason)) => {
            test_support::skip_notice(
                "a_requested_reboot_reboots_with_the_software_reset_cause",
                &reason,
            );
            return;
        }
    };
    let builder = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .strict(true);
    let board = S3Board::build(builder, UsbHost::Attached { draining: true })
        .expect("the shipped image direct-loads");
    let mut host = EmuLinkHost::new(board, 0x5E55_0303, true);

    let hello = host.wait_for_line("\"hello\":{", GATE_US).expect("the run");
    assert!(
        hello.is_some(),
        "no hello on the link:\n{}",
        host.console().join("\n")
    );
    let first_boot = host.console().len();
    host.send(&ClientMessage {
        id: 7,
        msg: ClientRequest::Reboot,
    })
    .expect("the link takes the request");
    let restarted = host
        .wait_for_line("[link] reset (PeerRestarted)", GATE_US)
        .expect("the reboot");
    assert!(
        restarted.is_some(),
        "the host never saw the board restart:\n{}",
        host.console().join("\n")
    );
    let second = host
        .wait_for_line("[RECOVERY] boot: cause=", GATE_US)
        .expect("the second boot");
    let text = host.console()[first_boot..].join("\n");
    assert!(second.is_some(), "no second boot:\n{text}");
    assert_eq!(host.board.machine.reboots(), 1, "one software reboot");
    assert!(
        host.board.machine.first_strict_violation().is_none(),
        "a strict refusal: {:?}",
        host.board.machine.first_strict_violation()
    );
    assert!(
        text.contains("[REBOOT] client requested a restart"),
        "the request reached the server:\n{text}"
    );
    assert!(
        text.contains(
            "[RECOVERY] boot: cause=software-reset level=green safe_mode=false \
             prior_boot_complete=true"
        ),
        "the second boot reads CoreSw and finds the first boot's ledger in RTC memory:\n{text}"
    );
    let hello_again = host.wait_for_line("\"hello\":{", GATE_US).expect("the run");
    assert!(hello_again.is_some(), "no hello after the reboot:\n{text}");
}

/// **The radio costs the render nothing it can see** (S3 radio M1, AC1):
/// `projects/test/shader-oracle` pushed over the link to the shipped image
/// and to the radio probe image (`radio_boot_probe`: Wi‑Fi and BTDM started
/// beside the same boot) lights pad 9 — `D10` on the XIAO ESP32-S3 Plus
/// profile both images fall back to — with the **same frame**, byte for
/// byte, for ten whole frames. The radio image boots strict with nothing
/// unmapped on the way.
///
/// `shader-oracle` is clock-free, so its lit frame is one frame and the
/// first whole lit frame on each side is the comparison. Frames are the
/// machine's own decode of the pad (`lp-emu:esp32s3:t1`), not silicon.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF, LP_EMU_ESP32S3_MERGED, LP_EMU_ESP32S3_RADIO_ELF and LP_EMU_ESP32S3_RADIO_MERGED; run through `just test-emu-esp32s3-boot`"]
fn the_radio_image_renders_the_shipped_images_frame() {
    const TEST: &str = "the_radio_image_renders_the_shipped_images_frame";
    let images = (|| {
        Ok::<_, String>((
            test_support::fw_esp32s3_image()?,
            test_support::merged_chip_image()?,
            test_support::radio_image()?,
            test_support::radio_merged_image()?,
        ))
    })();
    let (elf, merged, radio_elf, radio_merged) = match images {
        Ok(images) => images,
        Err(reason) => {
            test_support::skip_notice(TEST, &reason);
            return;
        }
    };
    let shipped = oracle_render("shipped", &elf, &merged);
    let radio = oracle_render("radio", &radio_elf, &radio_merged);
    for line in [
        "[RADIO] ble HCI event: [04, 0e, 04, 05, 03, 0c, 00]",
        "[RADIO] wifi scan done: 0 access points",
    ] {
        assert!(
            radio.console.contains(line),
            "the radio image started its radio: no `{line}`"
        );
    }
    assert!(
        !shipped.console.contains("[RADIO]"),
        "the shipped image starts no radio"
    );
    assert_eq!(
        hex(&radio.first_lit),
        hex(&shipped.first_lit),
        "the radio image's first lit frame on pad {ORACLE_PAD} is the shipped image's"
    );
    println!(
        "ORACLE FRAME (lp-emu:esp32s3:t1) on pad {ORACLE_PAD}, {} bytes, identical: {}\n  \
         shipped: {} frames, {} whole lit, by {} us\n  radio:   {} frames, {} whole lit, \
         by {} us",
        shipped.first_lit.len(),
        hex(&shipped.first_lit),
        shipped.frames,
        shipped.whole_lit,
        shipped.at_us,
        radio.frames,
        radio.whole_lit,
        radio.at_us
    );
}

/// `D10` on the XIAO ESP32-S3 Plus profile (`scripts/emu/m4-walk.sh`'s S3
/// arm, `LP_WALK_PAD` 9).
const ORACLE_PAD: u8 = 9;

/// The most either image may take to boot, take the deploy, compile and
/// light [`LIT_FRAMES`] whole frames. A ceiling, not a duration: the run
/// stops as soon as it has its frames (checked every [`ORACLE_STEP_US`]),
/// because rendering past them proves nothing more and costs the S3's CI
/// job minutes it does not have (`docs/debt/emulator-ci-jobs-outgrow-their-time-budgets.md`).
const ORACLE_US: u64 = 6_000_000;

/// How often the run stops to count lit frames.
const ORACLE_STEP_US: u64 = 100_000;

/// Whole lit frames each side must reach.
const LIT_FRAMES: usize = 10;

struct OracleRender {
    console: String,
    /// Emulated microseconds at which the run had its frames.
    at_us: u64,
    first_lit: Vec<u8>,
    frames: usize,
    whole_lit: usize,
}

fn oracle_render(tag: &str, elf: &Path, merged: &Path) -> OracleRender {
    let builder = Esp32S3Builder::new()
        .app(AppSource::Path(elf.to_path_buf()))
        .flash(FlashBacking::Copy(merged.to_path_buf()))
        .strict(true);
    let board = S3Board::build(builder, UsbHost::Attached { draining: true })
        .unwrap_or_else(|e| panic!("{tag}: the image direct-loads: {e}"));
    let mut host = EmuLinkHost::new(board, 0x5E55_0304, true);
    deploy(&mut host, &repo_root().join("projects/test/shader-oracle"));
    let lit_so_far = |m: &lp_emu_esp32s3::machine::Machine| {
        m.frames(ORACLE_PAD)
            .iter()
            .filter(|f| f.is_complete() && f.wire.iter().any(|b| *b != 0))
            .count()
    };
    while host.board.machine.micros() < ORACLE_US && lit_so_far(&host.board.machine) < LIT_FRAMES {
        let next = host.board.machine.micros() + ORACLE_STEP_US;
        host.run_until(next, None)
            .unwrap_or_else(|e| panic!("{tag}: the run stops only at its deadline: {e}"));
    }
    let console = host.console().join("\n");
    let at_us = host.board.machine.micros();
    let m = &mut host.board.machine;
    m.flush_frames();
    assert!(
        m.first_strict_violation().is_none(),
        "{tag}: a strict refusal: {:?}",
        m.first_strict_violation()
    );
    assert_eq!(
        m.bus().unmapped_reads() + m.bus().unmapped_writes(),
        0,
        "{tag}: unmapped"
    );
    assert_eq!(host.link_errors, 0, "{tag}: {console}");
    assert!(!console.contains("PANIC"), "{tag}: a panic:\n{console}");
    let frames = host.board.machine.frames(ORACLE_PAD);
    let bit_errors: u64 = frames.iter().map(|f| f.error_count).sum();
    assert_eq!(bit_errors, 0, "{tag}: bit errors on pad {ORACLE_PAD}");
    let lit: Vec<&lp_emu_esp_common::strip::ws281x::Frame> = frames
        .iter()
        .filter(|f| f.is_complete() && f.wire.iter().any(|b| *b != 0))
        .collect();
    assert!(
        lit.len() >= LIT_FRAMES,
        "{tag}: at least {LIT_FRAMES} whole lit frames on pad {ORACLE_PAD} by {ORACLE_US} us, \
         got {} of {}:\n{console}",
        lit.len(),
        frames.len()
    );
    OracleRender {
        first_lit: lit[0].wire.clone(),
        frames: frames.len(),
        whole_lit: lit.len(),
        at_us,
        console,
    }
}

/// Wait for the board's hello, then deploy `dir` over the link as `lp-cli
/// upload` deploys it (the classic's gate's helper, on this chip's board).
fn deploy(host: &mut EmuLinkHost<S3Board>, dir: &Path) {
    let hello = host
        .wait_for_line("\"hello\":{", GATE_US)
        .expect("the boot");
    assert!(hello.is_some(), "no hello:\n{}", host.console().join("\n"));
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
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli sits under the repo root")
        .to_path_buf()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
