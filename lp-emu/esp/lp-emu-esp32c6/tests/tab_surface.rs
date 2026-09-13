//! The library surface an in-process host drives the machine through, with
//! no socket and no file (C6-in-tab P1, S1–S3).
//!
//! The tab host in `lpa-studio-web` is a Web Worker holding one wasm module.
//! It has no sockets, no filesystem and no threads: it runs a guest slice,
//! gets the machine back, pushes and pulls bytes, applies a control line,
//! and runs the next slice. Four entry points make that possible, and this
//! file is their gate on the native build — everything asserted here is what
//! `scripts/emu/tab-smoke.mjs` then asserts through the `emu_*` exports, so
//! a divergence between the two is a wasm problem rather than a model one.
//!
//! # Why most of this needs no firmware
//!
//! The guest is the **mask ROM's download console**, exactly as
//! `tests/rom_download_console.rs` uses it: boot with the download strap and
//! the ROM prints its banner and then answers SLIP commands over USB, having
//! opened no flash and loaded no app. So a host-write → guest → host-read
//! round trip is provable with the vendored ROM alone, under a bare `cargo
//! test`. Only the `FlashBacking::Bytes` boot gate wants a real image, and
//! it is `#[ignore]`d like every other test that does.

use lp_emu_esp_common::Strap;
use lp_emu_esp32c6::control::ControlReply;
use lp_emu_esp32c6::flash::FlashBacking;
use lp_emu_esp32c6::loader::{EfuseIdentity, ResetCause};
use lp_emu_esp32c6::machine::{
    AppSource, BootMode, Esp32C6Builder, Esp32C6Machine, Outcome, StopCondition, Uart0Sink, UsbHost,
};
use lp_emu_esp32c6::test_support::{ReferenceImage, merged_image, reference_image, skip_notice};

/// The desk board's MAC, as `rom_download_console.rs` uses it.
const DESK_MAC: [u8; 6] = [0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c];

/// esptool's SYNC, SLIP-framed, byte for byte from
/// `scripts/rom-download-sync.usb` — the 36-byte payload the protocol fixes.
const SYNC: &[u8] = &[
    0xc0, 0x00, 0x08, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x07, 0x12, 0x20, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0xc0,
];

/// One SLIP-framed SYNC response, and how many of them `UartConnCheck`
/// sends. Pinned in `rom_download_console.rs` against the same ROM.
const SYNC_REPLY: &[u8] = &[
    0xc0, 0x01, 0x08, 0x04, 0x00, 0x07, 0x07, 0x12, 0x20, 0x00, 0x00, 0x00, 0x00, 0xc0,
];
const SYNC_REPLIES: usize = 8;

/// The download console's banner's last line; the host reads the banner off
/// the link before it sends anything.
const WAITING: &[u8] = b"waiting for download\r\n";

const MS_US: u64 = 1_000;

/// A download-strap machine with the real ROM and a blank chip. The cable
/// and the port are left to the caller, because two of these tests are about
/// who applies them.
fn download_console() -> Esp32C6Builder {
    Esp32C6Builder::new()
        .boot_mode(BootMode::RomUp)
        .reset_cause(ResetCause::UsbUartHpSys)
        .strap(Strap::Download)
        .efuse(EfuseIdentity {
            mac: DESK_MAC,
            ..EfuseIdentity::default()
        })
        .uart0(Uart0Sink::Memory)
}

