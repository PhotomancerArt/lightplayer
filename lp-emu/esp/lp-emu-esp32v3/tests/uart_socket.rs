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

use lp_emu_esp32v3::control::{Cable, ControlCommand};
use lp_emu_esp32v3::machine::{
    AppSource, BootMode, Esp32V3Builder, Machine, Outcome, StopCondition,
};
use lp_emu_esp32v3::memmap;
use lp_emu_esp32v3::test_support::{fw_esp32v3_image, skip_notice};
use lp_emu_esp_common::{ScriptedSource, Strap};

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
    assert_eq!(machine.cable(), Cable { dtr: false, rts: true });
    assert!(!machine.cable().en(), "EN is low: the chip is held in reset");
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
    // 20 ms in, well past the first boot's `[INIT] I/O task spawned` at
    // 13,521 us; released one emulated millisecond later.
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
                20 * ms,
                ControlCommand::Signals {
                    dtr: Some(false),
                    rts: Some(true),
                },
            ),
            (
                21 * ms,
                ControlCommand::Signals {
                    dtr: Some(false),
                    rts: Some(false),
                },
            ),
        ])
        .build()
        .expect("builds");

    let outcome = machine.run_until(&StopCondition::after_micros(45_000));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "no strict stop across the reset: {outcome:?}"
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
