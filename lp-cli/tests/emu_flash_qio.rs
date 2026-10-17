//! The core's flash read mode on `lp-emu:esp32c6`: the shipped split image
//! booted from the reset vector (the real mask ROM, the IDF bootloader, the
//! loader, the core), with lp-cli's in-process host on its link, once per
//! flash part.
//!
//! `fw-esp32c6`'s `flash_qio` moves the read path to QIO only on a listed part
//! whose QE bit is already set, and never writes a status register. Each test
//! names the part (`Esp32C6Builder::flash_part`) and reads three things: the
//! core's one `[flash]` line (on the log channel, so here and not in the
//! emulator's own crate — the MIT fence), the cache controller's
//! `SPI_MEM_CTRL` read-mode bits after the boot, and the chip's count of
//! status-register writes. The engine's hello must still come, so the core
//! and the engine ran from flash read the chosen way.
//!
//! What this proves is the switch's decisions and the ROM's register writes
//! as the real mask ROM makes them. The emulator moves the same bytes
//! whatever the bus width (`periph/spi1.rs`, "What is *not* modelled"), so a
//! wrong dummy count or a miswired pad would pass here: that is silicon's
//! question (CX1 and CX2, PR #1172).
//!
//! `#[ignore]`d: it needs a split build (`LP_EMU_BUILD_FW=1`), and `just
//! test-emu-c6-cli-boards` runs it.

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::flash::{FlashBacking, FlashPart};
use lp_emu_esp32c6::loader::ResetCause;
use lp_emu_esp32c6::machine::{AppSource, BootMode, Esp32C6Builder, UsbHost};
use lp_emu_esp32c6::test_support::{FwImage, SplitImage, split_image};

const NONCE: u32 = 0x0F1A_5410;
/// The engine's hello, which every boot here must reach.
const HELLO: &str = "M!{\"id\":0,\"msg\":{\"hello\":{\"proto\":";
/// `SPI_MEM` 0's `CTRL`, and its read-mode bits.
const SPI0_CTRL: u32 = 0x6000_2008;
const FREAD_DIO: u32 = 1 << 23;
const FREAD_QIO: u32 = 1 << 24;

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli-boards` runs it"]
fn a_desk_part_with_qe_set_is_read_in_qio_and_nothing_writes_its_status() {
    for (part, id) in [(FlashPart::CX1, "164046"), (FlashPart::XIAO_C6, "164020")] {
        let Some(run) = boot(part) else { return };
        run.expect_line(&format!("[flash] QIO (jedec {id}, sr2 02)"));
        assert_eq!(
            run.ctrl & (FREAD_DIO | FREAD_QIO),
            FREAD_QIO,
            "the cache reads in QIO"
        );
        assert_eq!(run.status_writes, 0, "no status-register write");
    }
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli-boards` runs it"]
fn a_part_off_the_list_stays_dio() {
    let Some(run) = boot(FlashPart::ROM_DEFAULT) else {
        return;
    };
    run.expect_line("[flash] DIO: part not on the QIO list (jedec 1640ef, sr2 00)");
    assert_eq!(run.ctrl & (FREAD_DIO | FREAD_QIO), FREAD_DIO);
    assert_eq!(run.status_writes, 0);
}

#[test]
#[ignore = "needs a split fw-esp32c6 build; `just test-emu-c6-cli-boards` runs it"]
fn a_listed_part_with_qe_clear_stays_dio_and_qe_is_never_set() {
    let Some(run) = boot(FlashPart {
        status2: 0,
        ..FlashPart::CX1
    }) else {
        return;
    };
    run.expect_line("[flash] DIO: QE bit clear (jedec 164046, sr2 00)");
    assert_eq!(run.ctrl & (FREAD_DIO | FREAD_QIO), FREAD_DIO);
    assert_eq!(
        run.status_writes, 0,
        "the switch reads QE and never sets it — nor does the ROM it calls"
    );
}

/// One boot's facts.
struct Run {
    console: String,
    ctrl: u32,
    status_writes: u64,
}

impl Run {
    fn expect_line(&self, line: &str) {
        let at = self
            .console
            .find(line)
            .unwrap_or_else(|| panic!("no {line:?} in:\n{}", self.console));
        let hello = self
            .console
            .find(HELLO)
            .unwrap_or_else(|| panic!("no hello in:\n{}", self.console));
        assert!(at < hello, "the core chose before the engine spoke");
        assert_eq!(
            self.console.matches("[flash] ").count(),
            1,
            "one line a boot:\n{}",
            self.console
        );
    }
}

/// The shipped split image, ROM-up, on `part`, run to the engine's hello.
fn boot(part: FlashPart) -> Option<Run> {
    let split = match split_image(&FwImage::SHIPPED) {
        Ok(split) => split,
        Err(reason) => {
            eprintln!("emu_flash_qio: skipped — {reason}");
            return None;
        }
    };
    let mut host = hosted(&split, part);
    host.run_until(3_000_000, Some(HELLO)).expect("the run");
    let console = host.console().join("\n");
    assert_eq!(host.link_errors, 0, "{console}");
    let machine = &mut host.board.machine;
    let ctrl = machine.peek_word(SPI0_CTRL).expect("SPI0 CTRL reads");
    let status_writes = machine.flash().lock().unwrap().status_writes;
    println!(
        "flash part {part:?} (lp-emu:esp32c6:t1): {} · SPI0 CTRL {ctrl:#010x} · {status_writes} status writes",
        console
            .lines()
            .find(|l| l.contains("[flash] "))
            .unwrap_or("(no [flash] line)")
    );
    Some(Run {
        console,
        ctrl,
        status_writes,
    })
}

fn hosted(split: &SplitImage, part: FlashPart) -> EmuLinkHost<C6Board> {
    let len = std::fs::metadata(split.merged()).unwrap().len() as u32;
    let machine = Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .app(AppSource::Path(split.p2_elf()))
        .flash(FlashBacking::Copy(split.merged()))
        .flash_len(len)
        .flash_part(part)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::App)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj_queue_source()
        .build()
        .expect("the split image builds a machine");
    EmuLinkHost::new(C6Board::new(machine).expect("a hosted board"), NONCE, true)
}
