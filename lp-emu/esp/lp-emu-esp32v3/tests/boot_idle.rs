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
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{outcome:?}"
    );
    let text = machine.uart0().text();
    assert!(text.ends_with("[INIT] runtime started\n"), "got:\n{text}");
    assert!(
        text.len() < PREFIX_BYTES,
        "and it stopped there rather than running on to the I/O task's line"
    );
}

/// The three lines G2 calls the **idle heartbeat**, in the order the board
/// prints them.
const HEARTBEAT: &[&str] = &["[stack] heartbeat: ", "[MEM] free=", "[JIT] used="];

/// The line silicon prints, and — **since M4 P1** — the line this machine
/// prints: core 1 is released by DPORT, boots through the mask ROM's own
/// reset path and binds the RMT ISR in its own matrix
/// (`lp-fw/fw-esp32v3/src/main.rs:838-845`). Through M3 the machine held
/// core 1 (Q5) and the firmware took its documented other arm,
/// [`SINGLE_CORE_LINE`]; P8's gate asserted that arm, and this is the
/// correction. Heap region 3 is added in **both** arms (`main.rs:846-855`),
/// so the heap arithmetic is unaffected either way.
const DUAL_CORE_LINE: &str = "[INIT] RMT ISR on APP core";

/// The Q5 fallback arm — what this machine printed through M3, and what a
/// run with core 1 running must **not** print.
const SINGLE_CORE_LINE: &str = "[INIT] APP core unavailable; RMT ISR on PRO core";

/// ⚠️ **The heartbeat triple is not idle-emitted, and P8 is where that was
/// found out.**
///
/// `[stack] heartbeat:` / `[MEM]` / `[JIT]` come from `esp32_memory_stats`
/// (`main.rs:445`), which `lpa_server` calls from `log_memory` on a project
/// load, unload or stop-all and from `runtime_status` on a client read. It is
/// **not** on the server loop's five-second heartbeat path — that one calls
/// `heartbeat_memory_stats`, which fills a wire field and prints nothing.
///
/// On the desk board the triple appears because the board auto-loads the
/// `Zook dome` project at boot, and it appears **before**
/// `[INIT] fw-esp32 initialized, starting server loop` in L0's own capture,
/// which is what makes it visible there at all. An emulator booting a blank
/// `lpfs` has no project to load, so nothing calls that seam — the boot
/// reaches `[RECOVERY] boot complete (first frame served)` and idles in
/// `esp_rtos::task::idle_hook` with nothing more to say.
///
/// So the gate **asks for it**, over the wire, with the smallest request that
/// reaches the same call site: `stopAllProjects`, which
/// `handlers::handle_stop_all_projects` answers by calling `log_memory`
/// whether or not any project is loaded. That is a client doing what a client
/// does, on a machine with no client attached — which is also G2's "hello
/// over the socket", one request earlier.
const STOP_ALL: &str = "M!{\"id\":1,\"msg\":\"stopAllProjects\"}\n";

/// The device's answer to it, and the gate runs' exit line.
///
/// ⚠️ It is the **last** of the things the gate asserts to reach the wire,
/// not the first. The heartbeat triple is `esp_println` straight into the
/// FIFO; the reply travels through the io_task's queue. A run that stopped
/// on the triple would not have seen the reply yet — two console paths with
/// different latencies, a fact about this firmware rather than this machine.
const REPLY_LINE: &str = "\"id\":1,\"msg\":\"stopAllProjects\"";

/// The script both gate runs use: wait for the io_task to say it is up, then
/// send one request a millisecond later.
///
/// `after`, not an absolute cycle: a host client answers what it hears, and
/// the two boot paths reach this line 240 million cycles apart.
fn stop_all_script() -> lp_emu_esp_common::ScriptedSource {
    lp_emu_esp_common::ScriptedSource::new().after(
        "[INIT] I/O task spawned",
        lp_emu_esp32v3::memmap::CYCLES_PER_US * 1_000,
        STOP_ALL.as_bytes(),
    )
}

