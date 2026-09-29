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
//! # Where P05's boot stopped, and what P06 changed
//!
//! Until P06 the image did **not** reach `[INIT] fw-esp32 initialized,
//! starting server loop` on this machine: after `[INIT] I/O task spawned`
//! it called `mount_filesystem(flash)`, `esp_storage` set `SPI1.cmd` bit 28
//! (`usr`) and spun until hardware cleared it, and `SPI1` was an accept
//! block, so the bit stayed set for ever — exactly what
//! `lp_emu_esp32s3::periph::accept::spi1`'s own doc predicted in P04. P06
//! put `engine::spi_flash` behind `SPI1` and a real chip behind the windows
//! (`crate::flash`, `crate::cache`), so the boot now goes on: the
//! filesystem mounts (a fresh chip is formatted first), the hardware
//! manifest prints, the server loop starts, the unsolicited `hello` goes
//! out on the wire, and a request on the wire is answered.
//! [`the_boot_goes_past_where_p05_stopped`] pins the three lines P05 pinned
//! as absent, present, with the same register read back — `cmd.usr` clear,
//! because the engine completes the transfer.
//!
//! The ledger triple (`[stack]`, `[MEM]`, `[JIT]`, added to this image by
//! P04b / PR #742) is **elicited**, as the classic's is: a `stopAllProjects`
//! over the wire, one millisecond after `[INIT] I/O task spawned` — the same
//! directive P08's `walks/s3-stop-all.script` carries.
//! [`the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire`] is that run.
//!
//! # The link's one send buffer, and the firmware's gate
//!
//! `docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md`:
//! the block has **one** IN buffer that firmware cannot write from a flush
//! until the host has read it (the TRM; `lp-emu-esp-common`'s `ip/usb_sj.rs`
//! cites it and pins the contract at the registers). esp-println (polled)
//! and the io_task (esp-hal's `write_async`) are two writers on it, and
//! esp-hal's writer neither reads `serial_in_ep_data_free` nor clears the
//! raw `serial_in_empty` esp-println's drains leave set. Until the fix the
//! io_task wrote into esp-println's pending packet (a stop-all's reply was
//! lost) and woke early on that stale raw bit (one 64-byte packet of the
//! boot `hello` was lost, until an unrelated timing shift hid it).
//!
//! The fix is the firmware's: `fw-esp32-common/src/serial/in_endpoint.rs` (the
//! io_task gate, shared with the C6) waits for the buffer to be free and clears
//! the stale bit before every io_task packet. So this file pins the
//! **mechanism**, not a byte count that moves with timing: on every path — a
//! draining host, no host, a closed port, a scripted cable, a request on the
//! wire — the guest **never writes into a pending or full buffer**
//! (`Machine::usb_sj_refused() == Some(0)`), and with a draining host nothing
//! is merely tried at all.
//!
//! The `[INIT]` chain itself (esp-println, polled) is delivered whole and
//! is still pinned byte for byte as the stream's prefix.
//!
//! # Since wire proto 30: lp-link on the USB port
//!
//! The image speaks lp-link on USB-Serial-JTAG (plan
//! `lp2025/2026-09-27-0215-lp-link-usb-cutover`): the `[INIT]` chain up to
//! the server loop's boot marker is still raw esp-println text, but the
//! board's `log` lines (the hardware manifest, the `[FS]` format lines,
//! `[RECOVERY] boot complete`) ride the link's log channel, and the hello
//! and every reply go out only once a host has brought the link up. Nothing
//! under `lp-emu/` may host a link (the MIT fence), so this file now pins
//! what the PORT shows — the raw chain, the boot marker, the one-buffer
//! mechanism, determinism — and the elicited ledger triple, which needs a
//! request on the wire, moved to the host side:
//! `lp-cli/tests/emu_s3_link_gates.rs` (`the_ledger_triple_is_elicited_…`,
//! with the `stack_total_bytes` figure).
//!
//! The tests that need a built `fw-esp32s3` are `#[ignore]`d and run by
//! `just test-emu-esp32s3-boot`.

use std::path::PathBuf;

use lp_emu_esp32s3::flash::FlashBacking;
use lp_emu_esp32s3::machine::{
    AppSource, Esp32S3Builder, Machine, Outcome, StopCondition, UsbHost,
};
use lp_emu_esp32s3::periph::usb_sj::IN_DRAIN_LATENCY_CYCLES;
use lp_emu_esp32s3::{memmap, test_support};
use sha2::{Digest, Sha256};

