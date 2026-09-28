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

use lp_cli::commands::emu::link_host::{EmuLinkHost, S3Board};
use lp_emu_esp_figures::Figures;
use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{AppSource, Esp32S3Builder};
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
    let board = S3Board::build(builder).expect("the shipped image direct-loads");
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
    assert_eq!(stack.len(), 1, "one [stack] line per stop-all:\n{text}");
    assert_eq!(mem.len(), 2, "[MEM] before and after the stop:\n{text}");
    assert_eq!(jit.len(), 2, "[JIT] before and after the stop:\n{text}");
    // `[stack] heartbeat: high-water <used> B of <total> B (<headroom> B headroom)`
    let words: Vec<&str> = stack[0].split_whitespace().collect();
    let used: u32 = words[3].parse().expect("high-water bytes");
    assert_eq!(&words[4..6], &["B", "of"], "{}", stack[0]);
    let total: u32 = words[6].parse().expect("the stack's total");
    let headroom: u32 = words[8]
        .trim_start_matches('(')
        .parse()
        .expect("headroom bytes");
    assert_eq!(used + headroom, total, "{}", stack[0]);
    assert!(used > 0 && used < total, "{}", stack[0]);
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
    // The one boot line P04b's note fixes: a single-number heap, where the
    // classic prints a four-region sum.
    assert!(
        text.contains("[INIT] chip=esp32s3 arch=xtensa heap=245760"),
        "HEAP_SIZE = 240 * 1024, one number: {text}"
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
        stack[0], mem[1], jit[1], counters.frames_tx, counters.frames_rx, counters.resends
    );
}
