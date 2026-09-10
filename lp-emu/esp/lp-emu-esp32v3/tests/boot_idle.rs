//! **The hello.** The shipped image, direct-loaded, and the bytes that leave
//! the chip.
//!
//! P3's ledger recorded 543 bytes of `[INIT]` chain written into an
//! accept block that remembers only the last one
//! (`docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md` §1.1). This
//! file is the same 543 bytes coming out of the **host stream** — through a
//! FIFO, through a shifter draining at 921,600 baud in emulated time, and
//! into a sink — byte for byte, with nothing unmapped and with two runs
//! producing one digest.
//!
//! # What this phase's gate does *not* include, and why neither is a defect
//!
//! - `[INIT] flash filesystem mounted` needs a flash chip, which is **P7**.
//!   The run stands in `esp_rom_spiflash_read_status`'s `memw; l32i; bnez`
//!   spin at the line before it, exactly where P3 left it.
//! - the line after that would read `[INIT] RMT ISR on APP core` on the desk
//!   board and reads `[INIT] APP core unavailable; RMT ISR on PRO core
//!   (single-core semantics)` here — the Q5 fallback arm
//!   (`lp-fw/fw-esp32v3/src/main.rs:838-845`), a supported configuration of
//!   the firmware rather than a hole.
//!
//! # And one number that is *not* silicon's, on purpose
//!
//! `[INIT] main stack 45280 B` where L0's capture says 45,488: the desk board
//! runs a different, dirty commit (`2e21b6226bcd-dirty`, ruling R7). P3
//! recorded the same difference against the same image.

use std::path::PathBuf;

use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, skip_notice};
use sha2::{Digest, Sha256};

/// The last line of the P3 prefix. Everything before it is the 543 bytes,
/// and the line after it is P7's.
const LAST_LINE: &str = "[INIT] I/O task spawned";

/// P3's byte count for the `[INIT]` chain, over a register that swallowed it
/// (the ledger's §1.1). It is the same count out of the wire.
const PREFIX_BYTES: usize = 543;

/// The golden's SHA-256, pinned so a change to the boot's *text* is a
/// deliberate edit to this line and not a test that quietly re-blessed
/// itself. A run that changes it must say which line moved and why.
const PREFIX_SHA256: &str = "ea8bae305953ef613f68a97fb84919378f33b37eb5623dcb970e8dce2b7343e7";

/// Run the shipped image, direct-loaded, under `--strict-bus`, stopping at
/// the first complete line containing `exit_on`.
fn run_to_line(elf: PathBuf, exit_on: &str, micros: u64) -> (Machine, Outcome) {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .build()
        .expect("builds");
    let stop = StopCondition {
        exit_on: Some(exit_on.to_string()),
        ..StopCondition::after_micros(micros)
    };
    let outcome = machine.run_until(&stop);
    (machine, outcome)
}