/// What both halves of G2 (a) assert about a run that reached the heartbeat.
fn assert_reached_the_heartbeat(machine: &Machine, outcome: &Outcome, path: &str) {
    let text = machine.uart0().text();
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "{path}: the run stops because the heartbeat appeared: {outcome:?}\n{text}"
    );
    assert!(
        machine.first_strict_violation().is_none(),
        "{path}: no strict refusal anywhere in the boot"
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "{path}: G2's first binary condition — zero unmapped accesses"
    );
    for line in HEARTBEAT {
        assert!(text.contains(line), "{path}: no `{line}` in:\n{text}");
    }
    assert!(
        text.contains(DUAL_CORE_LINE) && !text.contains(SINGLE_CORE_LINE),
        "{path}: core 1 runs (M4 P1), so the firmware prints silicon's `{DUAL_CORE_LINE}` and \
         not the Q5 fallback:\n{text}"
    );
    // ⚠️ **Not** `[RECOVERY] boot complete (first frame served)`, which the
    // server loop prints on its first successful tick. That line is a
    // `log::info!` and travels through the io_task's queue; the heartbeat
    // triple is `esp_println`, straight into the FIFO. So the triple reaches
    // the wire *first* and a run that stops on it has not seen the log line
    // yet — two console paths with different latencies, which is a fact
    // about this firmware and not about this machine. The frame really was
    // served: `stopAllProjects` is answered below, and that answer comes out
    // of `tick_and_send`.
    // The hello the server sends unprompted, and the reply to the request
    // the script made — G2 (d), over the wire rather than over a register.
    assert!(
        text.contains("\"id\":0,\"msg\":{\"hello\""),
        "{path}: the unsolicited wire hello:\n{text}"
    );
    assert!(
        text.contains("\"id\":1,\"msg\":\"stopAllProjects\""),
        "{path}: the scripted request is answered:\n{text}"
    );
}

/// **G2 (a), the direct half.** The shipped image, direct-loaded onto the
/// merged chip, under `--strict-bus`, reaches the idle heartbeat with zero
/// unmapped accesses.
#[test]
#[ignore = "needs the shipped image and espflash; run through `just test-emu-esp32v3-boot`"]
fn the_direct_load_reaches_the_idle_heartbeat() {
    let Some(elf) = image() else { return };
    let merged = match lp_emu_esp32v3::test_support::merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_direct_load_reaches_the_idle_heartbeat", &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .flash(lp_emu_esp32v3::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .strict(true)
        .uart0_script(stop_all_script())
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition {
        exit_on: Some(REPLY_LINE.to_string()),
        ..StopCondition::after_micros(2_000_000)
    });
    assert_reached_the_heartbeat(&machine, &outcome, "direct");
}

/// **G2 (a), the ROM-up half.** The same image, the same chip, started at the
/// mask ROM's reset vector — through the real ROM and the real ESP-IDF
/// second-stage bootloader — and the same heartbeat comes out.
#[test]
#[ignore = "needs the shipped image and espflash; run through `just test-emu-esp32v3-boot`"]
fn the_rom_up_boot_reaches_the_idle_heartbeat() {
    let merged = match lp_emu_esp32v3::test_support::merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_rom_up_boot_reaches_the_idle_heartbeat", &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(lp_emu_esp32v3::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .strict(true)
        .uart0_script(stop_all_script())
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition {
        exit_on: Some(REPLY_LINE.to_string()),
        ..StopCondition::after_micros(3_000_000)
    });
    assert_reached_the_heartbeat(&machine, &outcome, "rom-up");
    // And the bootloader's own log is in front of it, including the `E` that
    // is correct (`tests/rom_up_boot.rs` compares it line for line).
    let text = machine.uart0().text();
    assert!(text.contains("ets Jul 29 2019 12:21:46"), "the ROM banner");
    assert!(
        text.contains("Image contains multiple DROM segments"),
        "the DROM-segments line the desk board prints on every boot"
    );
}