/// Run one slice the way the worker does: to an absolute cycle budget, with
/// no wall timeout and no exit condition.
fn slice(m: &mut Esp32C6Machine, micros: u64) -> Outcome {
    let stop = StopCondition {
        stop_cycle: Some(m.cycles() + micros * lp_emu_esp32c6::memmap::CYCLES_PER_US),
        ..Default::default()
    };
    m.run_until(&stop)
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A control line answers exactly what the same command answers on the
/// socket, and the script's own verb is refused.
#[test]
fn a_control_line_answers_the_sockets_reply_and_refuses_the_scripts_verb() {
    let mut m = download_console()
        .usb_host(UsbHost::Absent)
        .build()
        .expect("the download-console machine builds");

    // `state` before anything: the shape is the socket's, and the cycle it
    // names is the cycle the machine is stopped at.
    let reply = m.control_line("state");
    assert!(
        matches!(reply, ControlReply::State { cycle, .. } if cycle == m.cycles()),
        "state is answered at the machine's own cycle: {reply}"
    );
    assert_eq!(
        reply.to_string(),
        format!("ok state cyc=0 us=0 host=absent draining=false sof=off in_pending=0 out_queued=0"),
        "the reply text is the control protocol's, not a second dialect"
    );

    // The cable, then the port — the coupling rule, applied by the host
    // rather than implied by a socket client.
    assert_eq!(m.control_line("attach").to_string(), "ok attach cyc=0 us=0");
    assert_eq!(m.control_line("open").to_string(), "ok open cyc=0 us=0");
    assert_eq!(
        m.control_line("open").to_string(),
        "err open: the port is already open",
        "a precondition failure answers err and changes nothing"
    );
    assert!(
        m.control_line("state")
            .to_string()
            .contains("host=attached draining=true")
    );

    // `wait` is the script's verb: a live host waits by waiting.
    assert!(
        m.control_line("wait 5").to_string().starts_with("err "),
        "wait must not be applicable on a live channel: {}",
        m.control_line("wait 5")
    );
    // A line that is not a command at all is a reply, never a panic.
    assert!(
        m.control_line("teleport")
            .to_string()
            .starts_with("err unknown command `teleport` (expected one of: attach,"),
        "an unknown verb is answered with the protocol's own refusal: {}",
        m.control_line("teleport")
    );
    assert_eq!(
        m.control_line("").to_string(),
        "err empty command",
        "an empty line is refused rather than ignored"
    );

    // Nothing above ran the guest, and the applied lines were counted the
    // way the socket's are.
    assert_eq!(m.cycles(), 0);
    assert_eq!(m.control_lines(), 4, "state, attach, open, state");
}

/// The whole in-process link in one run: the host pushes bytes on the queue
/// between slices, the mask ROM answers them, and the host reads the answer
/// back out of the drain — with no socket anywhere.
#[test]
fn host_bytes_pushed_between_slices_reach_the_guest_and_its_reply_comes_back() {
    let mut m = download_console()
        .usb_sj_queue_source()
        .usb_host(UsbHost::Attached { draining: true })
        .build()
        .expect("the download-console machine builds");
    let host = m
        .usb_sj_host_handle()
        .expect("the builder installed a queue source, so the machine has its handle");

    // Slice one: the ROM boots and says it is waiting. The console is
    // drained on the host's own cadence, not at the end of the run.
    slice(&mut m, 5 * MS_US);
    let banner = m.take_usb_sj_output();
    assert!(
        banner.ends_with(WAITING),
        "the download console's banner, on the USB link:\n{}",
        String::from_utf8_lossy(&banner)
    );
    assert!(
        m.take_usb_sj_output().is_empty(),
        "the drain took the bytes with it"
    );

    // The host sends, between slices, at no guest cycle in particular.
    host.push(SYNC);
    assert_eq!(host.pending(), SYNC.len(), "queued, not yet delivered");
    assert_eq!(
        m.take_usb_sj_output(),
        Vec::<u8>::new(),
        "pushing bytes did not run the guest"
    );

    // Slice two: the ROM takes them and answers.
    slice(&mut m, 20 * MS_US);
    assert_eq!(host.pending(), 0, "the guest took every byte");
    let replies = m.take_usb_sj_output();
    assert_eq!(
        hex(&replies),
        hex(&SYNC_REPLY.repeat(SYNC_REPLIES)),
        "eight SYNC replies from the real ROM, over the in-process link"
    );

    // The console is the UART0 drain's, separately, and it carries the
    // banner and nothing the USB link answered.
    let console = m.take_uart0_output();
    assert!(
        console.ends_with(WAITING),
        "UART0 carries the banner:\n{}",
        String::from_utf8_lossy(&console)
    );
    assert!(
        m.take_uart0_output().is_empty(),
        "the console drain empties too"
    );
    assert!(
        !console.windows(2).any(|w| w == [0xc0, 0x01]),
        "the replies went where the SYNC came from, not to the console"
    );
}

/// The `flash` word the door reports and the tab's `loaded` face asks, from
/// one function: a blank chip has nothing at the reset vector, and a chip a
/// host wrote an image into has.
#[test]
fn the_reset_vector_says_blank_until_an_image_is_written_there() {
    let m = download_console()
        .usb_host(UsbHost::Absent)
        .build()
        .expect("the machine builds");
    assert!(
        !m.has_image_at_reset_vector(),
        "a 4 MiB part of 0xff is a chip with nothing on it"
    );

    // A direct write, the way the tab host's Flash verb writes: erase the
    // sector, then place the bytes.
    {
        let mut flash = m.flash().lock().expect("the chip");
        assert!(flash.erase(0, lp_emu_esp32c6::flash::SECTOR_LEN));
        assert!(flash.program(0, &[0xe9, 0x06, 0x02, 0x2f]));
    }
    assert!(m.has_image_at_reset_vector(), "the ROM would boot this");

    // And a chip built from bytes the host handed over answers on its own,
    // with no write at all.
    let bytes = Esp32C6Builder::new()
        .flash(FlashBacking::Bytes(vec![0xe9, 0x06, 0x02, 0x2f]))
        .build()
        .expect("a bytes-backed machine builds");
    assert!(bytes.has_image_at_reset_vector());
}

/// The reference image both halves use — the same pin `tests/rom_up_boot.rs`
/// boots.
const IMAGE: ReferenceImage = ReferenceImage::SHIPPED_USB_SILICON;

/// Long enough for the ROM, the bootloader and the app's `[INIT]` chain, as
/// `rom_up_boot.rs` pins it.
const BOOT_GATE_US: u64 = 400_000;

/// A `Bytes` chip is a `Copy` chip that came through memory: the same merged
/// image, handed over as bytes rather than named as a path, boots to a
/// byte-identical console.
///
/// This is the claim the tab host rests on — its 4 MiB image arrives from an
/// OPFS file through JavaScript and never touches a filesystem this crate
/// can see.
#[test]
#[ignore = "needs a fw-esp32c6 build; `just test-emu-c6`"]
fn a_bytes_backed_chip_boots_to_the_same_console_as_the_file_it_came_from() {
    let merged = match merged_image(&IMAGE) {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("tab_surface", &reason);
            return;
        }
    };
    let elf = reference_image(&IMAGE).expect("the ELF beside the merged image");
    let image = std::fs::read(&merged).expect("the merged image");
    let len = image.len() as u32;

    let boot = |backing: FlashBacking| {
        let mut m = Esp32C6Builder::new()
            .boot_mode(BootMode::RomUp)
            .app(AppSource::Path(elf.clone()))
            .flash(backing)
            .flash_len(len)
            .reset_cause(ResetCause::UsbUartHpSys)
            .strap(Strap::App)
            .uart0(Uart0Sink::Memory)
            .usb_host(UsbHost::Attached { draining: true })
            .strict(true)
            .build()
            .expect("the rom-up machine builds");
        let outcome = m.run_until(&StopCondition::after_micros(BOOT_GATE_US));
        (m.uart0().bytes(), m.cycles(), outcome)
    };

    let (from_file, file_cycles, file_outcome) = boot(FlashBacking::Copy(merged));
    let (from_bytes, bytes_cycles, bytes_outcome) = boot(FlashBacking::Bytes(image));

    assert_eq!(
        String::from_utf8_lossy(&from_bytes),
        String::from_utf8_lossy(&from_file),
        "the console is the image's, not the backing's"
    );
    assert_eq!(
        bytes_cycles, file_cycles,
        "and it got there in the same guest time"
    );
    assert_eq!(format!("{bytes_outcome:?}"), format!("{file_outcome:?}"));
}