/// Why a test here did nothing: both files are needed.
const SKIP: &str = "no image (LP_EMU_ESP32S3_ELF) or no merged chip (LP_EMU_ESP32S3_MERGED)";

/// The shipped ELF and the merged chip behind it, or `None` — the caller
/// prints the skip notice. **Every test here boots the flashed chip**: since
/// P06 the boot's second half — the mount, the server loop, the hello — is
/// what the chip holds, and a blank chip is a different reading (the
/// memory-FS fallback), which `tests/boot.rs` pins.
fn images() -> Option<(PathBuf, PathBuf)> {
    match (
        test_support::fw_esp32s3_image(),
        test_support::merged_chip_image(),
    ) {
        (Ok(elf), Ok(merged)) => Some((elf, merged)),
        _ => None,
    }
}

/// Long enough that the whole boot — the `[INIT]` chain by ~8.9 ms of guest
/// time, the first-boot format and the server loop's first tick by ~120 ms
/// — has been printed, short enough that a suite run is seconds; the RWDT's
/// boot stage is 30 s away.
const GATE_US: u64 = 2_000_000;

/// The defect entry for the link's stale-`serial_in_empty` drop (module docs).
const LINK_DEFECT: &str = "docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md";

/// The lines P05 pinned as absent and P06 delivers (DD86) that are still raw
/// text on the port since proto 30 (the hardware manifest, the hello and
/// `[RECOVERY] boot complete` ride the link now).
const PAST_P05: &[&str] = &[
    "[INIT] flash filesystem mounted",
    // The line `lpa_link::device_session::device_readiness` matches, and
    // which is chip-agnostic on purpose — never "fw-esp32s3".
    "[INIT] fw-esp32 initialized, starting server loop... proto=",
];

/// The mechanism, on any path: the guest never wrote into a pending or
/// full IN buffer ([`LINK_DEFECT`], module docs).
fn assert_nothing_was_refused(machine: &mut Machine) {
    assert_eq!(
        machine.usb_sj_refused(),
        Some(0),
        "a write into a pending or full IN buffer — the one-buffer contract broken \
         ({LINK_DEFECT})"
    );
}

/// With a draining host attached from power-on nothing the guest wrote is
/// merely tried, and it never wrote into a pending buffer. See
/// [`LINK_DEFECT`]. (Until proto 30 this also found the `M!` hello's
/// feature-list packet the defect once dropped; the hello rides the link
/// now, and a host must bring the link up to see it.)
fn assert_the_hello_is_delivered_whole(machine: &mut Machine, _delivered: &[u8]) {
    assert_nothing_was_refused(machine);
    let tried = machine.usb_sj_tried();
    assert_eq!(
        tried.len(),
        0,
        "nothing is merely tried with a draining host ({LINK_DEFECT}): {:?}",
        String::from_utf8_lossy(&tried)
    );
}

/// **The hello, byte for byte.** Pinned as a whole stream, as
/// `lp-emu-esp32v3/tests/boot.rs` pins the classic's.
const HELLO: &str = "\
[INIT] fw-esp32s3 boot
[INIT] chip=esp32s3 arch=xtensa heap=245760
[RECOVERY] boot: cause=power-on level=green safe_mode=false prior_boot_complete=true
[RECOVERY] RWDT armed: boot 30000 ms, runtime 8000 ms
[INIT] runtime started
[INIT] USB link task spawned
";

/// [`HELLO`]'s length and sha256, so a change to any byte of it is a failure
/// that names the diff rather than a diff a reader has to spot.
const HELLO_BYTES: usize = 258;
const HELLO_SHA: &str = "373e7a52efe39079977dd7397208d35241e5e8870a94867d07556f2b2f570995";