/// **G2's memory figures, side by side with L0's.**
///
/// The two runs are of **different image bytes** — the desk board is on
/// `2e21b6226bcd`-dirty (ruling R7) and this tree is not — so nothing here is
/// asserted equal. What is asserted is that the triple parses, that both boot
/// paths produce the **same** figures as each other, and that the numbers are
/// printed where the gate packet can quote them. The equality against silicon
/// is L1's, and it is held until L1 captures from a clean pinned commit.
#[test]
#[ignore = "needs the shipped image and espflash; run through `just test-emu-esp32v3-boot`"]
fn the_two_paths_report_the_same_memory_figures() {
    let Some(elf) = image() else { return };
    let merged = match lp_emu_esp32v3::test_support::merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_two_paths_report_the_same_memory_figures", &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;

    let triple = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| HEARTBEAT.iter().any(|h| l.contains(h)))
            .map(|l| {
                let at = HEARTBEAT
                    .iter()
                    .find_map(|h| l.find(h))
                    .expect("a heartbeat line");
                l[at..].to_string()
            })
            .take(3)
            .collect()
    };

    let mut direct = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .flash(lp_emu_esp32v3::flash::FlashBacking::Copy(merged.clone()))
        .flash_len(len)
        .strict(true)
        .uart0_script(stop_all_script())
        .build()
        .expect("builds");
    direct.run_until(&StopCondition {
        exit_on: Some("[JIT] used=".to_string()),
        ..StopCondition::after_micros(2_000_000)
    });

    let mut rom_up = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .flash(lp_emu_esp32v3::flash::FlashBacking::Copy(merged))
        .flash_len(len)
        .strict(true)
        .uart0_script(stop_all_script())
        .build()
        .expect("builds");
    rom_up.run_until(&StopCondition {
        exit_on: Some("[JIT] used=".to_string()),
        ..StopCondition::after_micros(3_000_000)
    });

    let a = triple(&direct.uart0().text());
    let b = triple(&rom_up.uart0().text());
    assert_eq!(a.len(), 3, "the direct run printed the triple: {a:?}");
    println!("direct:\n  {}", a.join("\n  "));
    println!("rom-up:\n  {}", b.join("\n  "));
    assert_eq!(a, b, "the two boot paths report the same memory figures");
    // The one figure the boot banner carries too, so the triple can be read
    // against `[INIT] chip=esp32 … heap=…`.
    assert!(
        direct
            .uart0()
            .text()
            .contains("heap=15072+112640+98304+15536=241552"),
        "the heap arithmetic is the desk board's, in both arms of Q5"
    );
}

// ---------------------------------------------------------------------------
// G2 (e), the memory half: this machine against the desk board, field by field.
// ---------------------------------------------------------------------------

/// The committed silicon transcript this machine's memory figures are read
/// against — **lab task L1's**, taken from the DOM-Z-102 after it was
/// re-flashed with the pinned clean reference image.
///
/// ⚠️ **It is the same bytes on both sides, and that is the whole point.**
/// Ruling R7 was that L0's board ran `2e21b6226bcd`-**dirty**, which no
/// commit rebuilds, so the earlier pair
/// `silicon-esp32v3-2026-09-10-2e21b6226-{115200,921600}` could only ever be
/// compared by judgement. L1 answered it (a): the board was written with
/// `build-reference-image.sh --chip esp32 esp32,server,float-f32 75486b114`
/// merged by `espflash save-image --merge`, and re-captured. Those two
/// transcripts stay committed as the dirty-image record — `rom_up_boot.rs`
/// still reads the 115200 one — and this is the one G2 stands on.
const SILICON_921600: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../transcripts/esp32v3/boot-idle/",
    "silicon-esp32v3-2026-09-10-75486b114-921600.txt"
);

/// The first line of `text` containing `marker`, from `marker` to the end of
/// that line.
///
/// ⚠️ **Not a line-start match, deliberately.** `esp_println` writes the
/// triple straight into the TX FIFO while `log::info!` lines travel through
/// the io_task's queue, so on both sides a `[stack] heartbeat:` can begin in
/// the middle of somebody else's line. That interleaving is on the wire and
/// the transcript keeps it; reading from the marker is what makes the field
/// readable without editing either side.
fn field_line<'a>(text: &'a str, marker: &str) -> &'a str {
    let at = text
        .find(marker)
        .unwrap_or_else(|| panic!("no `{marker}` in:\n{text}"));
    text[at..].split(['\r', '\n']).next().expect("a line")
}

/// The decimal number following `key` in `line`.
fn number(line: &str, key: &str) -> u64 {
    let at = line
        .find(key)
        .unwrap_or_else(|| panic!("no `{key}` in `{line}`"));
    let rest = &line[at + key.len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end]
        .parse()
        .unwrap_or_else(|_| panic!("`{key}` in `{line}` is not a number"))
}

/// **The heap's live bytes, and this machine is 84 of them heavier.**
///
/// Measured by L1 on 2026-09-10, deterministic on both sides: three boots of
/// this machine give `free=223268 used=18284` every time, and two separate
/// power-on captures of the desk board give `free=223352 used=18200` every
/// time. So it is **not** the transient the bench notes warn about — L1
/// re-captured with the request fired within a millisecond of the trigger
/// rather than within fifty, and silicon did not move — and it is not
/// sampling noise, even though silicon's own three samples span 536 B.
///
/// The one structural difference known to exist between the two runs is
/// **Q5**: silicon binds the RMT ISR to the APP core and this machine has no
/// APP core, so the firmware takes its documented single-core arm
/// ([`SINGLE_CORE_LINE`]). It is the prime suspect and it is not proven.
/// Pinned rather than tolerated: a change in either direction is a finding.
const HEAP_USED_GAP: u64 = 84;