fn image() -> Option<PathBuf> {
    match fw_esp32v3_image() {
        Ok(p) => Some(p),
        Err(reason) => {
            skip_notice("boot_idle", &reason);
            None
        }
    }
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// **The phase's gate.** The `[INIT]` chain comes out of the host stream, in
/// order, byte for byte, with zero unmapped accesses and no strict stop.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_init_chain_comes_out_of_the_wire_byte_for_byte() {
    let Some(elf) = image() else { return };
    let (machine, outcome) = run_to_line(elf, LAST_LINE, 60_000);
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the boot reached `{LAST_LINE}`: {outcome:?}"
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "zero unmapped accesses: every block the boot met is modelled or accepted"
    );
    assert!(
        machine.first_strict_violation().is_none(),
        "and no strict refusal behind it"
    );

    let bytes = machine.uart0().bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    assert_eq!(
        bytes.len(),
        PREFIX_BYTES,
        "P3 counted {PREFIX_BYTES} bytes into the accept block; the wire carries the same \
         count. Got:\n{text}"
    );
    assert_eq!(
        hex(&Sha256::digest(&bytes)),
        PREFIX_SHA256,
        "the golden moved. Say which line changed and why before re-blessing:\n{text}"
    );

    // The chain, in order — the same list L0 captured off the desk board
    // (`../bench.md`) as far as this phase reaches.
    let expected = [
        "[INIT] fw-esp32v3 boot",
        "[INIT] chip=esp32 arch=xtensa heap=15072+112640+98304+15536=241552",
        "[INIT] heap regions: 0 0x3ffe0440+15072 (ROM PRO stack)",
        "[INIT] main stack 45280 B",
        "[RECOVERY] boot: cause=power-on level=green safe_mode=false prior_boot_complete=true",
        "[INIT] runtime started",
        "[INIT] I/O task spawned (uart0 921600 8N1, swi2 executor prio2, timg0t1 pacer 1ms)",
    ];
    let mut from = 0usize;
    for line in expected {
        let at = text[from..]
            .find(line)
            .unwrap_or_else(|| panic!("missing {line:?} after byte {from} in:\n{text}"));
        from += at + line.len();
    }
    // The heap line is byte-identical to silicon's; the stack line is not,
    // and the reason is the desk board's dirty commit (module docs).
    assert!(text.contains("heap=15072+112640+98304+15536=241552"));
    assert!(
        !text.contains("flash filesystem mounted"),
        "the mount is the line after the flash read, and the flash chip is P7's"
    );
}

/// The run is **deterministic**: two runs, one digest, one cycle count, one
/// instruction count. Nothing in the machine consults the host clock.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn two_runs_of_the_hello_are_one_run() {
    let Some(elf) = image() else { return };
    let (a, oa) = run_to_line(elf.clone(), LAST_LINE, 60_000);
    let (b, ob) = run_to_line(elf, LAST_LINE, 60_000);
    assert_eq!(oa, ob, "the same outcome at the same cycle");
    assert_eq!(a.cycles(), b.cycles());
    assert_eq!(a.instructions(), b.instructions());
    assert_eq!(a.idle_skips(), b.idle_skips());
    let (ba, bb) = (a.uart0().bytes(), b.uart0().bytes());
    assert_eq!(hex(&Sha256::digest(&ba)), hex(&Sha256::digest(&bb)));
    assert_eq!(ba, bb);
}

/// `--exit-on` stops at the line it names and not at a prefix of one still
/// arriving, so the digest above is a whole-line boundary rather than
/// wherever a slice happened to end.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn exit_on_stops_at_a_complete_line() {
    let Some(elf) = image() else { return };
    let (machine, outcome) = run_to_line(elf, "[INIT] runtime started", 60_000);
    assert!(matches!(outcome, Outcome::ExitMatched { .. }), "{outcome:?}");
    let text = machine.uart0().text();
    assert!(text.ends_with("[INIT] runtime started\n"), "got:\n{text}");
    assert!(
        text.len() < PREFIX_BYTES,
        "and it stopped there rather than running on to the I/O task's line"
    );
}

/// Where the direct load now stands: not on a strict stop, but on the flash
/// controller's command word — P7's, and P3's reading unchanged except that
/// the boot got there having actually transmitted its console.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn past_the_hello_the_run_stands_at_the_flash_until_p7() {
    let Some(elf) = image() else { return };
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition::after_micros(80_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "no strict stop and no fault: {outcome:?}"
    );
    let pc = machine.harts[0].pc();
    let sym = machine.symbolize(pc).unwrap_or_default();
    assert!(
        sym.starts_with("esp_rom_spiflash_read_status"),
        "spinning on the flash status command, got pc={pc:#010x} ({sym})"
    );
    // Everything the console had to say is out, and nothing is still in the
    // FIFO waiting for a shifter that stopped.
    assert_eq!(machine.uart0().len(), PREFIX_BYTES);
}
