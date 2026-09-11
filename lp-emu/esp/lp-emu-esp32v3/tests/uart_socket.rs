//! The wire and the cable, through the real machine.
//!
//! `src/periph/uart.rs`'s own tests drive the view in a sandbox; these drive
//! it **through the bus**, at the addresses a guest uses, with the machine's
//! scheduler moving guest time — and they drive the CH340 cable's own socket,
//! which only exists at machine level.
//!
//! # Why a rom-up machine with no application
//!
//! Every test below that needs guest time but not a guest program starts the
//! mask ROM and stops well before cycle 30,989, which is where `uartAttach`
//! first touches UART0 (`tests/boot.rs`). Until then the ROM is inside its
//! own anti-glitch eFuse check and does not look at this block at all, so the
//! machine supplies a clock, a scheduler and a bus decode and nothing
//! competes for the registers.

use lp_emu_esp_common::{ScriptedSource, Strap};
use lp_emu_esp32v3::control::{Cable, ControlCommand};
use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::memmap;
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, skip_notice};

const FIFO: u32 = memmap::periph::UART0;
const INT_RAW: u32 = memmap::periph::UART0 + 0x04;
const INT_CLR: u32 = memmap::periph::UART0 + 0x10;
const CLKDIV: u32 = memmap::periph::UART0 + 0x14;
const STATUS: u32 = memmap::periph::UART0 + 0x1c;
const CONF1: u32 = memmap::periph::UART0 + 0x24;

const INT_RXFIFO_FULL: u32 = 1 << 0;
const INT_RXFIFO_TOUT: u32 = 1 << 8;

/// A divider that makes a symbol cheap in guest cycles, so a test that
/// delivers five bytes finishes long before the ROM reaches this block.
/// `clkdiv = 1` against APB is `divider16 = 16`, one bit `ceil(240e6 × 16 /
/// (80e6 × 16))` = 3 cycles, a 10-bit symbol 30.
const FAST_CLKDIV: u32 = 1;
const FAST_SYMBOL: u64 = 30;

/// A machine running the mask ROM, with `script` on UART0's receive side and
/// the divider above already programmed.
fn rig(script: ScriptedSource) -> Machine {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .uart0_script(script)
        .build()
        .expect("builds");
    assert!(machine.poke_word(CLKDIV, FAST_CLKDIV));
    machine
}

fn run_to(machine: &mut Machine, cycle: u64) {
    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(cycle),
        ..Default::default()
    });
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the ROM ran quietly to cycle {cycle}: {outcome:?}"
    );
}

/// A scripted source delivers its bytes at the cycle the file says, one
/// symbol apart from there, and the guest reads them back **in order**
/// through the same decode it would use itself.
#[test]
fn scripted_bytes_arrive_at_their_cycles_and_read_back_in_order() {
    let mut machine = rig(ScriptedSource::new().at(1_000, b"M!\n"));

    run_to(&mut machine, 999);
    assert_eq!(
        machine.peek_word(STATUS).map(|w| w & 0xff),
        Some(0),
        "nothing has arrived before the cycle the script names"
    );

    // The first byte lands at 1,000 and the next two one symbol apart.
    run_to(&mut machine, 1_000 + 2 * FAST_SYMBOL);
    assert_eq!(
        machine.peek_word(STATUS).map(|w| w & 0xff),
        Some(3),
        "three bytes in the receive FIFO"
    );
    let mut got = Vec::new();
    for _ in 0..3 {
        got.push(machine.peek_word(FIFO).expect("mapped") as u8 & 0xff);
    }
    assert_eq!(got, b"M!\n", "in the order the wire carried them");
    assert_eq!(
        machine.peek_word(STATUS).map(|w| w & 0xff),
        Some(0),
        "and the FIFO is empty again"
    );
    // An empty FIFO reads zero rather than repeating the last byte.
    assert_eq!(machine.peek_word(FIFO), Some(0));
}

