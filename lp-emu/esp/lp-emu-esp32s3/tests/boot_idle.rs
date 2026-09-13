//! **The hello**: the shipped `fw-esp32s3` image, direct-loaded under
//! `--strict-bus`, printing out of its USB-Serial-JTAG link — and the three
//! host states it prints into.
//!
//! M6 P05 carries the first half of the plan's acceptance line 4. What this
//! file pins:
//!
//! - **G5-1** attached-draining from boot: the `[INIT]` chain reaches the
//!   **delivered** stream in order, byte for byte, with its length and sha
//!   pinned; `unmapped == 0`; no strict stop; nothing is merely tried.
//! - **G5-2** host absent: the same run prints into an endpoint nobody
//!   drains. Nothing is delivered, the first commit is on the **observation**
//!   stream, and the console then falls silent for the rest of the run —
//!   which is the state a gate must be able to tell apart from a crash.
//! - **G5-3** attached-idle, then `open`: the committed packet is held until
//!   an application opens the port and arrives after the drain latency.
//! - **G5-4** determinism: two runs of G5-1 deliver identical bytes at
//!   identical cycle counts.
//! - **G5-5** where the boot stops, and whose it is.
//!
//! # ⚠️ Where this boot stops, and why the chain is shorter than the brief
//!
//! The image does **not** reach `[INIT] fw-esp32 initialized, starting
//! server loop` on this machine, and it cannot until **P06**. After
//! `[INIT] I/O task spawned` it calls `mount_filesystem(flash)`, which issues
//! a flash read: `esp_storage` sets `SPI1.cmd` bit 28 (`usr`) and spins until
//! hardware clears it. `SPI1` is an accept block here, so the bit stays set
//! and the spin never ends — which is exactly what
//! `lp_emu_esp32s3::periph::accept::spi1`'s own doc predicted in P04:
//!
//! > a flash read sets `cmd.usr` and spins until hardware clears it when the
//! > transfer is done, and a block that remembers holds it set forever. That
//! > spin is **P06**'s stop, on `engine::spi_flash`.
//!
//! So the `lpfs` mount failure, the memory-filesystem fallback and the
//! server-loop line are **P06's** readings, not P05's — and
//! [`the_boot_stops_where_p06_begins`] pins that they are absent, with the
//! register the run is spinning on, so the phase that changes it is visible.
//!
//! The same block is why the ledger triple (`[stack]`, `[MEM]`, `[JIT]`,
//! added to this image by P04b / PR #742) cannot be **elicited** here: the
//! classic elicits it with a `stop-all` over the wire, and a wire needs the
//! server loop. [`the_ledger_triple_is_not_elicitable_until_the_server_loop_runs`]
//! pins that too, with the shapes P06 will assert.
//!
//! The tests that need a built `fw-esp32s3` are `#[ignore]`d and run by
//! `just test-emu-esp32s3-boot`.

use lp_emu_esp32s3::machine::{
    AppSource, Esp32S3Builder, Machine, Outcome, StopCondition, UsbHost,
};
use lp_emu_esp32s3::periph::usb_sj::IN_DRAIN_LATENCY_CYCLES;
use lp_emu_esp32s3::{memmap, test_support};
use sha2::{Digest, Sha256};

/// Long enough that everything before the flash spin has been printed, short
/// enough that a suite run is seconds: the whole chain is out by ~8.9 ms of
/// guest time and the RWDT's boot stage is 30 s away.
const GATE_US: u64 = 2_000_000;

/// **The hello, byte for byte.** Pinned as a whole stream, as
/// `lp-emu-esp32v3/tests/boot.rs` pins the classic's.
const HELLO: &str = "\
[INIT] fw-esp32s3 boot
[INIT] chip=esp32s3 arch=xtensa heap=245760
[RECOVERY] boot: cause=power-on level=green safe_mode=false prior_boot_complete=true
[RECOVERY] RWDT armed: boot 30000 ms, runtime 8000 ms
[INIT] runtime started
[INIT] I/O task spawned
";

/// [`HELLO`]'s length and sha256, so a change to any byte of it is a failure
/// that names the diff rather than a diff a reader has to spot.
const HELLO_BYTES: usize = 253;
const HELLO_SHA: &str = "da070ac01e73ee4bca64cdf8f18984e8fd377ee91501f2fb019d2aeb7c326e8a";

