//! M3 P1's gates: the mask ROM's download console **answers**, on both
//! consoles, with nothing placed in memory and no hook installed.
//!
//! The chip boots from its reset vector with the download strap, prints
//! `waiting for download`, and is then driven the way esptool drives it: a
//! SLIP `SYNC` (`0x08`) and a `READ_REG` (`0x0a`) of `EFUSE_RD_MAC_SPI_SYS_0`
//! (`0x600b_0844`). The replies are the ROM's own — eight SYNC responses
//! carrying the payload's `0x2012_0707`, then the MAC's low four bytes as
//! `--efuse-mac` gave them to the run. Two MACs give two replies, so the
//! gate cannot pass on a constant.
//!
//! # Why this needs no firmware build
//!
//! The download console never opens the flash: the ROM's `main` takes the
//! strap to `detect_uart_usb_spi_sdio_boot_mode` → `ets_uart_download` and
//! stays there. So the flash is blank, the app is none, and these tests are
//! **not** `#[ignore]`d — they run under a bare `cargo test` and in every
//! CI job that builds this crate.
//!
//! # What the trace settled (docs/reports/2026-09-08-esp32c6-rom-console-trace.md)
//!
//! Nothing in the register model was wrong. The ROM's detector alternates
//! between UART0 (`uart_baudrate_detect`) and the USB console
//! (`usb_serial_device_rx_one_char` at `0x4002_28d8`, polling
//! `USB_DEVICE.ep1_conf` bit 2 and reading `ep1`), and the M6 model drives
//! both registers the way silicon does. The committed
//! `scripts/rom-download-sync.usb` simply never delivered its bytes: its
//! stamps were written as microseconds under a grammar that reads
//! milliseconds, so the SYNC was scheduled for twenty *seconds* in, past the
//! end of every run that attached it — and no test ever attached it. With
//! the stamps read as the grammar reads them, the real ROM answers on the
//! first try.
//!
//! # UART0
//!
//! On UART0 the ROM measures the host's baud first (`periph/uart.rs`,
//! "Auto-baud"), and the SYNC that carries the 128th edge is consumed by
//! that measurement, so `scripts/rom-download-sync.uart0` sends two SYNCs
//! — as esptool does — and the second is the one answered. The stated host
//! baud is the peripheral's default, 115,200; the `--uart0-baud` flag is
//! M3 P2's (the builder's plumbing was queued behind M2 P1), and the
//! computed divisor is pinned here from the counters' arithmetic.
//!
//! # Strict
//!
//! Every run here is `--strict-bus`, and `--strict-grade documented`: the
//! console path reads UART0's auto-baud counters on every loop of the
//! detector — on both consoles — and those registers are the PAC's
//! statement computed, not a transcript's measurement, so `documented` is
//! the highest level the registers on this path support. The scope the run
//! checked is asserted by name.

use lp_emu_esp_common::{RegGrade, ScriptedSource, Strap};
use lp_emu_esp32c6::control::{parse_byte_script, parse_usb_script};
use lp_emu_esp32c6::loader::{EfuseIdentity, ResetCause};
use lp_emu_esp32c6::machine::{
    BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink, UsbHost,
};

/// The desk board's MAC, and a second one that differs in every low byte.
const DESK_MAC: [u8; 6] = [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c];
const OTHER_MAC: [u8; 6] = [0x40, 0x4c, 0xca, 0x11, 0x22, 0x33];

/// The console's banner, as `tests/rom_up_boot.rs` pins it.
const BANNER: &[u8] = b"ESP-ROM:esp32c6-20220919\r\nBuild:Sep 19 2022\r\n\
    rst:0x15 (USB_UART_HPSYS),boot:0x16 (DOWNLOAD(USB/UART0/SDIO_REI_FEO))\r\n\
    waiting for download\r\n";