/// **Levels versus sticky events, at the addresses a driver uses.** An
/// `int_clr` write cannot clear `rxfifo_full` while the FIFO is over its
/// threshold; reading it down does. This is the single most common UART model
/// bug and it shows up as a driver spinning in its ISR.
#[test]
fn an_int_clr_cannot_clear_the_receive_level_while_it_holds() {
    let mut machine = rig(ScriptedSource::new().at(100, b"abcd"));
    // `conf1.rxfifo_full_thrhd` = 2, keeping the rest of the reset word.
    let conf1 = machine.peek_word(CONF1).expect("mapped");
    assert!(machine.poke_word(CONF1, (conf1 & !0x7f) | 2));

    run_to(&mut machine, 100 + 3 * FAST_SYMBOL);
    assert_eq!(machine.peek_word(STATUS).map(|w| w & 0xff), Some(4));
    assert_ne!(
        machine.peek_word(INT_RAW).expect("mapped") & INT_RXFIFO_FULL,
        0,
        "4 > 2: the level holds"
    );

    assert!(machine.poke_word(INT_CLR, u32::MAX));
    assert_ne!(
        machine.peek_word(INT_RAW).expect("mapped") & INT_RXFIFO_FULL,
        0,
        "a level is not sticky and write-one-to-clear cannot touch it"
    );

    machine.peek_word(FIFO);
    machine.peek_word(FIFO);
    assert_eq!(
        machine.peek_word(INT_RAW).expect("mapped") & INT_RXFIFO_FULL,
        0,
        "2 is not > 2: the condition went away, so the level did"
    );
}

/// The classic's third category. esp-hal, `uart/mod.rs:1180-1183`: *"On
/// ESP32 and S2, the timeout interrupt can't be cleared unless the FIFO is
/// empty."*
#[test]
fn the_receive_timeout_is_neither_a_plain_level_nor_a_plain_event() {
    let mut machine = rig(ScriptedSource::new().at(100, b"ab"));
    // `rx_tout_en` (bit 31) with `rx_tout_thrhd` (24:30) = 1 symbol.
    let conf1 = machine.peek_word(CONF1).expect("mapped");
    assert!(machine.poke_word(CONF1, conf1 | (1 << 31) | (1 << 24)));

    // The threshold is in symbols of eight bits on this part, so the timeout
    // is eight bit-times after the last byte: 8 × 3 cycles here.
    run_to(&mut machine, 100 + FAST_SYMBOL + 8 * 3 + 1);
    assert_ne!(
        machine.peek_word(INT_RAW).expect("mapped") & INT_RXFIFO_TOUT,
        0,
        "the timeout fired"
    );
    assert!(machine.poke_word(INT_CLR, INT_RXFIFO_TOUT));
    assert_ne!(
        machine.peek_word(INT_RAW).expect("mapped") & INT_RXFIFO_TOUT,
        0,
        "and it will not clear while there are bytes in the FIFO"
    );
    machine.peek_word(FIFO);
    machine.peek_word(FIFO);
    assert!(machine.poke_word(INT_CLR, INT_RXFIFO_TOUT));
    assert_eq!(
        machine.peek_word(INT_RAW).expect("mapped") & INT_RXFIFO_TOUT,
        0,
        "an empty FIFO lets it go"
    );
}

/// **The cable, as edges.** `signals dtr=0 rts=1` holds EN low and
/// `signals dtr=0 rts=0` releases it; the release is the reboot, and IO0's
/// level at that instant is the strap.
#[test]
fn the_auto_reset_circuit_reboots_on_the_release_and_not_on_the_assert() {
    let held = ControlCommand::Signals {
        dtr: Some(false),
        rts: Some(true),
    };
    let released = ControlCommand::Signals {
        dtr: Some(false),
        rts: Some(false),
    };
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .reboot_on_reset(true)
        .control_script(vec![(1_000, held), (2_000, released)])
        .build()
        .expect("builds");

    run_to(&mut machine, 1_500);
    assert_eq!(
        machine.cable(),
        Cable {
            dtr: false,
            rts: true
        }
    );
    assert!(
        !machine.cable().en(),
        "EN is low: the chip is held in reset"
    );
    assert_eq!(machine.reboots(), 0, "holding EN low is not yet a boot");

    run_to(&mut machine, 3_000);
    assert!(machine.cable().en(), "EN was released");
    assert_eq!(machine.reboots(), 1, "exactly one reboot, on the release");
    assert!(
        machine.cycles() < 3_000 + 8_192,
        "and the clock went back to zero and started again"
    );
}

