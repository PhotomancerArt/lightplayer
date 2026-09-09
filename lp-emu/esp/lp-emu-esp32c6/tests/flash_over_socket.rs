//! M3 P2's gate: a REAL flasher writes into the modelled flash, over the
//! socket, through the mask ROM's download console.
//!
//! M3 P1 made the console answer a hand-written SLIP `SYNC` and `READ_REG`.
//! This is the rest of it: espflash 3.3.0's own protocol implementation
//! opens a port, syncs, detects the chip, reads the eFuses, attaches SPI1,
//! erases, programs, and asks the ROM for an MD5 of what it programmed — and
//! the bytes that end up on the chip are the image's.
//!
//! # Why espflash's LIBRARY and not its CLI
//!
//! `espflash --port <path>` does not open `<path>`. It looks the name up in
//! the operating system's serial-port ENUMERATION — `available_ports()`,
//! which is IOKit on macOS and libudev / `/sys/class/tty` on Linux — and
//! refuses a name that is not in the list (espflash 3.3.0,
//! `src/cli/serial.rs`, `find_serial_port` → `espflash::serial_not_found`).
//! A pseudo-terminal is not an enumerated serial device on either platform,
//! and no bridge can change that: the refusal happens before a byte is sent.
//! `scripts/emu/flash-over-socket.sh --client espflash` says so and exits 20.
//!
//! So the flasher is driven the way this repository's own product code
//! drives it — `lp-cli/src/commands/fwcheck/flash.rs` and
//! `lp-app/lpa-link/src/providers/host_serial_esp32/host_esp32_flash.rs`
//! both open the port themselves and hand it to `Flasher::connect`, skipping
//! the same lookup for the same reason (their comment: "fall back to zeros
//! if the port isn't enumerable"). It is espflash 3.3.0's protocol, its
//! framing, its stub and its verification; only the CLI's port picker is
//! skipped.
//!
//! # The shape of a run
//!
//! `serialport::TTYPort::pair()` makes a pty. The flasher gets one end on a
//! worker thread; the machine's byte socket is pumped to the other. The
//! machine runs on the main thread in slices, so a run costs what it costs
//! instead of a fixed emulated deadline.
//!
//! # What is NOT here
//!
//! The whole 4 MiB image, the host's reset dance and the reboot into the
//! app. Those are `scripts/emu/flash-over-socket.sh`: writing four megabytes
//! through a modelled ROM costs minutes of wall clock and the `emu-c6` job
//! has a budget. What CI pays for is the whole console and SPI1 path with a
//! region that crosses sector and page boundaries many times over.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use espflash::connection::reset::{ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::targets::Chip;
use serialport::{SerialPort, TTYPort, UsbPortInfo};

use lp_emu_esp_common::{RegGrade, Strap};
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::{EfuseIdentity, ResetCause};
use lp_emu_esp32c6::machine::{
    BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink, UsbHost,
    UsbSjDrain, UsbSjSink,
};
use lp_emu_esp32c6::test_support::{ReferenceImage, merged_image, skip_notice};

/// The desk board's MAC — the same one `tests/rom_download_console.rs` uses,
/// and what the flasher must read back out of the eFuse block.
const DESK_MAC: [u8; 6] = [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c];

/// Written at `0x0`, so what lands is the second-stage bootloader the ROM
/// would run. 64 KiB crosses sixteen 4 KiB sectors and 256 pages.
const REGION: u32 = 0x1_0000;