fn sha(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// A strict run of the shipped image with the host in `host`, for `GATE_US`.
fn run(host: UsbHost) -> Option<(Machine, Outcome)> {
    let elf = test_support::fw_esp32s3_image().ok()?;
    let mut machine = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .usb_host(host)
        .build()
        .expect("the shipped image direct-loads");
    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(GATE_US * memmap::CYCLES_PER_US),
        ..Default::default()
    });
    Some((machine, outcome))
}

/// **G5-1 — the hello.** The shipped image, direct-loaded under
/// `--strict-bus` into a draining host, prints its whole `[INIT]` chain out
/// of the USB-Serial-JTAG **host stream**, with zero unmapped accesses and no
/// strict stop.
///
/// The stream is what a host **received**, never what the guest tried: that
/// distinction is the whole point of the host model, and G5-2 is the other
/// side of it.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn the_shipped_image_prints_its_init_chain_out_of_the_link() {
    let Some((machine, outcome)) = run(UsbHost::Attached { draining: true }) else {
        test_support::skip_notice(
            "the_shipped_image_prints_its_init_chain_out_of_the_link",
            "no image",
        );
        return;
    };
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "no strict stop and no fault: {outcome:?}"
    );
    assert!(machine.first_strict_violation().is_none());
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "unmapped = 0 — the binary condition a transcript can carry"
    );

    let delivered = machine.usb_sj();
    assert_eq!(
        String::from_utf8_lossy(&delivered),
        HELLO,
        "the whole chain, in order, out of the link"
    );
    assert_eq!(delivered.len(), HELLO_BYTES);
    assert_eq!(sha(&delivered), HELLO_SHA);
    assert!(
        machine.usb_sj_tried().is_empty(),
        "a draining host took every byte: nothing was merely tried"
    );
    // The console and the link are the same stream on this chip, so
    // `--console` writes exactly this.
    assert_eq!(machine.console().bytes(), delivered);

    println!(
        "HELLO: {} bytes, sha {}, delivered in {} us of guest time, unmapped=0",
        delivered.len(),
        sha(&delivered),
        machine.cycles() / memmap::CYCLES_PER_US
    );
    print!("{}", String::from_utf8_lossy(&delivered));
}

/// **G5-2 — nobody there.** With no cable the same image prints into an
/// endpoint nothing drains: the first line commits, `free` never comes back,
/// esp-println's 50,000-iteration wait latches, and the console falls silent
/// for the rest of the run.
///
/// Every byte that got as far as the FIFO is on the **observation** stream
/// and none is on the delivered one — which is the reading that lets a gate
/// tell "nobody is listening" from "the firmware crashed".
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn with_no_host_the_console_commits_once_and_falls_silent() {
    let Some((machine, outcome)) = run(UsbHost::Absent) else {
        test_support::skip_notice(
            "with_no_host_the_console_commits_once_and_falls_silent",
            "no image",
        );
        return;
    };
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert!(
        machine.usb_sj().is_empty(),
        "no host, so no byte ever reached one"
    );
    let tried = machine.usb_sj_tried();
    // ⚠️ **22 bytes, not 64.** esp-println's `println!` ends in a flush, so
    // the first commit is the first line *without* its newline — the FIFO
    // never fills. The next print finds `free` still 0, writes `wr_done`
    // into a committed endpoint, spins its 50,000 iterations, latches
    // `TIMED_OUT` and returns at once for ever after; so `[INIT] fw-esp32s3
    // boot` is every byte this firmware ever hands over with nobody there.
    assert_eq!(
        tried,
        b"[INIT] fw-esp32s3 boot",
        "one committed line, then TIMED_OUT latches and the console falls silent: {}",
        String::from_utf8_lossy(&tried)
    );
    assert!(
        HELLO.as_bytes().starts_with(&tried),
        "what was tried is a prefix of what a host would have received"
    );
    println!(
        "HOST ABSENT: 0 bytes delivered, {} bytes tried — the console fell silent after one \
         commit, and the run kept running ({} us, unmapped={})",
        tried.len(),
        machine.cycles() / memmap::CYCLES_PER_US,
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
    );
}

