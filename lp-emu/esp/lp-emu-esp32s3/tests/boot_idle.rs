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
//! The tests that need a built `fw-esp32s3` are `#[ignore]`d and run by
//! `just test-emu-esp32s3-boot`.

use std::path::PathBuf;

use lp_emu_esp_figures::Figures;
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

/// The three lines P05 pinned as absent and P06 delivers (DD86), plus the
/// wire's own two.
const PAST_P05: &[&str] = &[
    "[INIT] flash filesystem mounted",
    "hardware manifest",
    // The line `lpa_link::device_session::device_readiness` matches, and
    // which is chip-agnostic on purpose — never "fw-esp32s3".
    "[INIT] fw-esp32 initialized, starting server loop",
    "\"id\":0,\"msg\":{\"hello\"",
    "[RECOVERY] boot complete (first frame served)",
];

/// The packet of the `hello`'s feature list [`LINK_DEFECT`] dropped before
/// the firmware's gate (module docs). Asserted **present**.
const ONCE_DROPPED: &str =
    ".button\",\"node.clock\",\"node.fluid\",\"node.fixture\",\"node.playlist";

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

/// With a draining host attached from power-on, the io_task's first framed
/// write — the `hello` — reaches the host whole, and nothing is merely
/// tried. See [`LINK_DEFECT`].
fn assert_the_hello_is_delivered_whole(machine: &mut Machine, delivered: &[u8]) {
    assert_nothing_was_refused(machine);
    let tried = machine.usb_sj_tried();
    let text = String::from_utf8_lossy(delivered).into_owned();
    assert_eq!(
        tried.len(),
        0,
        "nothing is merely tried with a draining host ({LINK_DEFECT}): {:?}",
        String::from_utf8_lossy(&tried)
    );
    assert!(
        text.contains(ONCE_DROPPED),
        "the packet the defect used to drop is on the delivered stream ({LINK_DEFECT}):\n{text}"
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
    // mounts, starts the server loop, says hello on the wire and serves a
    // frame.
    for line in PAST_P05 {
        assert!(text.contains(line), "P06's boot prints `{line}`:\n{text}");
    }
    assert!(
        text.contains("[FS] Mount failed (filesystem corrupt), formatting partition..."),
        "a fresh copy of the chip is formatted on its first boot:\n{text}"
    );
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
        // mounted`), mounted this time — the flash is real.
        "[INIT] flash filesystem mounted",
        // The hardware manifest, printed straight after the mount.
        "hardware manifest",
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
after \"[INIT] I/O task spawned\" +1ms \"M!{\\\"id\\\":1}\\n\"
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
    // so 11 + 3 host bytes crossed — and since P06 the server loop **reads**
    // them: nothing is left queued, and both lines are refused by name on
    // the wire, because neither is a request (`M!{"id":1}` has no `msg`,
    // `M!` has nothing). That is a request answered, one request before
    // the ledger test's real one.
    let reply = a.apply_control_now(&lp_emu_esp32s3::control::ControlCommand::State);
    let lp_emu_esp32s3::control::ControlReply::State { host, .. } = reply else {
        panic!("`state` answers a HostReport: {reply}");
    };
    assert!(host.attached && host.draining && host.sof);
    assert_eq!(
        host.out_queued, 0,
        "`after \"…\"` delivered 11 bytes and `then +2ms` three more, and the guest read them"
    );
    assert!(
        text.contains("dropping unparseable 8 B M! line (missing field `msg`"),
        "the first line is refused by name:\n{text}"
    );
    assert!(
        text.contains("dropping unparseable 0 B M! line"),
        "and the second:\n{text}"
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

/// **The ledger triple, elicited** (DD86's last two deliverables: an answered
/// request on the wire, and the triple).
///
/// P04b (PR #742) gave this image the classic's `[stack]` / `[MEM]` / `[JIT]`
/// lines at the classic's elicitation points — project load, unload,
/// stop-all, `runtime_status`, either side of a shader compile — and
/// **never** on the five-second heartbeat. So a bare boot prints none of
/// them, exactly as the classic's bare boot does, and eliciting one needs a
/// request on the wire: `stopAllProjects`, the smallest request that reaches
/// `handlers::handle_stop_all_projects`'s `log_memory`, sent one millisecond
/// after `[INIT] I/O task spawned` — the same directive, on the same
/// trigger line, as P08's `walks/s3-stop-all.script` and the classic's
/// `walks/v3-stop-all.script`.
///
/// Two facts about the triple are structural rather than measured, and are
/// asserted as such:
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
///
/// **The reply is delivered.** The server answers the request (`Stopping
/// all projects (0 loaded)` … `Stopped all projects` are on the wire, and
/// the triple is printed twice — `log_memory` before and after the stop),
/// and the io_task writes the reply line `M!{"id":1,"msg":"stopAllProjects"}`
/// straight after the triple's last `esp_println` packet commits. Before the
/// firmware's gate that write went into the pending packet and was refused —
/// the reply was on the **tried** stream ([`LINK_DEFECT`]); the gate waits
/// for the buffer, so it reaches the host and nothing is refused.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire() {
    let Some((elf, merged)) = images() else {
        test_support::skip_notice(
            "the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire",
            SKIP,
        );
        return;
    };
    const STOP_ALL: &str = "M!{\"id\":1,\"msg\":\"stopAllProjects\"}\n";
    const SCRIPT: &str = "after \"[INIT] I/O task spawned\" +1ms \
                          \"M!{\\\"id\\\":1,\\\"msg\\\":\\\"stopAllProjects\\\"}\\n\"\n";
    let script = lp_emu_esp32s3::control::parse_usb_script(SCRIPT).expect("the script parses");
    assert!(
        script.commands.is_empty(),
        "bytes only: the host is attached from power-on"
    );
    let mut machine = Esp32S3Builder::new()
        .app(AppSource::Path(elf))
        .flash(FlashBacking::Copy(merged))
        .strict(true)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_script_source(script.bytes)
        .build()
        .expect("the shipped image direct-loads");
    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(GATE_US * memmap::CYCLES_PER_US),
        ..Default::default()
    });
    assert!(matches!(outcome, Outcome::Deadline { .. }), "{outcome:?}");
    assert!(machine.first_strict_violation().is_none());
    let delivered = machine.usb_sj();
    let text = String::from_utf8_lossy(&delivered).into_owned();

    // The request reached the server loop and was handled.
    assert!(
        text.contains("Stopping all projects (0 loaded)"),
        "the request was read and handled:\n{text}"
    );
    assert!(text.contains("Stopped all projects"), "{text}");

    // The triple, twice (before and after the stop), in the shapes P05
    // named — with the numbers read rather than pinned, and the structural
    // zeros asserted as constants.
    let stack: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("[stack] heartbeat: high-water "))
        .collect();
    let mem: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("[MEM] free="))
        .collect();
    let jit: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("[JIT] used="))
        .collect();
    assert_eq!(stack.len(), 1, "one [stack] line per stop-all:\n{text}");
    assert_eq!(mem.len(), 2, "[MEM] before and after the stop:\n{text}");
    assert_eq!(jit.len(), 2, "[JIT] before and after the stop:\n{text}");
    // `[stack] heartbeat: high-water <used> B of <total> B (<headroom> B headroom)`
    //
    // `<total>` is the main stack's size, `_stack_start − _stack_end`: the
    // residual of RWDATA after `.data`/`.bss`, so it moves with every byte
    // of statics the image gains or loses — four times on main between 2026-09-23
    // and 2026-09-24 (37,280 → 37,272 → 37,296 → 37,280 → 37,256), none of
    // them a change to the stop-all path. It is a **figure**:
    // `stack_total_bytes` in `lp-emu/esp/figures/esp32s3.json`, exactly as
    // strict as the literal it replaced, re-recorded by
    // `just bless-chips esp32s3`.
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
        "boot_idle::the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire",
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
            *line,
            "[JIT] used=0 peak=0 cap=0 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 \
             largest_free=0",
            "structural: no reserved code region; JIT residency is inside [MEM] used"
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
        "the S3 prints no `main stack` line; its total is in every `[stack]` line's \
         `of <total> B` instead"
    );

    // The reply reached the host, and nothing was refused on the way
    // (module docs, [`LINK_DEFECT`]).
    assert!(
        text.contains(STOP_ALL.trim_end()),
        "the reply is on the delivered stream ({LINK_DEFECT}):\n{text}"
    );
    let tried = String::from_utf8_lossy(&machine.usb_sj_tried()).into_owned();
    assert!(tried.is_empty(), "nothing merely tried: {tried:?}");
    assert_nothing_was_refused(&mut machine);

    println!(
        "LEDGER TRIPLE, elicited by a stop-all at +1 ms after `I/O task spawned`:\n  {}\n  {}\n  {}\n\
         reply: delivered ({} tried bytes; {LINK_DEFECT})",
        stack[0],
        mem[1],
        jit[1],
        machine.usb_sj_tried().len()
    );
}