/// The strict-grade scope these tests claim, BY NAME. Not "every block with
/// a table": since the accept blocks grade themselves there are twenty-six
/// of those, and the mask ROM writes `PLIC_UX+0x3fc` in `_init` on its way
/// to the console — a register PLIC_UX honestly grades `modeled`, in a block
/// these tests make no claim about. These are the blocks the console and the
/// flasher's own path go through: the two consoles, the strap, and SPI1 —
/// the flash controller the ROM drives for every erase and every program.
///
/// # Why `EFUSE` is NOT in it, though the flasher reads the eFuses
///
/// It was, and the run refused itself. On its way to the console the mask
/// ROM reads `EFUSE+0x30` (`rd_repeat_data0`) at `pc=0x4001_fef8`, and this
/// model answers zero — which is right for a part with nothing burned, but
/// the source is our reading of esp-hal's field table
/// (`efuse/esp32c6/fields.rs`), not a part anybody measured. `modeled` is
/// the honest grade for it, so a run that named `EFUSE` at `documented`
/// would be claiming something this repository cannot show.
///
/// The three words the flasher's chip detection actually reads back —
/// `rd_mac_spi_sys_0`, `_1`, `_3` — *are* graded `documented`, with their
/// source in `periph/efuse.rs`. What stands in for the block-level claim is
/// the assertion below: espflash formats the MAC out of the words it read
/// itself, and it is this run's own `--efuse-mac`. A constant would not
/// survive that. `EFUSE` therefore stays UNGRADED for strict purposes, and
/// says so here rather than being promoted to make a gate green.
const SCOPE: &[&str] = &["UART0", "UART1", "USB_DEVICE", "GPIO", "SPI1"];

fn efuse() -> EfuseIdentity {
    EfuseIdentity {
        mac: DESK_MAC,
        ..EfuseIdentity::default()
    }
}

/// A port nothing else holds: bound, read back and dropped.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
    listener.local_addr().expect("its address").port()
}

/// The machine a flasher talks to: ROM-up from a WRITABLE chip with the
/// download strap, its byte socket listening, and a host with the port open.
///
/// `--merged` is not what a flashing run uses. A merged image is read-only
/// on purpose — it is the image a gate named — and here the image is the
/// flasher's INPUT, not the chip's contents. The chip is a `--flash` file,
/// the backing that keeps a run's writes.
fn download_console(chip: &std::path::Path, addr: &str) -> Esp32C6Machine {
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::Download)
        .efuse(efuse())
        .flash(FlashBacking::File(chip.to_path_buf()))
        .uart0(Uart0Sink::Memory)
        .usb_host(UsbHost::Attached { draining: true })
        .usb_sj(UsbSjSink::Tcp(addr.to_string()))
        .usb_sj_drain(UsbSjDrain::Auto)
        .strict(true)
        .strict_grade(Some(RegGrade::Documented))
        .strict_grade_blocks(Some(SCOPE.to_vec()))
        .build()
        .expect("the download-console machine builds")
}

/// Pump one pty end to the machine's socket and back, until `done`.
///
/// Short timeouts rather than non-blocking reads, so this is a plain loop
/// with no spin: the thread sleeps in one `read` or the other almost all the
/// time.
fn pump(mut tty: TTYPort, mut sock: TcpStream, done: Arc<AtomicBool>) {
    let _ = tty.set_timeout(Duration::from_millis(10));
    let _ = sock.set_read_timeout(Some(Duration::from_millis(10)));
    let mut buf = [0u8; 4096];
    while !done.load(Ordering::Relaxed) {
        match tty.read(&mut buf) {
            Ok(n) if n > 0 => {
                if sock.write_all(&buf[..n]).is_err() {
                    return;
                }
            }
            _ => {}
        }
        match sock.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => {
                if tty.write_all(&buf[..n]).is_err() {
                    return;
                }
            }
            Err(_) => {}
        }
    }
}

/// The port info espflash uses to pick its reset strategy. Zeros, for the
/// same reason `lp-cli`'s `usb_port_info_for` falls back to zeros: a pty is
/// not enumerable, so there is no vid/pid to report — and every reset here
/// is `NoReset` anyway, because a pty carries no DTR or RTS.
fn pty_port_info() -> UsbPortInfo {
    UsbPortInfo {
        vid: 0,
        pid: 0,
        serial_number: None,
        manufacturer: None,
        product: None,
    }
}