/// **The main stack's high-water, and the desk board goes 960 B deeper.**
///
/// Same measurement, same determinism, and the obvious explanation was
/// **tested and refuted**: this is not a question of when the sample was
/// taken. L1 tightened the host's send from a 50 ms poll to a 1 ms one, so
/// the desk board's sample moved to within a millisecond of the same trigger
/// this machine's script uses, and silicon reported the identical 16972 B.
/// High-water is monotonic, so a later sample can only be larger — and
/// silicon's stayed 16972 through three requests over twelve seconds.
///
/// Q5 again is the suspect and again unproven; note that the naive reading of
/// it points the wrong way (an RMT ISR moved onto the PRO core should make
/// *this* machine's main stack deeper, not shallower). Pinned, not tolerated.
///
/// ⚠️ **M4 P1 could not re-measure either gap.** With core 1 running, the
/// shipped image dies in `LpFs::read_file` ~30k cycles after the release
/// (`docs/defects/2026-09-10-the-app-cores-rom-boot-rewrites-heap-region-0.md`),
/// before any heartbeat — so both constants still carry the single-core
/// fallback's figures, this test is red on the shipped image until the
/// defect is fixed, and the first green run after that fix is the one that
/// re-pins them. A diagnostic firmware with ESP-IDF's region ordering read
/// `high-water 16268 B` and `free=223268 used=18284` on its second boot,
/// which is not the shipped image and is quoted in the P1 report, not here.
const STACK_HIGH_WATER_GAP: u64 = 960;