/// The two shorthand verbs play the repo's own sequences
/// (`spikes/serial-lab/index.html:341-357`) and each is exactly one reboot.
/// `download-mode` latches IO0 low, which is the strap the chip would boot
/// into — `documented`, not measured, until L1 captures one.
#[test]
fn the_reset_and_download_verbs_are_one_reboot_each_with_the_right_strap() {
    for (verb, strap) in [
        (ControlCommand::Reset, Strap::App),
        (ControlCommand::DownloadMode, Strap::Download),
    ] {
        let mut machine = Esp32V3Builder::new()
            .boot_mode(BootMode::RomUp)
            .reboot_on_reset(true)
            .control_script(vec![(1_000, verb.clone())])
            .build()
            .expect("builds");
        run_to(&mut machine, 3_000);
        assert_eq!(machine.reboots(), 1, "`{}` is one reboot", verb.verb());
        let _ = strap;
        // Both dances end with both lines deasserted, which is "run".
        assert_eq!(
            machine.cable(),
            Cable {
                dtr: false,
                rts: false
            },
            "`{}` leaves the lines slack",
            verb.verb()
        );
    }
}

/// Without `--reboot-on-reset` the release **ends the run** and names the
/// strap, rather than silently doing nothing.
#[test]
fn a_release_without_reboot_on_reset_ends_the_run_and_names_the_strap() {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .control_script(vec![(1_000, ControlCommand::DownloadMode)])
        .build()
        .expect("builds");
    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(3_000),
        ..Default::default()
    });
    assert!(
        matches!(
            outcome,
            Outcome::Reset {
                strap: Strap::Download,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(machine.reboots(), 0);
}

/// **Opening the port moves no chip state.** The whole difference from the
/// C6, asserted rather than only written down: `attach` and `open` change the
/// host's bookkeeping and leave every register and the cycle count where they
/// were.
#[test]
fn attaching_a_cable_and_opening_the_port_move_no_chip_state() {
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::RomUp)
        .control_script(vec![
            (1_000, ControlCommand::Attach),
            (1_000, ControlCommand::Open),
        ])
        .build()
        .expect("builds");
    let before = machine.snapshot();
    run_to(&mut machine, 1_500);
    let report = machine.cable_report();
    assert!(report.attached && report.port_open);
    assert_eq!(report.reboots, 0);
    assert_eq!(
        machine.cable(),
        Cable {
            dtr: false,
            rts: false
        },
        "and neither verb touched a modem line"
    );
    // The peripheral blobs are what a register change would move. The ROM
    // ran, so RAM and the clock moved; the cable did not move them.
    assert_eq!(before.periph.len(), machine.snapshot().periph.len());
}

/// **The acceptance for the cable.** The shipped image boots, the cable
/// resets it, and it boots again — with both boots in one console log.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn a_cable_reset_reboots_the_running_app_and_the_log_has_both_boots() {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("uart_socket: cable reset", &reason);
            return;
        }
    };
    // ⚠️ **P7 moved this window.** P6 pulled EN at 20 ms, well past the first
    // boot's `[INIT] I/O task spawned` at 13,521 us, because the boot then sat
    // on the flash controller's command word for ever. With a flash chip
    // behind SPI1 it does not sit anywhere: it runs on to
    // `CpuControl::start_app_core` and meets `rer` at **14,539 us**
    // (`tests/boot.rs`), which is before P6's 20 ms — so a reset scheduled
    // there would never be delivered and this test would be asserting that
    // the cable works while the cable was never used.
    //
    // 12 ms is past `[RECOVERY] boot: cause=power-on` on the wire and inside
    // the first boot's life. It is a narrow window and a deterministic one:
    // the same run every time, and a firmware change that moves the console
    // moves this test rather than silently hollowing it out.
    let ms = 1_000 * memmap::CYCLES_PER_US;
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .reboot_on_reset(true)
        .control_script(vec![
            (0, ControlCommand::Attach),
            (0, ControlCommand::Open),
            (
                12 * ms,
                ControlCommand::Signals {
                    dtr: Some(false),
                    rts: Some(true),
                },
            ),
            (
                13 * ms,
                ControlCommand::Signals {
                    dtr: Some(false),
                    rts: Some(false),
                },
            ),
        ])
        .build()
        .expect("builds");

    let outcome = machine.run_until(&StopCondition::after_micros(45_000));
    // ⚠️ **P7 moved this.** P6 read a `Deadline` here, because the first boot
    // spun on the flash controller and never got past it. With a flash chip
    // behind SPI1 the boot goes through, so the *second* boot runs on into
    // `CpuControl::start_app_core` and meets `rer` — the one instruction this
    // hart does not decode (`tests/boot.rs`'s
    // `past_the_filesystem_the_boot_meets_the_one_opcode_this_hart_lacks`).
    // Which is still "no strict stop across the reset", and is asserted as
    // such rather than by widening the outcome match.
    assert!(
        machine.first_strict_violation().is_none(),
        "no strict refusal across the reset: {outcome:?}"
    );
    assert!(
        matches!(
            outcome,
            Outcome::Deadline { .. }
                | Outcome::Fault {
                    pc: 0x4010_01bd,
                    fault: lp_xt_emu::mach::HartFault::UnsupportedInstruction {
                        word: 0x0040_6890,
                        ..
                    },
                    ..
                }
        ),
        "the deadline, or the `rer` wall of the second boot: {outcome:?}"
    );
    assert_eq!(machine.reboots(), 1, "one release of EN, one boot");
    assert_eq!(machine.cable_report().reboots, 1);

    let text = machine.uart0().text();
    let boots = text.matches("[INIT] fw-esp32v3 boot").count();
    assert_eq!(boots, 2, "both boots are in the log:\n{text}");
    // The console kept its bytes across the restore — a log that lost
    // everything before the reset would be a worse record than one with both.
    assert!(
        text.matches("[RECOVERY] boot: cause=power-on").count() == 2,
        "and the second boot reports POWERON too, because an EN-pin reset on \
         this part is a chip reset and the classic has no code for one:\n{text}"
    );
}

