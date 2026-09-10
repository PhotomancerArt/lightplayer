//! Plan two M5 — the half of "Studio flashes an emulated board" that needs no
//! browser: a `kind=rom-up` board with nothing on its chip, and the mask ROM's
//! download console answering a real flasher's bytes **over the WebSocket
//! door**.
//!
//! The milestone's own gate is a browser walk and cannot be a CI job (PD9).
//! What CI can have is everything under it, and this is that: the third board
//! kind boots from the reset vector out of a writable chip, the `flash` word
//! is about the chip rather than about the file, and the SLIP `SYNC` esptool-js
//! sends after its DTR/RTS dance is answered through the door by the vendored
//! mask ROM running on the modelled hart.
//!
//! **Not `#[ignore]`d**, unlike its two siblings: a board with nothing on it
//! needs no firmware image at all, so a bare `cargo test -p lp-cli` runs the
//! whole file. That is the point of writing it against a blank board.
//!
//! Nothing here asserts a cycle or a duration — a socket is not deterministic
//! (`lp-emu/esp/README.md` §Determinism). The wall nets in `support` are a
//! safety net, never an input.

mod support;

use support::{Serve, read_until, scratch};
use tungstenite::Message;

/// A whole 4 MiB chip whose first byte is the ESP image magic — enough to
/// make the mask ROM's question ("is there an image at the reset vector?")
/// answer yes, which is the only question `GET /boards`'s `flash` word asks.
/// Not a bootable image, and this file never boots it.
fn chip_with_an_image_magic(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("seed.bin");
    let mut bytes = vec![0xffu8; 4 * 1024 * 1024];
    bytes[0] = 0xe9;
    std::fs::create_dir_all(dir).expect("the scratch dir");
    std::fs::write(&path, bytes).expect("the seed chip");
    path
}

/// esptool's `SYNC`, SLIP-framed exactly as esptool-js puts it on the wire:
/// direction 0x00, command 0x08, 36 bytes of payload, and the payload's
/// `07 07 12 20` followed by 32 × `0x55`.
fn sync_frame() -> Vec<u8> {
    let mut payload = vec![0x00u8, 0x08, 36, 0, 0, 0, 0, 0, 0x07, 0x07, 0x12, 0x20];
    payload.extend(std::iter::repeat_n(0x55u8, 32));
    let mut out = vec![0xc0];
    for byte in payload {
        match byte {
            0xdb => out.extend_from_slice(&[0xdb, 0xdd]),
            0xc0 => out.extend_from_slice(&[0xdb, 0xdc]),
            other => out.push(other),
        }
    }
    out.push(0xc0);
    out
}

/// The `SYNC` reply the ROM sends: `c0 01 08 04 00 <4 bytes> c0`. Matched by
/// its head, because the ROM sends eight of them back to back and a test that
/// pinned the whole burst would be pinning the ROM's retry count.
const SYNC_REPLY_HEAD: &[u8] = &[0xc0, 0x01, 0x08, 0x04, 0x00];

/// **Gate 1, headlessly** — the third board kind exists, boots ROM-up, and
/// says so.
///
/// A `kind=rom-up` board with `blank` for an image has nothing loaded and
/// nothing seeded: the hart starts at the reset vector and the real mask ROM
/// looks in an erased part. What it prints is the part's own answer, and it is
/// **not** "waiting for download" — the C6's ROM loops on `invalid header`
/// forever. The host's reset dance is what reaches the console, which is what
/// every flasher does first anyway (see the SYNC test below).
#[test]
fn a_rom_up_board_boots_the_mask_rom_out_of_its_own_chip() {
    let dir = scratch();
    let serve = Serve::start_specs(&["c6-a=blank,kind=rom-up".to_string()], &[], dir);

    let board = serve.board("c6-a");
    assert_eq!(board["boot"], "rom-up", "the third kind names its entry");
    assert_eq!(
        board["flash"], "blank",
        "an erased chip is blank: {board:#}"
    );

    let mut bytes = serve.bytes("c6-a");
    let text = read_until(&mut bytes, |text| text.contains("invalid header"));
    assert!(
        text.contains("ESP-ROM:esp32c6"),
        "no mask ROM banner — this did not boot from the reset vector:\n{text}"
    );
    assert!(
        text.contains("SPI_FAST_FLASH_BOOT"),
        "the ROM did not take the flash boot path:\n{text}"
    );
    assert!(
        text.contains("invalid header: 0xffffffff"),
        "the ROM found something at the reset vector of an erased chip:\n{text}"
    );
}