/// **G2 (e).** Every memory-class field of the idle heartbeat, this machine
/// against the desk board, on the same image and the same request.
///
/// # What is compared, and what is not
///
/// Compared: the boot banner's heap arithmetic, the main stack's **size**,
/// the JIT region placement line, the whole `[JIT]` census, and the `[MEM]`
/// line's `largest_free` and `retry_saves`. Those are equal, exactly.
///
/// Pinned rather than asserted equal: `used`/`free` and the stack's
/// high-water — see [`HEAP_USED_GAP`] and [`STACK_HIGH_WATER_GAP`], which
/// carry the measurements and say what is still unexplained. G2 asked for
/// every memory-class field to be equal; two are not, deterministically, and
/// a test that widened a threshold to hide that would be answering a
/// different question.
///
/// Not compared at all: anything in the `timing` class, and anything the
/// sidecar grades `modeled`. The desk board's sample is a host-latency
/// distance from its trigger and this machine's is an emulated millisecond;
/// that difference is real and is why the two `[MEM]` readings of a single
/// `stopAllProjects` — `log_memory` runs before and after the stop — are
/// taken as a pair on each side rather than across them.
///
/// # ⚠️ Why this machine boots twice
///
/// The desk board was captured on its **second** boot: `espflash` hard-resets
/// after writing, so the board had already formatted the merged image's blank
/// `lpfs` before L1 ever opened the port. A machine given a fresh
/// `FlashBacking::Copy` is on its **first** boot, formats, and reports
/// `largest_free=106494` — 2032 B short of silicon, purely because the format
/// is still live in the arena. That gap is not a difference between the two
/// machines and it disappears when both sides have mounted: this test boots
/// once to format, flushes the chip, and reads the figures off the boot after
/// it. `rom_up_boot.rs::a_second_boot_from_the_same_chip_mounts_rather_than_reformats`
/// is the same fact from the flash census's side.
#[test]
#[ignore = "needs the shipped image and espflash; run through `just test-emu-esp32v3-boot`"]
fn the_heartbeats_memory_figures_are_the_desk_boards() {
    let Some(elf) = image() else { return };
    let merged = match lp_emu_esp32v3::test_support::merged_chip_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("the_heartbeats_memory_figures_are_the_desk_boards", &reason);
            return;
        }
    };
    let len = std::fs::metadata(&merged).expect("the merged image").len() as u32;
    let silicon = std::fs::read_to_string(SILICON_921600).expect("the committed silicon capture");

    // A writable chip, so the second boot mounts what the first one wrote —
    // which is the state the desk board was in when it was captured.
    let chip = std::env::temp_dir().join(format!("lp-emu-v3-l1-{}.bin", std::process::id()));
    std::fs::copy(&merged, &chip).expect("a writable chip");
    let boot = |script: bool| {
        let mut b = Esp32V3Builder::new()
            .boot_mode(BootMode::Direct)
            .app(AppSource::Path(elf.clone()))
            .flash(lp_emu_esp32v3::flash::FlashBacking::File(chip.clone()))
            .flash_len(len)
            .strict(true);
        if script {
            b = b.uart0_script(stop_all_script());
        }
        b.build().expect("builds")
    };

    let mut first = boot(false);
    first.run_until(&StopCondition {
        exit_on: Some("boot complete (first frame served)".to_string()),
        ..StopCondition::after_micros(3_000_000)
    });
    assert!(
        first.uart0().text().contains("Formatted and mounted fresh"),
        "the first boot formats the merged image's blank `lpfs`:\n{}",
        first.uart0().text()
    );
    first.flush_flash().expect("the write back");

    let mut second = boot(true);
    let outcome = second.run_until(&StopCondition {
        exit_on: Some("[JIT] used=".to_string()),
        ..StopCondition::after_micros(2_000_000)
    });
    let emulated = second.uart0().text();
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the second boot reaches the elicited heartbeat: {outcome:?}\n{emulated}"
    );
    assert!(
        !emulated.contains("Formatted and mounted fresh"),
        "the second boot MOUNTS what the first wrote:\n{emulated}"
    );
    let _ = std::fs::remove_file(&chip);

    // The three lines the boot banner carries, which are image-derived and
    // must be identical to the byte.
    for marker in [
        "[INIT] chip=esp32 arch=xtensa heap=",
        "[INIT] main stack ",
        "[INIT] JIT code region: ",
    ] {
        let (a, b) = (field_line(&silicon, marker), field_line(&emulated, marker));
        println!("  {marker}\n    silicon  {a}\n    emulator {b}");
        assert_eq!(a, b, "`{marker}` is image-derived and must be identical");
    }

    // The whole `[JIT]` census, which is a memory-class line end to end.
    let (jit_s, jit_e) = (
        field_line(&silicon, "[JIT] used="),
        field_line(&emulated, "[JIT] used="),
    );
    println!("  [JIT]\n    silicon  {jit_s}\n    emulator {jit_e}");
    assert_eq!(jit_s, jit_e, "the `[JIT]` census is identical");

    // `[MEM]`, field by field.
    let (mem_s, mem_e) = (
        field_line(&silicon, "[MEM] free="),
        field_line(&emulated, "[MEM] free="),
    );
    println!("  [MEM]\n    silicon  {mem_s}\n    emulator {mem_e}");
    assert_eq!(
        number(mem_s, "largest_free="),
        number(mem_e, "largest_free="),
        "the largest free block is the desk board's, once both sides have mounted"
    );
    assert_eq!(
        number(mem_s, "retry_saves="),
        number(mem_e, "retry_saves="),
        "no OOM retry saved either side"
    );
    // `free` and `used` partition one arena, and the arena is the boot
    // banner's 241552 on both sides — so the gap below is one number, not two.
    assert_eq!(
        number(mem_s, "free=") + number(mem_s, "used="),
        number(mem_e, "free=") + number(mem_e, "used="),
        "`free` and `used` partition the same total on both sides"
    );
    assert_eq!(
        number(mem_e, "used=") - number(mem_s, "used="),
        HEAP_USED_GAP,
        "the live-bytes gap is the measured one; see HEAP_USED_GAP"
    );

    // `[stack] heartbeat:` — the size is image-derived and equal, the
    // high-water is the second pinned gap.
    let (st_s, st_e) = (
        field_line(&silicon, "[stack] heartbeat: "),
        field_line(&emulated, "[stack] heartbeat: "),
    );
    println!("  [stack]\n    silicon  {st_s}\n    emulator {st_e}");
    assert_eq!(
        number(st_s, " B of "),
        number(st_e, " B of "),
        "the main stack is the same size on both sides"
    );
    assert_eq!(
        number(st_s, "high-water ") - number(st_e, "high-water "),
        STACK_HIGH_WATER_GAP,
        "the high-water gap is the measured one; see STACK_HIGH_WATER_GAP"
    );

    // Which of the firmware's two supported configurations this machine was
    // running, named so a reader of a failure knows. Since M4 P1 it is the
    // dual-core one, the same as the desk board's.
    assert!(
        emulated.contains(DUAL_CORE_LINE),
        "this machine runs core 1 (M4 P1) and binds the RMT ISR to it:\n{emulated}"
    );
    assert!(
        silicon.contains("[INIT] RMT ISR on APP core"),
        "the desk board has an APP core and binds the RMT ISR to it"
    );
}