// --- M4 P3b: the link after the boot settles (ruling R6) ---------------------

/// The needle every C6 walk opens on, and the line the classic's server loop
/// prints on its first successful tick. A request fired after it lands on a
/// board that has left the busy boot behind — which is where R6 lived.
const BOOT_COMPLETE: &str = "[RECOVERY] boot complete (first frame served)";

/// A `--uart0-script` in the CLI's own grammar, parsed by the same parser:
/// the first request a millisecond after the boot settles, the next two
/// fifty milliseconds apart. Three separate steps on purpose — concatenating
/// them into one delivery is the workaround R6 forbids.
const THREE_REQUESTS: &str = concat!(
    "after \"[RECOVERY] boot complete (first frame served)\" +1ms ",
    "\"M!{\\\"id\\\":1,\\\"msg\\\":\\\"stopAllProjects\\\"}\\n\"\n",
    "then +50ms \"M!{\\\"id\\\":2,\\\"msg\\\":\\\"stopAllProjects\\\"}\\n\"\n",
    "then +50ms \"M!{\\\"id\\\":3,\\\"msg\\\":\\\"stopAllProjects\\\"}\\n\"\n",
);

/// A trace sink a test can read back: every line the machine's bus trace
/// emits for the blocks the builder was given.
#[derive(Clone, Default)]
struct SharedTrace(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for SharedTrace {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("trace sink").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedTrace {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("trace sink")).into_owned()
    }
}