/// **The `flash` word is about the chip, not the file.**
///
/// `blank` and `loaded` are the question the mask ROM asks — is there an image
/// magic at the reset vector — and nothing else. Before M5 the word was the
/// flash FILE's length, computed once at power-on, which got it wrong in both
/// directions: a board flashed through esptool-js still said `blank` until the
/// server restarted, and a board whose file was 4 MiB of `0xff` said `loaded`.
#[test]
fn the_flash_word_asks_the_question_the_mask_rom_asks() {
    let dir = scratch();
    let seed = chip_with_an_image_magic(&dir);
    let serve = Serve::start_specs(
        &[
            "c6-blank=blank,kind=rom-up".to_string(),
            format!("c6-seeded={},kind=rom-up", seed.display()),
        ],
        &[],
        dir,
    );

    assert_eq!(
        serve.board("c6-blank")["flash"],
        "blank",
        "an erased chip is not loaded"
    );
    assert_eq!(
        serve.board("c6-seeded")["flash"],
        "loaded",
        "a chip seeded from a whole-chip image is loaded from its first breath"
    );
    // Two boards, two identities, whatever is on them (PD4).
    assert_ne!(
        serve.board("c6-blank")["mac"],
        serve.board("c6-seeded")["mac"]
    );
}

/// **The milestone's own claim, minus the browser**: the mask ROM's download
/// console answers a real flasher's first bytes, over the WebSocket door,
/// after the host's own reset dance.
///
/// The dance is esptool-js 0.6.0's `UsbJtagSerialReset`, spelled as
/// `VirtualSerialPort.setSignals` forwards it — one line named per command,
/// which is what `Transport.setDTR`/`setRTS` produce. Nothing here
/// pattern-matches it: `USB_DEVICE` decodes the RTS falling edge and whether
/// DTR was ever high, and this test only presses the same keys a browser does.
///
/// Then the SLIP `SYNC` esptool-js sends, byte for byte, and the ROM's reply.
/// Two of them, because the console is reached mid-boot and the first can be
/// consumed (`m4-rom-download-console.md`'s fact 1 is UART0's, but a second
/// SYNC costs nothing and is what every flasher sends anyway).
#[test]
fn the_download_console_answers_a_sync_through_the_door() {
    let dir = scratch();
    let serve = Serve::start_specs(&["c6-a=blank,kind=rom-up".to_string()], &[], dir);

    // The byte client first: connecting IS the application opening the port,
    // and the console has to be draining before the dance reaches it.
    let mut bytes = serve.bytes("c6-a");
    let mut control = serve.control("c6-a");

    // esptool-js's `UsbJtagSerialReset`, in its own order.
    for line in [
        "rts 0", "dtr 0", "dtr 1", "rts 0", "rts 1", "dtr 0", "rts 1", "dtr 0", "rts 0",
    ] {
        let reply = control.cmd(line);
        assert!(
            reply.starts_with("ok "),
            "the control channel refused `{line}`: {reply}"
        );
    }

    // The reset is decoded from the falling RTS, and the board reboots into
    // the download strap. It says so on the way.
    let banner = read_until(&mut bytes, |text| text.contains("waiting for download"));
    assert!(
        banner.contains("DOWNLOAD(USB/UART0/SDIO_REI_FEO)"),
        "the dance did not land the chip in its download strap:\n{banner}"
    );
    // An outcome, waited for, never a duration: the counter is published by
    // the board thread at its next slice boundary and the door has no
    // schedule.
    let deadline = std::time::Instant::now() + support::NET;
    while serve.reboots("c6-a") == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(
        serve.reboots("c6-a"),
        1,
        "the dance is one reset, not several"
    );

    let mut seen: Vec<u8> = Vec::new();
    let mut answered = false;
    for _ in 0..2 {
        bytes
            .send(Message::Binary(sync_frame()))
            .expect("the byte endpoint took the SYNC");
        let deadline = std::time::Instant::now() + support::NET / 4;
        while std::time::Instant::now() < deadline {
            match bytes.read() {
                Ok(Message::Binary(more)) => seen.extend_from_slice(&more),
                Ok(Message::Text(more)) => seen.extend_from_slice(more.as_bytes()),
                Ok(_) => continue,
                Err(_) => break,
            }
            if seen.windows(SYNC_REPLY_HEAD.len()).any(|w| w == SYNC_REPLY_HEAD) {
                answered = true;
                break;
            }
        }
        if answered {
            break;
        }
    }
    assert!(
        answered,
        "the mask ROM's download console did not answer a SLIP SYNC over the door; \
         it sent {} byte(s): {:02x?}",
        seen.len(),
        &seen[seen.len().saturating_sub(64)..]
    );
}