/// One SLIP-framed SYNC response: direction 1, command 0x08, size 4, value =
/// the SYNC payload's first word (`0x2012_0707`), status `00 00`, and two
/// more zero bytes the ROM's 12-byte reply buffer carries.
const SYNC_REPLY: &[u8] = &[
    0xc0, 0x01, 0x08, 0x04, 0x00, 0x07, 0x07, 0x12, 0x20, 0x00, 0x00, 0x00, 0x00, 0xc0,
];
/// `UartConnCheck` sends the SYNC response eight times.
const SYNC_REPLIES: usize = 8;

/// The READ_REG reply: direction 1, command 0x0a, size 4, value = the
/// register's word, then a zero status.
fn read_reg_reply(mac: [u8; 6]) -> Vec<u8> {
    // `EFUSE_RD_MAC_SPI_SYS_0` holds the MAC's low four bytes, little-endian,
    // as the block serves them: mac[5] in bits 0:7 … mac[2] in bits 24:31.
    let mut reply = vec![0xc0, 0x01, 0x0a, 0x04, 0x00, mac[5], mac[4], mac[3], mac[2]];
    reply.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0xc0]);
    reply
}

fn expected_console_output(mac: [u8; 6]) -> Vec<u8> {
    let mut out = BANNER.to_vec();
    for _ in 0..SYNC_REPLIES {
        out.extend_from_slice(SYNC_REPLY);
    }
    out.extend(read_reg_reply(mac));
    out
}

fn efuse(mac: [u8; 6]) -> EfuseIdentity {
    EfuseIdentity {
        mac,
        ..EfuseIdentity::default()
    }
}

fn script_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join(name)
}

/// A download-strap machine: the real ROM, a blank chip, the strict bus and
/// the strict grade, and a USB host with the port open.
fn download_console(mac: [u8; 6]) -> Esp32C6Builder {
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::Download)
        .efuse(efuse(mac))
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .strict(true)
        .strict_grade(Some(RegGrade::Documented))
}