/// **G5-3 — the cable is in, the port is closed.** The first packet commits
/// and is **held**: nothing is delivered and nothing is dropped. An
/// application then opens the port and the packet arrives after the drain
/// latency.
///
/// This is the state `--usb-sj-drain manual` exists for, and the one a Web
/// Serial page is in between `requestPort()` and `open()`.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn an_attached_but_closed_port_holds_the_packet_until_an_application_opens_it() {
    let Ok(elf) = test_support::fw_esp32s3_image() else {
        test_support::skip_notice(
            "an_attached_but_closed_port_holds_the_packet_until_an_application_opens_it",
            "no image",
        );
        return;
    };
    let mut machine = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .strict(true)
        .usb_host(UsbHost::Attached { draining: false })
        .build()
        .expect("the shipped image direct-loads");
    machine.run_until(&StopCondition {
        stop_cycle: Some(GATE_US * memmap::CYCLES_PER_US),
        ..Default::default()
    });
    assert!(
        machine.usb_sj().is_empty(),
        "the port is closed: nothing drains"
    );
    assert!(
        machine.usb_sj_tried().is_empty(),
        "and nothing is dropped either — a held packet is not a lost one"
    );
    assert_eq!(
        machine.usb_host_now(),
        Some(UsbHost::Attached { draining: false })
    );

    // An application opens the port. One control command, applied at the
    // next slice boundary; the packet crosses after the drain latency.
    let reply = machine.apply_control_now(&lp_emu_esp32s3::control::ControlCommand::Open);
    assert!(
        matches!(
            reply,
            lp_emu_esp32s3::control::ControlReply::Ok { verb: "open", .. }
        ),
        "{reply}"
    );
    let at = machine.cycles();
    machine.run_until(&StopCondition {
        stop_cycle: Some(at + 2 * IN_DRAIN_LATENCY_CYCLES),
        ..Default::default()
    });
    let delivered = machine.usb_sj();
    // The one packet the printer committed before it latched — the first
    // line without its newline (see `with_no_host_…` for why 22 and not 64).
    assert_eq!(
        delivered,
        b"[INIT] fw-esp32s3 boot",
        "the held packet, and only it: {}",
        String::from_utf8_lossy(&delivered)
    );
    assert!(HELLO.as_bytes().starts_with(&delivered));
    assert!(
        machine.usb_sj_tried().is_empty(),
        "held, then delivered — never dropped"
    );
    println!(
        "ATTACHED-IDLE → OPEN: {} bytes held for {} us, then delivered in {} us",
        delivered.len(),
        at / memmap::CYCLES_PER_US,
        IN_DRAIN_LATENCY_CYCLES / memmap::CYCLES_PER_US,
    );
}

/// **G5-4 — determinism.** Two runs of the same image with the same host
/// deliver identical bytes at identical cycle counts. Guest time is the only
/// clock in the machine (PD5), so this is a property rather than a hope.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn two_runs_of_the_hello_are_the_same_run() {
    let Some((a, _)) = run(UsbHost::Attached { draining: true }) else {
        test_support::skip_notice("two_runs_of_the_hello_are_the_same_run", "no image");
        return;
    };
    let (b, _) = run(UsbHost::Attached { draining: true }).expect("the image is still there");
    assert_eq!(a.usb_sj(), b.usb_sj(), "identical bytes");
    assert_eq!(a.cycles(), b.cycles(), "at identical cycles");
    assert_eq!(a.instructions(), b.instructions());
    println!(
        "DETERMINISM: {} bytes, {} cycles, {} instructions — twice",
        a.usb_sj().len(),
        a.cycles(),
        a.instructions()
    );
}

