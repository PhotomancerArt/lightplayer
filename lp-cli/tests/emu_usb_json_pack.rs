//! JSON Pack on the emulated C6's USB link (plan `lp-json-pack`, P4).
//!
//! The shipped image, with a host attached and draining, is asked to pack
//! its replies with `ClientRequest::SetEncoding` and then talked to. What
//! the capture must show, in order:
//!
//! 1. the boot hello as JSON, carrying `packDictionary`;
//! 2. an opt-in naming a **different** dictionary answered `json`, and the
//!    link staying JSON;
//! 3. an opt-in naming this build's dictionary answered `packed` — the
//!    answer itself still a JSON line — and every reply after it a packed
//!    frame that decodes, through `lpc-wire`, to a correlated message;
//! 4. the firmware's own console lines between those frames, whole;
//! 5. after the cable is pulled and plugged back in, JSON again: the host
//!    that asked is gone, so the next host gets today's `M!{json}` lines.
//!
//! It lives in `lp-cli` for the reason `emu_usb_hello.rs` does: the
//! requests are framed and the frames decoded by `lpc-wire`, which nothing
//! under `lp-emu/` may depend on.
//!
//! `#[ignore]`d and run by `just test-emu-c6`: it needs a built
//! `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`) and builds the emulator in
//! release.

use std::process::Command;

use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lp_json_pack::{FRAME_KIND_PACK, FrameScanner, ScanEvent, VecFrameBuffer};
use lpc_wire::json::to_serial_line;
use lpc_wire::message::client::{ClientMessage, ClientRequest};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{WIRE_DICTIONARY_FINGERPRINT, WireEncoding, WireServerMessage};

/// The conversation, by emulated millisecond. The boot hello goes out at
/// ~143 ms and the server is serving well before the first line.
const WRONG_DICTIONARY_AT_MS: u64 = 1_500;
const OPT_IN_AT_MS: u64 = 2_000;
const PACKED_HELLO_AT_MS: u64 = 2_500;
const PACKED_LIST_AT_MS: u64 = 3_000;
const DETACH_AT_MS: u64 = 4_000;
const ATTACH_AT_MS: u64 = 4_500;
const OPEN_AT_MS: u64 = 5_000;
const JSON_AGAIN_AT_MS: u64 = 5_500;

#[test]
#[ignore = "needs a built fw-esp32c6 ELF and a release emulator; `just test-emu-c6` runs it"]
fn an_opted_in_link_gets_packed_frames_until_the_cable_is_pulled() {
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("emu_usb_json_pack: skipped — {reason}");
            return;
        }
    };
    let dir = tempfile::tempdir().expect("a temp dir");
    let script_path = dir.path().join("json-pack.usb-script");
    let capture = dir.path().join("delivered.bin");

    let set_encoding = |dictionary| ClientRequest::SetEncoding {
        encoding: WireEncoding::Packed,
        dictionary,
    };
    // Each request framed by the single framer, written as the hex bytes an
    // emulator script carries (see `emu_usb_hello.rs` for why hex).
    let send = |ms: u64, id: u64, msg: ClientRequest| {
        let line = to_serial_line(&ClientMessage { id, msg }).expect("framing a request");
        let hex: Vec<String> = line.bytes().map(|b| format!("{b:02x}")).collect();
        format!("# {}\n{ms}  {}\n", line.trim_end(), hex.join(" "))
    };
    let script = [
        "0  attach\n0  open\n".to_string(),
        send(
            WRONG_DICTIONARY_AT_MS,
            1,
            set_encoding(WIRE_DICTIONARY_FINGERPRINT ^ 1),
        ),
        send(OPT_IN_AT_MS, 2, set_encoding(WIRE_DICTIONARY_FINGERPRINT)),
        send(PACKED_HELLO_AT_MS, 3, ClientRequest::Hello),
        send(PACKED_LIST_AT_MS, 4, ClientRequest::ListLoadedProjects),
        format!("{DETACH_AT_MS}  detach\n{ATTACH_AT_MS}  attach\n{OPEN_AT_MS}  open\n"),
        send(JSON_AGAIN_AT_MS, 5, ClientRequest::Hello),
    ]
    .concat();
    std::fs::write(&script_path, &script).expect("writing the script");

    let last_needle = "\"id\":5,\"msg\":{\"hello\"";
    let output = Command::new("cargo")
        .args(["run", "-q", "-p", "lp-emu-esp32c6", "--release", "--"])
        .args(["--elf", elf.to_str().expect("a utf-8 path")])
        .args(["--usb-host", "attached"])
        .args(["--usb-script", script_path.to_str().expect("a utf-8 path")])
        .args(["--usb-sj", &format!("file:{}", capture.display())])
        .args(["--exit-on", last_needle])
        .args(["--timeout", "8s", "--wall-timeout", "300"])
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
        "the last reply never arrived — the run hit its timeout instead\n{stderr}"
    );

    let delivered = std::fs::read(&capture).expect("reading the capture");
    let items = split_link(&delivered);

    // Every message, in link order, with the form it came in.
    let messages: Vec<(Form, WireServerMessage)> = items
        .iter()
        .filter_map(|item| match item {
            Item::Json(json) => Some((Form::Json, lpc_wire::json::from_str(json).unwrap())),
            Item::Packed(json) => Some((Form::Packed, lpc_wire::json::from_str(json).unwrap())),
            Item::Console(_) => None,
        })
        .collect();
    let find = |id: u64, what: &str| {
        messages
            .iter()
            .position(|(_, m)| m.id == id && body_is(&m.msg, what))
            .unwrap_or_else(|| panic!("no {what} reply with id {id} in the capture"))
    };

    // 1. The boot hello: JSON, naming this build's dictionary.
    let (form, boot) = &messages[0];
    assert_eq!(
        *form,
        Form::Json,
        "the boot hello goes out before anyone asks"
    );
    match &boot.msg {
        ServerMsgBody::Hello(hello) => {
            assert_eq!(boot.id, 0);
            assert_eq!(hello.proto, lpc_wire::WIRE_PROTO_VERSION);
            assert_eq!(hello.pack_dictionary, WIRE_DICTIONARY_FINGERPRINT);
        }
        other => panic!("the first message is not the boot hello: {other:?}"),
    }

    // 2. The wrong dictionary: answered json, and the link stays JSON.
    let wrong = find(1, "setEncoding");
    assert_eq!(messages[wrong].0, Form::Json);
    assert_encoding(&messages[wrong].1, WireEncoding::Json);

    // 3. The right one: the answer is JSON, everything after it is packed.
    let opt_in = find(2, "setEncoding");
    assert_eq!(messages[opt_in].0, Form::Json, "the answer is always JSON");
    assert_encoding(&messages[opt_in].1, WireEncoding::Packed);
    assert!(
        messages[wrong..opt_in]
            .iter()
            .all(|(f, _)| *f == Form::Json),
        "a refused opt-in must not pack anything"
    );
    let hello = find(3, "hello");
    let list = find(4, "listLoadedProjects");
    let back = find(5, "hello");
    assert!(opt_in < hello && hello < list && list < back);
    for (form, msg) in &messages[opt_in + 1..=list] {
        assert_eq!(*form, Form::Packed, "id {} after the opt-in", msg.id);
    }

    // 4. Console lines between the frames are whole: the firmware says it
    //    switched, and says so as a line of its own.
    let switched = items
        .iter()
        .position(|i| matches!(i, Item::Console(l) if l.contains("replies are now packed")))
        .unwrap_or_else(|| {
            panic!(
                "the firmware's own line about the switch:\n{}",
                summary(&items)
            )
        });
    let first_packed = items
        .iter()
        .position(|i| matches!(i, Item::Packed(_)))
        .expect("a packed frame");
    assert!(switched < first_packed);
    for item in &items {
        if let Item::Console(line) = item {
            assert!(
                !line.chars().any(|c| c.is_control() && c != '\t'),
                "a console line carries frame bytes: {line:?}"
            );
        }
    }

    // 5. After the unplug: JSON again, for whoever opens the port next.
    assert_eq!(messages[back].0, Form::Json, "a new link starts as JSON");
    assert!(
        messages[list + 1..].iter().all(|(f, _)| *f == Form::Json),
        "nothing after the unplug is packed"
    );

    let packed = items
        .iter()
        .filter(|i| matches!(i, Item::Packed(_)))
        .count();
    eprintln!(
        "json-pack: {} B delivered, {} messages, {packed} packed, opt-in at message {opt_in}",
        delivered.len(),
        messages.len()
    );
}