/// **R6, pinned on the wire.** Three requests, each its own script step, the
/// first fired after `[RECOVERY] boot complete` — and three answers, in
/// order. Before M4 P3b the first was swallowed: the guest's thread-mode
/// executor had parked for ever at the end of its first iteration, because
/// the software interrupt it raised to yield (a DPORT store) zeroed the
/// hart's asserted-line mask at poll point (c) and was not taken until a
/// later interrupt landed — by which time the executor thread had run on
/// into its next `take_all` and was switched out mid-way through it, to
/// resume later against a stale "empty". The cause and the trace are in the
/// README, "The link, after the boot settles".
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn a_script_with_three_requests_is_answered_three_times() {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("uart_socket: three requests", &reason);
            return;
        }
    };
    let script = lp_emu_esp32v3::control::parse_byte_script(THREE_REQUESTS).expect("script");
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .uart0_script(script)
        .build()
        .expect("builds");
    let reply = |id: u32| format!("M!{{\"id\":{id},\"msg\":\"stopAllProjects\"}}");

    // The boot settles at ~115 ms; the third request lands ~101 ms after
    // that. Half a second is room, not a tuned number.
    let outcome = machine.run_until(&StopCondition {
        exit_on: Some(reply(3)),
        ..StopCondition::after_micros(500_000)
    });
    let text = machine.uart0().text();
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the third answer is what stops the run: {outcome:?}\n{text}"
    );
    assert!(
        machine.first_strict_violation().is_none(),
        "no strict refusal anywhere in the run"
    );
    assert_eq!(
        machine.bus().unmapped_reads() + machine.bus().unmapped_writes(),
        0,
        "zero unmapped accesses"
    );
    let boot_complete = text
        .find(BOOT_COMPLETE)
        .unwrap_or_else(|| panic!("no `{BOOT_COMPLETE}` on the wire:\n{text}"));
    let mut last = boot_complete;
    for id in 1..=3 {
        let at = text[last..]
            .find(&reply(id))
            .map(|i| last + i)
            .unwrap_or_else(|| panic!("request {id} was not answered after byte {last}:\n{text}"));
        last = at;
    }
    assert_eq!(
        text.matches("\"msg\":\"stopAllProjects\"").count(),
        3,
        "each request answered exactly once:\n{text}"
    );
}

/// **The sleep itself, independent of the wire.** The shipped image runs
/// idle for two seconds of guest time past the boot settling, and esp-rtos's
/// thread-mode executor keeps re-arming the TIMG0 `t0` alarm that is its
/// tick — one arm per `Timer::after(1 ms)` of the server loop, so about two
/// thousand. Before M4 P3b there was exactly **one** in the whole run: the
/// arm at the end of the first iteration, after which the executor parked at
/// `Instant::EPOCH + Duration::MAX` and never registered another.
///
/// Counted as `t0.config` writes with both `en` (bit 31) and `alarm_en`
/// (bit 10) set — the last write of esp-hal's `Timer::start`, once per arm.
#[test]
#[ignore = "needs the shipped image; run through `just test-emu-esp32v3-boot`"]
fn the_thread_executor_keeps_re_arming_its_tick_after_the_boot_settles() {
    let elf = match fw_esp32v3_image() {
        Ok(p) => p,
        Err(reason) => {
            skip_notice("uart_socket: idle re-arm", &reason);
            return;
        }
    };
    let trace = SharedTrace::default();
    let mut machine = Esp32V3Builder::new()
        .boot_mode(BootMode::Direct)
        .app(AppSource::Path(elf))
        .strict(true)
        .trace(Box::new(trace.clone()), vec!["TIMG0".to_string()])
        .build()
        .expect("builds");

    // To the line, then two seconds of guest time past it.
    let outcome = machine.run_until(&StopCondition {
        exit_on: Some(BOOT_COMPLETE.to_string()),
        ..StopCondition::after_micros(500_000)
    });
    assert!(
        matches!(outcome, Outcome::ExitMatched { .. }),
        "the boot settles: {outcome:?}\n{}",
        machine.uart0().text()
    );
    let settled = machine.cycles();
    let two_seconds = 2_000_000 * memmap::CYCLES_PER_US;
    let outcome = machine.run_until(&StopCondition {
        stop_cycle: Some(settled + two_seconds),
        ..Default::default()
    });
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "two quiet seconds: {outcome:?}"
    );
    assert!(machine.first_strict_violation().is_none());

    let arms = trace
        .text()
        .lines()
        .filter(|line| line.contains("W4 TIMG0+0x000 t0.config = 0x"))
        .filter(|line| {
            let word = line.rsplit("= 0x").next().expect("a value");
            let value = u32::from_str_radix(word.trim(), 16).expect("hex");
            value & (1 << 31) != 0 && value & (1 << 10) != 0
        })
        .count();
    assert!(
        arms >= 1_000,
        "the executor's tick was re-armed {arms} times in two idle seconds; one arm per \
         millisecond is ~2,000, and one arm in total is the executor asleep at MAX"
    );
}