fn sha(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// A strict run of the shipped image with the host in `host`, for `GATE_US`.
fn run(host: UsbHost) -> Option<(Machine, Outcome)> {
    let (elf, merged) = images()?;
    let mut machine = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
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
    let Some((mut machine, outcome)) = run(UsbHost::Attached { draining: true }) else {
        test_support::skip_notice(
            "the_shipped_image_prints_its_init_chain_out_of_the_link",
            SKIP,
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
    let text = String::from_utf8_lossy(&delivered).into_owned();
    // The `[INIT]` chain, in order, byte for byte, **first** out of the
    // link — P05's hello, as the stream's prefix now that the boot goes on.
    assert!(
        text.starts_with(HELLO),
        "the whole chain, in order, first out of the link:\n{text}"
    );
    assert_eq!(&delivered[..HELLO_BYTES], HELLO.as_bytes());
    assert_eq!(sha(&delivered[..HELLO_BYTES]), HELLO_SHA);
    // …and P06's continuation behind it: the flash is real, so the boot
    // mounts and starts the server loop. (Its hello, the `[FS]` format
    // lines and the first frame served ride the link since proto 30; the
    // format itself is `rom_up_boot.rs`'s flash-census claim.)
    for line in PAST_P05 {
        assert!(text.contains(line), "P06's boot prints `{line}`:\n{text}");
    }
    // A draining host took every byte, the io_task's hello included, and
    // the guest never wrote into a pending buffer (module docs).
    assert_the_hello_is_delivered_whole(&mut machine, &delivered);
    // The console and the link are the same stream on this chip, so
    // `--console` writes exactly this.
    assert_eq!(machine.console().bytes(), delivered);

    println!(
        "HELLO: {} bytes, sha {}, then {} more; {} tried ({LINK_DEFECT}); {} us of guest time, \
         unmapped=0",
        HELLO_BYTES,
        HELLO_SHA,
        delivered.len() - HELLO_BYTES,
        machine.usb_sj_tried().len(),
        machine.cycles() / memmap::CYCLES_PER_US
    );
    print!("{text}");
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
    let Some((mut machine, outcome)) = run(UsbHost::Absent) else {
        test_support::skip_notice(
            "with_no_host_the_console_commits_once_and_falls_silent",
            SKIP,
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
    // boot` is every byte **esp-println** ever hands over with nobody there.
    const FIRST: &[u8] = b"[INIT] fw-esp32s3 boot";
    assert!(
        tried.starts_with(FIRST),
        "one committed line, then TIMED_OUT latches: {}",
        String::from_utf8_lossy(&tried)
    );
    let rest = &tried[FIRST.len()..];
    let rest_text = String::from_utf8_lossy(rest).into_owned();
    assert!(
        !rest_text.contains("[INIT]"),
        "the console fell silent after one commit: no second `[INIT]` line was ever tried: \
         {rest_text:?}"
    );
    // Since P06 the boot goes on to the server loop, whose **io_task** does
    // not latch on esp-println's flag. Before the firmware's gate it wrote
    // the hello's first 64-byte chunk and then a `\n` per probe interval
    // into esp-println's committed packet — refused, and observed here
    // (22 + 64 + 2). With the gate it waits for a buffer that never frees,
    // its 250 ms chunk timeout fires, it latches *its* not-draining, and it
    // writes **nothing**: the first line is every byte anyone handed over.
    assert_eq!(
        tried.len(),
        FIRST.len(),
        "the first line and nothing else: {rest_text:?}"
    );
    assert_nothing_was_refused(&mut machine);
    println!(
        "HOST ABSENT: 0 bytes delivered, {} bytes tried — esp-println fell silent after one \
         commit, the io_task waited for a free buffer and wrote nothing, and the run kept \
         running ({} us, unmapped={})",
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
    let Some((elf, merged)) = images() else {
        test_support::skip_notice(
            "an_attached_but_closed_port_holds_the_packet_until_an_application_opens_it",
            SKIP,
        );
        return;
    };
    let mut machine = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
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
    // esp-println's held packet is not a lost one, and the io_task waits for
    // it rather than writing into it (before the firmware's gate its first
    // chunk and two probes, 64 + 2 bytes, were refused here — see
    // `with_no_host_the_console_commits_once_and_falls_silent`).
    let tried = machine.usb_sj_tried();
    let tried_text = String::from_utf8_lossy(&tried).into_owned();
    assert!(
        tried.is_empty(),
        "nothing tried, nothing lost: {tried_text:?}"
    );
    assert_nothing_was_refused(&mut machine);
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
        "held, then delivered — the open dropped nothing"
    );
    assert_nothing_was_refused(&mut machine);
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
        test_support::skip_notice("two_runs_of_the_hello_are_the_same_run", SKIP);
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

/// **G5-5, flipped — the boot goes past where P05 stopped, and the register
/// says why.**
///
/// P05 pinned the **absence** of three lines with the register the run was
/// spinning on: `SPI1.cmd` bit 28 (`usr`), set by `esp_storage` for a flash
/// read no hardware there completed. P06 put `engine::spi_flash` behind
/// `SPI1`, so the same register now reads with `usr` **clear** — the engine
/// completes the transfer and clears the trigger, which is what the ROM's
/// driver and `esp_storage` both spin on — and the three lines are present.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn the_boot_goes_past_where_p05_stopped() {
    let Some((mut machine, _)) = run(UsbHost::Attached { draining: true }) else {
        test_support::skip_notice("the_boot_goes_past_where_p05_stopped", SKIP);
        return;
    };
    let text = String::from_utf8_lossy(&machine.usb_sj()).into_owned();
    for present in [
        // The `lpfs` mount line (`main.rs`'s `[INIT] flash filesystem
        // mounted`), mounted this time — the flash is real. (The hardware
        // manifest line after it is a log line, on the link since proto 30.)
        "[INIT] flash filesystem mounted",
        // The line `lpa_link::device_session::device_readiness` matches, and
        // which is chip-agnostic on purpose — never "fw-esp32s3".
        "fw-esp32 initialized, starting server loop",
    ] {
        assert!(
            text.contains(present),
            "`{present}` is printed after `mount_filesystem`, which P06's flash engine \
             serves:\n{text}"
        );
    }
    assert!(
        !text.contains("memory filesystem"),
        "no memory-filesystem fallback: the mount succeeded:\n{text}"
    );

    // The register P05 cited, read back through the machine's own decode so
    // the citation cannot drift from the model: `usr` is clear, because the
    // engine's command completed and cleared its own trigger.
    const SPI1_CMD_USR: u32 = 1 << 28;
    let cmd = machine
        .peek_word(memmap::periph::SPI1)
        .expect("SPI1.cmd is mapped — engine::spi_flash (P06)");
    assert_eq!(
        cmd & SPI1_CMD_USR,
        0,
        "SPI1.cmd.usr is clear: the engine completes a `usr` transfer and clears the bit \
         `esp_storage` spins on"
    );
    // And the chip saw the mount: a fresh copy is formatted (erases and
    // programs), then read.
    let census = machine.flash().lock().expect("flash").command_census();
    assert!(
        census.reads > 0 && census.sector_erases > 0 && census.programs > 0,
        "{census}"
    );
    let pc = machine.harts[0].pc();
    println!(
        "PAST P05: pc={pc:#010x} ({}), SPI1.cmd={cmd:#010x} (usr clear), {census}, after {} \
         console bytes",
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
    let Some((elf, merged)) = images() else {
        test_support::skip_notice(
            "a_usb_script_resolves_its_walk_forms_and_two_runs_are_the_same_run",
            SKIP,
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
after \"[INIT] USB link task spawned\" +1ms \"M!{\\\"id\\\":1}\\n\"
then +2ms 4d 21 0a
";
    let build = || {
        let script = lp_emu_esp32s3::control::parse_usb_script(SCRIPT).expect("the script parses");
        assert_eq!(script.commands.len(), 2, "attach and open");
        let mut machine = Esp32S3Builder::new()
            .app(AppSource::Path(elf.clone()))
            .flash(FlashBacking::Copy(merged.clone()))
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
    // chain: the script's timing is the reason nothing of it is merely
    // tried. (The hello, too, is delivered whole — the firmware's gate; see
    // the module docs.)
    let delivered = a.usb_sj();
    let text = String::from_utf8_lossy(&delivered).into_owned();
    assert!(
        text.starts_with(HELLO),
        "the whole chain, delivered to a host the script plugged in:\n{text}"
    );
    assert_the_hello_is_delivered_whole(&mut a, &delivered);
    assert_eq!(a.usb_host_now(), Some(UsbHost::Attached { draining: true }));

    // **Both walk forms resolved.** The `after` step waited on a line the
    // device printed on this link and the `then` step on its own predecessor,
    // so 11 + 3 host bytes crossed, and the link task **read** them: nothing
    // is left queued. (Until proto 30 the server loop then refused both
    // lines by name on the wire, `dropping unparseable … M! line`; the link
    // task reads them as text outside frames and drops them, and says so
    // only on its log channel, which a host must bring up to read.)
    let reply = a.apply_control_now(&lp_emu_esp32s3::control::ControlCommand::State);
    let lp_emu_esp32s3::control::ControlReply::State { host, .. } = reply else {
        panic!("`state` answers a HostReport: {reply}");
    };
    assert!(host.attached && host.draining && host.sof);
    assert_eq!(
        host.out_queued, 0,
        "`after \"…\"` delivered 11 bytes and `then +2ms` three more, and the guest read them"
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