/// **G5-5 — where the boot stops, and whose it is.**
///
/// ⚠️ This test asserts the **absence** of three lines, on purpose. The boot
/// ends spinning on `SPI1.cmd` bit 28 (`usr`) — a flash read no hardware here
/// completes — so everything `mount_filesystem` and what follows it would
/// print belongs to **P06**. Pinning the absence is what makes the phase that
/// changes it visible in a diff instead of in a reader's memory.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn the_boot_stops_where_p06_begins() {
    let Some((mut machine, _)) = run(UsbHost::Attached { draining: true }) else {
        test_support::skip_notice("the_boot_stops_where_p06_begins", "no image");
        return;
    };
    let text = String::from_utf8_lossy(&machine.usb_sj()).into_owned();
    for absent in [
        // The `lpfs` mount failure and the memory-filesystem fallback.
        "lpfs",
        // The hardware manifest, printed straight after the mount.
        "hardware manifest",
        // The line `lpa_link::device_session::device_readiness` matches, and
        // which is chip-agnostic on purpose — never "fw-esp32s3".
        "fw-esp32 initialized, starting server loop",
    ] {
        assert!(
            !text.contains(absent),
            "`{absent}` is printed after `mount_filesystem`, which needs the flash engine \
             (P06). If this now appears, P06 has landed and this test is the one to update."
        );
    }

    // The register it is spinning on, read back through the machine's own
    // decode so the citation cannot drift from the model.
    const SPI1_CMD_USR: u32 = 1 << 28;
    let cmd = machine
        .peek_word(memmap::periph::SPI1)
        .expect("SPI1.cmd is mapped — it is an accept block (P04)");
    assert_eq!(
        cmd & SPI1_CMD_USR,
        SPI1_CMD_USR,
        "SPI1.cmd.usr is set and nothing here will clear it: that is P06's stop, and \
         `periph::accept::spi1`'s own doc predicted it"
    );
    let pc = machine.harts[0].pc();
    println!(
        "P06's STOP: pc={pc:#010x} ({}), SPI1.cmd={cmd:#010x} (usr set), after {} console bytes",
        machine.symbolize(pc).unwrap_or_else(|| "?".into()),
        machine.usb_sj().len(),
    );
}

/// **G5-6 — the scripted host, and its determinism.**
///
/// `--usb-script` is the deterministic twin of the two sockets: the cable
/// schedule and the host's bytes are in a file, at declared **emulated**
/// times, so two runs of one script deliver identical bytes at identical
/// cycle counts. A socket is host time and is only auditable; a script is
/// guest time and reproduces.
///
/// The script here exercises both walk forms. `after "<line>"` waits on what
/// the **device said on this link** — the same log a host on the socket would
/// have read, which is what lets one walk file replay over either link — and
/// `then +<ms>` paces the host after its own last chunk.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn a_usb_script_resolves_its_walk_forms_and_two_runs_are_the_same_run() {
    let Ok(elf) = test_support::fw_esp32s3_image() else {
        test_support::skip_notice(
            "a_usb_script_resolves_its_walk_forms_and_two_runs_are_the_same_run",
            "no image",
        );
        return;
    };
    // The cable goes in at 1 ms and the port opens at 2 ms — so the first
    // line is printed into an endpoint with nobody there, and the transition
    // is what delivers it. Then the host answers the boot: one chunk when the
    // device says it spawned its io task, a second 2 ms after the first.
    const SCRIPT: &str = "\
1   attach
2   open
after \"[INIT] I/O task spawned\" +1ms \"M!{\\\"id\\\":1}\\n\"
then +2ms 4d 21 0a
";
    let build = || {
        let script = lp_emu_esp32s3::control::parse_usb_script(SCRIPT).expect("the script parses");
        assert_eq!(script.commands.len(), 2, "attach and open");
        let mut machine = Esp32S3Builder::new()
            .app(AppSource::Path(elf.clone()))
            .strict(true)
            .usb_host(UsbHost::Absent)
            .usb_script(script.commands)
            .usb_script_source(script.bytes)
            .build()
            .expect("the shipped image direct-loads");
        let outcome = machine.run_until(&StopCondition {
            stop_cycle: Some(GATE_US * memmap::CYCLES_PER_US),
            ..Default::default()
        });
        (machine, outcome)
    };

    let (mut a, outcome) = build();
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert_eq!(a.control_lines(), 2, "both cable commands applied");
    assert_eq!(a.scripted_commands_left(), 0, "and both came due");
    // The cable is in and the port open by 2 ms, and the firmware's first
    // line is not printed until ~8.7 ms, so a draining host takes the whole
    // chain: the script's timing is the reason nothing is merely tried.
    assert_eq!(
        String::from_utf8_lossy(&a.usb_sj()),
        HELLO,
        "the whole chain, delivered to a host the script plugged in"
    );
    assert!(a.usb_sj_tried().is_empty());
    assert_eq!(a.usb_host_now(), Some(UsbHost::Attached { draining: true }));

    // **Both walk forms resolved.** The `after` step waited on a line the
    // device printed on this link and the `then` step on its own predecessor,
    // so 11 + 3 host bytes crossed; the guest is spinning on P06's flash read
    // and never reads them, which is why they are still queued.
    let reply = a.apply_control_now(&lp_emu_esp32s3::control::ControlCommand::State);
    let lp_emu_esp32s3::control::ControlReply::State { host, .. } = reply else {
        panic!("`state` answers a HostReport: {reply}");
    };
    assert!(host.attached && host.draining && host.sof);
    assert_eq!(
        host.out_queued, 14,
        "`after \"…\"` delivered 11 bytes and `then +2ms` three more"
    );

    let (b, _) = build();
    assert_eq!(a.usb_sj(), b.usb_sj(), "identical bytes");
    assert_eq!(a.cycles(), b.cycles(), "at identical cycles");
    assert_eq!(a.usb_sj_tried(), b.usb_sj_tried());
    println!(
        "USB SCRIPT: {} delivered, {} tried, {} commands, {} cycles — twice",
        a.usb_sj().len(),
        a.usb_sj_tried().len(),
        a.control_lines(),
        a.cycles()
    );
}