/// Run the machine on THIS thread, in slices, while `body` drives espflash
/// on another; stop as soon as the flasher is finished.
///
/// `budget_us` is the emulated ceiling, so a flasher that wedges ends the
/// run instead of hanging the job. The machine's outcome comes back with the
/// body's, because a strict violation must be asserted on the main thread
/// AFTER the worker has been joined — panicking while the worker is blocked
/// in a serial read would hang the test rather than fail it.
fn with_flasher<T, F>(
    m: &mut Esp32C6Machine,
    addr: &str,
    budget_us: u64,
    use_stub: bool,
    body: F,
) -> (T, Option<Outcome>)
where
    T: Send + 'static,
    F: FnOnce(Result<Flasher, String>) -> T + Send + 'static,
{
    let (master, slave) = TTYPort::pair().expect("a pty pair");
    let sock = TcpStream::connect(addr).expect("the machine's byte socket");
    let done = Arc::new(AtomicBool::new(false));
    let pump_done = done.clone();
    let pump_thread = std::thread::spawn(move || pump(master, sock, pump_done));

    let flasher_done = done.clone();
    let worker = std::thread::spawn(move || {
        let flasher = Flasher::connect(
            slave,
            pty_port_info(),
            Some(115_200),
            use_stub,
            /* verify */ false,
            /* skip   */ false,
            Some(Chip::Esp32c6),
            ResetAfterOperation::NoReset,
            // NoReset, not DefaultReset: `DefaultReset` toggles DTR and RTS,
            // and a pty has no modem control lines — the ioctl is accepted
            // and goes nowhere on both macOS and Linux. The chip is already
            // in its download console (the strap says so), which is the
            // state that dance exists to reach.
            ResetBeforeOperation::NoReset,
        )
        .map_err(|e| format!("{e}"));
        let out = body(flasher);
        flasher_done.store(true, Ordering::Relaxed);
        out
    });

    // Slices, not one deadline: `stop_cycle` is ABSOLUTE, so each slice asks
    // for a little more guest time and the loop leaves as soon as the worker
    // says it is finished.
    let slice = 2_000u64 * lp_emu_esp32c6::memmap::CYCLES_PER_US;
    let ceiling = budget_us * lp_emu_esp32c6::memmap::CYCLES_PER_US;
    let mut early = None;
    while !done.load(Ordering::Relaxed) && m.cycles() < ceiling {
        let outcome = m.run_until(&StopCondition {
            stop_cycle: Some((m.cycles() + slice).min(ceiling)),
            ..Default::default()
        });
        if !matches!(outcome, Outcome::Deadline { .. }) {
            early = Some(outcome);
            break;
        }
    }
    done.store(true, Ordering::Relaxed);
    let out = worker.join().expect("the flasher thread");
    let _ = pump_thread.join();
    (out, early)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("lp-emu-m3p2-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir.join("chip.bin")
}

/// Blame the machine before the flasher.
///
/// A strict violation ends the run, and what the flasher then reports is
/// `Error while connecting to device` — the symptom, with the cause on the
/// other side of the socket. So the machine's own complaint is read first
/// and printed with the flasher's.
fn check_flasher(report_error: &Option<String>, m: &Esp32C6Machine, early: &Option<Outcome>) {
    assert!(
        report_error.is_none(),
        "the flasher said {report_error:?}; the machine's first strict violation was {:?} \
         and it stopped with {early:?}",
        m.bus.first_strict_violation(),
    );
}

fn check_clean(m: &Esp32C6Machine, early: Option<Outcome>) {
    assert!(early.is_none(), "the machine stopped early: {early:?}");
    assert!(
        m.bus.first_strict_violation().is_none(),
        "{:?}",
        m.bus.first_strict_violation()
    );
    assert_eq!(
        m.bus.unmapped_reads() + m.bus.unmapped_writes(),
        0,
        "the flasher's path reached an address nothing claims"
    );
    assert!(m.hooks().is_empty(), "the ROM hook table is not empty");
    assert_eq!(m.hook_calls(), 0, "nothing was intercepted");
    assert_eq!(
        m.bus.blocks_in_strict_grade_scope(),
        SCOPE,
        "the blocks this run asked the level about, by name"
    );
}

/// What a run reports back from the flasher thread.
#[derive(Debug, Default)]
struct Report {
    error: Option<String>,
    mac: String,
    chip: String,
    revision: Option<(u32, u32)>,
    md5_written: Option<u128>,
    md5_erased: Option<u128>,
}

/// G2-2's `--no-stub` half, and the shortest statement of the milestone:
/// espflash 3.3.0's own connect sequence drives the real mask ROM's download
/// console, and what it reads back is this run's own eFuses.
///
/// Not `#[ignore]`d: like `tests/rom_download_console.rs`, the download
/// console never opens the flash for this, so no firmware image is needed
/// and a bare `cargo test` runs it.
#[test]
fn espflash_syncs_with_the_mask_rom_and_reads_the_runs_own_efuses() {
    let chip = scratch("info");
    let _ = std::fs::remove_file(&chip);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut m = download_console(&chip, &addr);

    let (report, early) = with_flasher(&mut m, &addr, 20_000_000, false, |flasher| {
        let mut r = Report::default();
        match flasher {
            Err(e) => r.error = Some(e),
            Ok(mut flasher) => match flasher.device_info() {
                Ok(info) => {
                    r.mac = info.mac_address.to_lowercase();
                    r.chip = info.chip.to_string();
                    r.revision = info.revision;
                }
                Err(e) => r.error = Some(format!("device_info: {e}")),
            },
        }
        r
    });

    check_flasher(&report.error, &m, &early);
    // The MAC espflash formatted from the eFuse words it read itself — the
    // MAC this run was given, so the gate cannot pass on a constant.
    assert_eq!(report.mac, "a0:f2:62:87:b4:8c");
    assert_eq!(report.chip, Chip::Esp32c6.to_string());
    assert_eq!(report.revision, Some((0, 2)), "the wafer revision");
    check_clean(&m, early);
    let _ = std::fs::remove_dir_all(chip.parent().unwrap());
}

/// G2-1: the bytes a flasher writes land in the persistent flash backing and
/// are byte-identical to the image, and the ROM's own MD5 of what it
/// programmed is not the MD5 of erased flash.
#[test]
#[ignore = "needs the merged reference image; run through `just test-emu-c6`"]
fn what_espflash_writes_is_byte_identical_to_the_image() {
    let image = match merged_image(&ReferenceImage::SHIPPED_USB_SILICON) {
        Ok(path) => path,
        Err(reason) => return skip_notice("flash_over_socket", &reason),
    };
    let bytes = std::fs::read(&image).expect("the merged image");
    let region = bytes[..REGION as usize].to_vec();

    let chip = scratch("write");
    let _ = std::fs::remove_file(&chip);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut m = download_console(&chip, &addr);

    let payload = region.clone();
    let (report, early) = with_flasher(&mut m, &addr, 600_000_000, false, move |flasher| {
        let mut r = Report::default();
        let Ok(mut flasher) = flasher.map_err(|e| r.error = Some(e)) else {
            return r;
        };
        if let Err(e) = flasher.write_bin_to_flash(0x0, &payload, None) {
            r.error = Some(format!("write_bin_to_flash: {e}"));
            return r;
        }
        // Both digests are the ROM's own (`SPI_FLASH_MD5`, command 0x13),
        // computed over the flash it just programmed and over a region of
        // the same size nobody wrote. A model that answered a constant, or
        // that never actually programmed anything, gives the same number
        // twice.
        match flasher.checksum_md5(0x0, REGION) {
            Ok(sum) => r.md5_written = Some(sum),
            Err(e) => r.error = Some(format!("checksum_md5(written): {e}")),
        }
        match flasher.checksum_md5(0x20_0000, REGION) {
            Ok(sum) => r.md5_erased = Some(sum),
            Err(e) => r.error = Some(format!("checksum_md5(erased): {e}")),
        }
        r
    });

    check_flasher(&report.error, &m, &early);
    check_clean(&m, early);
    assert_ne!(
        report.md5_written, report.md5_erased,
        "the chip's MD5 of what was programmed equals its MD5 of erased flash"
    );

    m.flush_flash().expect("the chip is written back");
    let written = std::fs::read(&chip).expect("the chip file");
    assert_eq!(
        written.len() as u32,
        lp_emu_esp32c6::flash::DEFAULT_FLASH_LEN,
        "the whole part is there"
    );
    assert_eq!(
        &written[..REGION as usize],
        &region[..],
        "the region the flasher wrote is not the image's bytes"
    );
    // Nothing outside the written region moved. A flasher that erased more
    // than it wrote shows up here rather than in the comparison above.
    assert!(
        written[REGION as usize..].iter().all(|b| *b == 0xff),
        "the flasher disturbed flash past the region it was given"
    );

    let census = m.flash_census();
    assert!(
        census.programs > 0 && census.sector_erases + census.block_erases > 0,
        "a flash write that neither erased nor programmed: {census}"
    );

    let _ = std::fs::remove_dir_all(chip.parent().unwrap());
}

/// G2-2's stub half. The stub is real RV32 code the flasher uploads into RAM
/// and jumps to (`MEM_BEGIN`/`MEM_DATA`/`MEM_END`), and it drives SPI1
/// through the ROM's own routines — paths the application never takes.
///
/// The claim here is narrow on purpose: the stub UPLOADS AND RUNS, and the
/// bytes it writes are the image's. What it touched on the way is graded in
/// `src/periph/spi1.rs`'s table, and what it read that the C6 does not have
/// is `docs/defects/2026-09-09-the-esptool-stub-reads-a-chip-struct-the-c6-does-not-have.md`.
#[test]
#[ignore = "needs the merged reference image; run through `just test-emu-c6`"]
fn the_flasher_stub_uploads_runs_and_writes_the_same_bytes() {
    let image = match merged_image(&ReferenceImage::SHIPPED_USB_SILICON) {
        Ok(path) => path,
        Err(reason) => return skip_notice("flash_over_socket", &reason),
    };
    let bytes = std::fs::read(&image).expect("the merged image");
    let region = bytes[..REGION as usize].to_vec();

    let chip = scratch("stub");
    let _ = std::fs::remove_file(&chip);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut m = download_console(&chip, &addr);

    let payload = region.clone();
    let (report, early) = with_flasher(&mut m, &addr, 600_000_000, true, move |flasher| {
        let mut r = Report::default();
        let Ok(mut flasher) = flasher.map_err(|e| r.error = Some(e)) else {
            return r;
        };
        // `Flasher::connect` with `use_stub` uploads the stub and jumps to
        // it; reaching here at all means it answered on the other side.
        r.chip = flasher.chip().to_string();
        if let Err(e) = flasher.write_bin_to_flash(0x0, &payload, None) {
            r.error = Some(format!("write_bin_to_flash: {e}"));
        }
        r
    });

    check_flasher(&report.error, &m, &early);
    assert_eq!(report.chip, Chip::Esp32c6.to_string());
    assert!(early.is_none(), "the machine stopped early: {early:?}");

    m.flush_flash().expect("the chip is written back");
    let written = std::fs::read(&chip).expect("the chip file");
    assert_eq!(
        &written[..REGION as usize],
        &region[..],
        "the stub wrote something other than the image's bytes"
    );

    // The stub really did run RV32 code of its own: espflash placed it in HP
    // SRAM and the machine executed from there. Nothing was hooked and
    // nothing was placed by the loader — `app_segments` is what the LOADER
    // put in memory, and a ROM-up run's is empty.
    assert!(m.app_segments().is_empty(), "the loader placed something");
    assert!(m.hooks().is_empty(), "the ROM hook table is not empty");
    assert_eq!(m.hook_calls(), 0);

    let _ = std::fs::remove_dir_all(chip.parent().unwrap());
}