/// How a message came over the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Form {
    Json,
    Packed,
}

/// One thing on the link, in order.
#[derive(Debug)]
enum Item {
    /// An `M!` line's JSON.
    Json(String),
    /// A packed frame, decoded to its JSON.
    Packed(String),
    /// Any other text line.
    Console(String),
}

/// Split the delivered bytes into lines and frames, the way a host reader
/// does: frames out of the byte stream first, then the text between them
/// into lines.
fn split_link(bytes: &[u8]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut text = Vec::new();
    let flush_lines = |text: &mut Vec<u8>, items: &mut Vec<Item>, all: bool| {
        while let Some(nl) = text.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = text.drain(..=nl).collect();
            let line = String::from_utf8(line).expect("console text is UTF-8");
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(json) = line.strip_prefix("M!") {
                items.push(Item::Json(json.to_string()));
            } else if !line.is_empty() {
                items.push(Item::Console(line.to_string()));
            }
        }
        if all && !text.is_empty() {
            let rest = String::from_utf8(std::mem::take(text)).expect("UTF-8");
            items.push(Item::Console(rest));
        }
    };
    let mut scanner = FrameScanner::new(VecFrameBuffer::new(64 * 1024));
    scanner.push(bytes, |event| match event {
        ScanEvent::Text(t) => {
            text.extend_from_slice(t);
            flush_lines(&mut text, &mut items, false);
        }
        ScanEvent::Frame { kind, payload } => {
            assert_eq!(kind, FRAME_KIND_PACK);
            // A frame follows its own `\n`, so no text is pending.
            flush_lines(&mut text, &mut items, true);
            let json = lpc_wire::decode_packed_to_json(payload).expect("a packed frame decodes");
            items.push(Item::Packed(json));
        }
        ScanEvent::Dropped(reason) => panic!("a frame was dropped: {reason:?}"),
    });
    flush_lines(&mut text, &mut items, true);
    items
}

/// Whether `body` is the variant whose wire name is `name`.
fn body_is(body: &ServerMsgBody, name: &str) -> bool {
    let json = lpc_wire::json::to_string(body).expect("a body serializes");
    json.starts_with(&format!("{{\"{name}\"")) || json == format!("\"{name}\"")
}

fn assert_encoding(msg: &WireServerMessage, expected: WireEncoding) {
    match &msg.msg {
        ServerMsgBody::SetEncoding { encoding } => assert_eq!(*encoding, expected),
        other => panic!("expected a setEncoding answer, got {other:?}"),
    }
}

/// The capture, one line per item, for failure messages.
fn summary(items: &[Item]) -> String {
    items
        .iter()
        .map(|item| match item {
            Item::Json(j) => format!("json   {}", &j[..j.len().min(100)]),
            Item::Packed(j) => format!("packed {}", &j[..j.len().min(100)]),
            Item::Console(l) => format!("text   {l}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