/// **The ledger triple, and why it is not here.**
///
/// P04b (PR #742) gave this image the classic's `[stack]` / `[MEM]` / `[JIT]`
/// lines at the classic's elicitation points — project load, unload,
/// stop-all, `runtime_status`, either side of a shader compile — and
/// **never** on the five-second heartbeat. So a bare boot prints none of
/// them, exactly as the classic's bare boot does, and eliciting one needs a
/// request on the wire.
///
/// ⚠️ **There is no wire yet.** A request is answered by the server loop, the
/// server loop is behind `mount_filesystem`, and `mount_filesystem` is behind
/// P06's flash engine (see [`the_boot_stops_where_p06_begins`]). So this test
/// pins what the boot prints **today** and names the three shapes as the
/// follow-on assertion, with the two facts about them that are structural
/// rather than measured:
///
/// - `[MEM] … retry_saves=0` — this image has no OOM retry allocator, so the
///   counter is a constant zero rather than a number that happened to be
///   zero on this run;
/// - the whole `[JIT]` line is zeros — the S3 has **no reserved code
///   region**: it JITs out of the `esp_alloc` heap through SRAM1's I-bus
///   alias, so `cap=0` is literally true and JIT residency is inside
///   `[MEM] used`.
///
/// Neither may ever be graded `measured`: nobody has read this chip.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn the_ledger_triple_is_not_elicitable_until_the_server_loop_runs() {
    let Some((machine, _)) = run(UsbHost::Attached { draining: true }) else {
        test_support::skip_notice(
            "the_ledger_triple_is_not_elicitable_until_the_server_loop_runs",
            "no image",
        );
        return;
    };
    let text = String::from_utf8_lossy(&machine.usb_sj()).into_owned();
    for marker in ["[stack]", "[MEM]", "[JIT]"] {
        assert!(
            !text.contains(marker),
            "`{marker}` is elicited, never printed on a bare boot (P04b): {text}"
        );
    }
    // And the one boot line P04b's note fixes: a single-number heap, where
    // the classic prints a four-region sum. Anything comparing the two
    // chips' boot captures must not expect the same line.
    assert!(
        text.contains("[INIT] chip=esp32s3 arch=xtensa heap=245760"),
        "HEAP_SIZE = 240 * 1024, one number: {text}"
    );
    assert!(
        !text.contains("[INIT] main stack"),
        "the S3 prints no `main stack` line; its 37,280 B total is in every `[stack]` \
         line's `of <total> B` instead"
    );

    println!(
        "LEDGER TRIPLE: not elicitable in P05 — a `stop-all` needs the server loop, which \
         needs P06's flash. P06 asserts, after a stop-all:\n  \
         [stack] heartbeat: high-water <used> B of 37280 B (<headroom> B headroom)\n  \
         [MEM] free=… used=… largest_free=… retry_saves=0   (structural: no retry allocator)\n  \
         [JIT] used=0 peak=0 cap=0 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 \
         largest_free=0   (structural: no reserved code region; JIT lives in [MEM] used)"
    );
}
