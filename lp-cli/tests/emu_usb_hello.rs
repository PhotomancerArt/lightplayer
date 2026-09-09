//! **G3-5** — a product crate speaks to the emulator over its USB link.
//!
//! This test lives in `lp-cli` and not in the emulator for one reason: the
//! `M!` frame. Since PR #538 every writer on the device link frames through
//! `lpc_wire::json::to_serial_line`, and `lpc-wire` is a product crate that
//! nothing under `lp-emu/` may depend on (`just lint-emu-fence`, plan PD2).
//! So the emulator's scripts carry **bytes**, and the frame that goes into
//! one is built here, by the same function the real client uses. If the
//! framing ever changes, this test's script changes with it and the device
//! keeps answering — which is the whole point of having one framer.
//!
//! What it proves end to end: the shipped image, on the emulated
//! USB-Serial-JTAG link with a host attached and draining, receives a
//! `ClientRequest::Hello` on the OUT endpoint, routes it, and answers on the
//! IN endpoint with a `ServerHello` correlated to the request's id.
//!
//! `#[ignore]`d and run by `just test-emu-c6`: it needs a built
//! `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`) and builds the emulator in
//! release.

use std::process::Command;

use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpc_wire::json::to_serial_line;
use lpc_wire::message::client::{ClientMessage, ClientRequest};

/// The request id. `0` is what the device stamps on its own unsolicited
/// messages (the boot hello, every heartbeat), so a reply to *this* is the
/// only frame in the capture that carries `"id":1` — which is what makes
/// the sentinel unambiguous.
const REQUEST_ID: u64 = 1;

/// When the host sends it. Late enough that the server is serving (the
/// unsolicited hello goes out at ~143 ms on this image) and early enough to
/// leave room inside the emulated timeout.
const SEND_AT_MS: u64 = 1_500;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and a release emulator; `just test-emu-c6` runs it"]
fn the_shipped_image_answers_a_hello_sent_over_the_emulated_usb_link() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED_NO_FLASH) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("emu_usb_hello: skipped — {reason}");
            return;
        }
    };
    let dir = tempfile::tempdir().expect("a temp dir");
    let script_path = dir.path().join("hello.usb-script");
    let capture = dir.path().join("delivered.log");

    // The one line the host sends, framed by the single framer, then written
    // as the hex bytes an emulator script carries. Hex rather than a quoted
    // string on purpose: the JSON is full of double quotes, and a test that
    // re-escapes them would be testing its own escaping.
    let line = to_serial_line(&ClientMessage {
        id: REQUEST_ID,
        msg: ClientRequest::Hello,
    })
    .expect("framing a Hello");
    assert!(line.starts_with("M!") && line.ends_with('\n'), "{line:?}");
    let hex: Vec<String> = line.bytes().map(|b| format!("{b:02x}")).collect();
    let script = format!(
        "# built by lpc_wire::json::to_serial_line — the emulator parses bytes\n\
         # {}\n\
         {SEND_AT_MS}  {}\n",
        line.trim_end(),
        hex.join(" ")
    );
    std::fs::write(&script_path, &script).expect("writing the script");

    // The device's answer to *this* request, as it will appear on the link.
    let needle = format!("\"id\":{REQUEST_ID},\"msg\":{{\"hello\"");

    let output = Command::new("cargo")
        .args(["run", "-q", "-p", "lp-emu-esp32c6", "--release", "--"])
        .args(["--elf", elf.to_str().expect("a utf-8 path")])
        // A host with the port open from boot: the link the shipped image
        // actually has, with someone reading it.
        .args(["--usb-host", "attached"])
        .args(["--usb-script", script_path.to_str().expect("a utf-8 path")])
        .args(["--usb-sj", &format!("file:{}", capture.display())])
        .args(["--exit-on", &needle])
        // Emulated time; the wall net only ends a run.
        .args(["--timeout", "4s", "--wall-timeout", "180"])
        .arg("--strict-bus")
        .output()
        .expect("running lp-emu-esp32c6");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the emulator exited {:?}\n{stderr}",
        output.status.code()
    );
    assert!(
        stderr.contains("--exit-on matched"),
        "the reply never arrived — the run hit its timeout instead\n{stderr}"
    );

    let delivered = std::fs::read_to_string(&capture).expect("reading the capture");
    let unsolicited = delivered
        .find("\"id\":0,\"msg\":{\"hello\":{\"proto\":")
        .expect("the device's own boot hello is missing from the capture");
    let answer = delivered
        .find(&needle)
        .expect("the answer to the request is missing from the capture");
    assert!(
        unsolicited < answer,
        "the reply came before the boot hello, which would mean the capture is not the link"
    );
    // It is a real ServerHello, not an echo of what was sent.
    let tail = &delivered[answer..];
    assert!(
        tail.contains("\"boardId\":\"seeed/xiao-esp32-c6\""),
        "the reply is not a ServerHello: {:?}",
        &tail[..tail.len().min(200)]
    );
    eprintln!(
        "G3-5: sent {} at {SEND_AT_MS} ms, {} B delivered, the reply at byte {answer}",
        line.trim_end(),
        delivered.len()
    );
}