fn run(mut m: Esp32C6Machine, micros: u64) -> Esp32C6Machine {
    let outcome = m.run_until(&StopCondition::after_micros(micros));
    assert!(
        matches!(outcome, Outcome::Deadline { .. }),
        "the run did not reach its deadline: {outcome:?}"
    );
    assert!(
        m.bus.first_strict_violation().is_none(),
        "{:?}",
        m.bus.first_strict_violation()
    );
    assert_eq!(
        m.bus.unmapped_reads() + m.bus.unmapped_writes(),
        0,
        "unmapped"
    );
    assert!(m.hooks().is_empty(), "the ROM hook table is not empty");
    assert_eq!(m.hook_calls(), 0);
    // The scope the strict grade actually checked, by name.
    assert_eq!(
        m.bus.blocks_in_strict_grade_scope(),
        vec!["UART0", "UART1", "USB_DEVICE"],
        "the blocks that publish a grade table"
    );
    m
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The console's bytes after the banner, or the whole log when the banner
/// is not where it should be.
fn after_banner(log: &[u8]) -> &[u8] {
    match log.strip_prefix(BANNER) {
        Some(rest) => rest,
        None => panic!(
            "the console did not start with the banner:\n{}",
            String::from_utf8_lossy(log)
        ),
    }
}

fn usb_console(mac: [u8; 6]) -> Esp32C6Machine {
    let text = std::fs::read_to_string(script_path("rom-download-sync.usb")).expect("the script");
    let script = parse_usb_script(&text).expect("the script parses");
    assert!(
        script.commands.is_empty(),
        "the sync script is bytes only; the cable is the builder's"
    );
    let m = download_console(mac)
        .usb_script_source(script.bytes)
        .build()
        .expect("the download-console machine builds");
    // The READ_REG lands at 40 ms; its reply is on the wire within the
    // millisecond. 60 ms is comfortable and short.
    run(m, 60_000)
}

/// G1-2 and G1-3 on the USB console: the SYNC reply, and the READ_REG reply
/// carrying the MAC this run was given.
#[test]
fn the_usb_console_answers_sync_and_read_reg_with_the_runs_own_mac() {
    let m = usb_console(DESK_MAC);
    let usb = m.usb_sj().bytes();
    let replies = after_banner(&usb);
    assert_eq!(
        hex(replies),
        hex(&expected_console_output(DESK_MAC)[BANNER.len()..]),
        "eight SYNC replies then the READ_REG reply, on the USB link"
    );
    // The ROM writes the banner to UART0 as well, and nothing else there:
    // the console byte selected USB, so the replies go where the SYNC came
    // from.
    assert_eq!(m.uart0().bytes(), BANNER, "UART0 carries the banner only");
    // Nothing was loaded and nothing was hooked: the ROM did this itself.
    assert!(m.app_segments().is_empty());
}

/// G1-3's second half: change the MAC and the READ_REG reply changes with it,
/// so the gate is not passing on a constant.
#[test]
fn a_different_efuse_mac_gives_a_different_read_reg_reply() {
    let desk = usb_console(DESK_MAC).usb_sj().bytes();
    let other = usb_console(OTHER_MAC).usb_sj().bytes();
    assert_eq!(hex(&desk), hex(&expected_console_output(DESK_MAC)));
    assert_eq!(hex(&other), hex(&expected_console_output(OTHER_MAC)));
    assert_ne!(desk, other);
    // The two logs differ in exactly the four MAC bytes of the last reply.
    let diff: Vec<usize> = desk
        .iter()
        .zip(other.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();
    let value_at = desk.len() - read_reg_reply(DESK_MAC).len() + 5;
    assert_eq!(diff, (value_at..value_at + 4).collect::<Vec<_>>());
}

/// G1-4: the same exchange on UART0, through the auto-baud model at the
/// stated host baud (the peripheral's default, 115,200). Two SYNCs go in —
/// the ROM spends the first measuring the baud — and the second is answered,
/// then READ_REG.
#[test]
fn the_uart0_console_answers_after_auto_baud_at_115200() {
    let text = std::fs::read_to_string(script_path("rom-download-sync.uart0")).expect("the script");
    let script: ScriptedSource = parse_byte_script(&text).expect("the script parses");
    let m = download_console(DESK_MAC)
        .uart0_script(script)
        .build()
        .expect("the download-console machine builds");
    // The READ_REG lands at 220 ms and takes 1.2 ms on the wire; the reply
    // follows within the ROM's next loop.
    let m = run(m, 240_000);

    let uart0 = m.uart0().bytes();
    let replies = after_banner(&uart0);
    assert_eq!(
        hex(replies),
        hex(&expected_console_output(DESK_MAC)[BANNER.len()..]),
        "eight SYNC replies then the READ_REG reply, on UART0"
    );
    // The USB link carries the banner only: the console byte selected UART0.
    assert_eq!(m.usb_sj().bytes(), BANNER, "USB carries the banner only");

    // The divisor the ROM computed from the counters. At 115,200 from the
    // 40 MHz XTAL one bit is 347 clocks (347.2 floored), `lowpulse` =
    // `highpulse` = 347, and the ROM's `((low + high) << 3) + 16` = 5568
    // sixteenths → `clkdiv` = 348, `frag` = 0 → 40e6 × 16 / 5568 = 114,943
    // baud. `uart_div_modify` wrote it; the block reports what it is running
    // at.
    let mut m = m;
    let clkdiv = m
        .peek_word(lp_emu_esp32c6::memmap::periph::UART0 + 0x14)
        .expect("UART0.clkdiv");
    assert_eq!(clkdiv & 0xfff, 348, "clkdiv = 0x{clkdiv:08x}");
    assert_eq!((clkdiv >> 20) & 0xf, 0, "frag = 0x{clkdiv:08x}");
    // That the divisor follows the *host's* rate rather than a constant is
    // pinned at the peripheral, where the host baud can be stated:
    // `periph::uart::tests::the_auto_baud_counters_follow_the_stated_host_baud`
    // (921,600 → 43 clocks a bit → 704 sixteenths → clkdiv 44). The machine
    // grows a `--uart0-baud` in M3 P2.
}
