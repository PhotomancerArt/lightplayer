//! JSON Pack on the emulated C6's USB link (plan `lp-json-pack`, P4).
//!
//! The shipped image, with a host attached and draining, is asked to pack
//! its replies with `ClientRequest::SetEncoding` and then talked to. What
//! the capture must show, in order:
//!
//! 1. the boot hello as JSON, carrying `packFormat`;
//! 2. an opt-in naming a **different** pack format answered `json`, and the
//!    link staying JSON;
//! 3. an opt-in naming this build's format answered `packed` — the answer
//!    itself still a JSON line — and every reply after it a learned packed
//!    frame that decodes, through one `lpc-wire` reader holding the link's
//!    table, to a correlated message;
//! 4. the firmware's own console lines between those frames, whole;
//! 5. after the cable is pulled and plugged back in, JSON again: the host
//!    that asked is gone, so the next host gets today's `M!{json}` lines.
//!
//! It lives in `lp-cli` for the reason `emu_usb_hello.rs` does: the
//! requests are framed and the frames decoded by `lpc-wire`, which nothing
//! under `lp-emu/` may depend on.
//!
//! A second test (P5) takes the same image through the `emu serve` door the
//! way a host does — a client that reads through `lpc_wire::WireStream` and
//! opts in with `lpc_wire::PackOptIn` — and checks that `lp-cli wire unpack`
//! turns what it received, and the door's wire tap, back into JSON.
//!
//! `#[ignore]`d and run by `just test-emu-c6`: it needs a built
//! `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`) and builds the emulator in
//! release.

mod support;

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpc_wire::json::to_serial_line;
use lpc_wire::message::client::{ClientMessage, ClientRequest};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{
    PACK_FORMAT_VERSION, PackOptIn, WireChunk, WireEncoding, WireServerMessage, WireStream,
};
use support::Serve;
use tungstenite::Message;

/// The conversation, by emulated millisecond. The boot hello goes out at
/// ~143 ms and the server is serving well before the first line.
const WRONG_FORMAT_AT_MS: u64 = 1_500;
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

    let set_encoding = |format| ClientRequest::SetEncoding {
        encoding: WireEncoding::Packed,
        format,
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
        send(WRONG_FORMAT_AT_MS, 1, set_encoding(PACK_FORMAT_VERSION + 1)),
        send(OPT_IN_AT_MS, 2, set_encoding(PACK_FORMAT_VERSION)),
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

    // 1. The boot hello: JSON, naming this build's pack format.
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
            assert_eq!(hello.pack_format, PACK_FORMAT_VERSION);
        }
        other => panic!("the first message is not the boot hello: {other:?}"),
    }

    // 2. The wrong format: answered json, and the link stays JSON.
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

/// P5: an opted-in client gets packed frames through the `emu serve` door,
/// and `lp-cli wire unpack` restores the JSON — of the bytes the client
/// read, and of the door's own wire tap.
///
/// The client is a host reader in miniature: bytes through
/// [`WireStream`], every message through [`PackOptIn`], whose asks it
/// writes. The first hello is re-sent on a wall cadence until answered (a
/// board that has just been opened may still be latched "not draining";
/// see `Serve::hello`), and that cadence is a retry, never an assertion.
#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn an_opted_in_client_through_the_door_gets_packed_frames_and_unpack_restores_json() {
    const ASK_AGAIN: Duration = Duration::from_secs(2);
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("emu_usb_json_pack: skipped — {reason}");
            return;
        }
    };
    let tap_dir = tempfile::tempdir().expect("a temp dir");
    let serve = Serve::start_specs_with_env(
        &[format!("c6-a={}", elf.display())],
        &[],
        support::scratch(),
        &[("LP_EMU_WIRE_TAP", tap_dir.path())],
    );
    let mut socket = serve.bytes("c6-a");
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a read timeout");

    let started = Instant::now();
    let mut raw = Vec::new();
    let mut wire = WireStream::new();
    let mut opt_in = PackOptIn::new(true);
    // (id, packed, JSON as it arrived) for every message the board sent.
    let mut received: Vec<(u64, bool, String)> = Vec::new();
    let mut asked_hello_at: Option<Instant> = None;
    let mut sent_reads = false;
    let deadline = Instant::now() + support::NET;

    let done = |received: &[(u64, bool, String)]| received.iter().any(|(id, ..)| *id == 4);
    while !done(&received) {
        assert!(
            Instant::now() < deadline,
            "no reply to request 4 within the wall net; received:\n{}",
            received_summary(&received)
        );
        let hello_answered = received.iter().any(|(id, ..)| *id == 2);
        if !hello_answered && asked_hello_at.is_none_or(|at| at.elapsed() >= ASK_AGAIN) {
            send(&mut socket, 2, ClientRequest::Hello);
            asked_hello_at = Some(Instant::now());
        }
        if !sent_reads && opt_in.encoding() == WireEncoding::Packed {
            send(&mut socket, 3, ClientRequest::Hello);
            send(&mut socket, 4, ClientRequest::ListLoadedProjects);
            sent_reads = true;
        }

        let bytes = match socket.read() {
            Ok(Message::Binary(bytes)) => bytes,
            Ok(Message::Close(_)) => panic!("the byte endpoint closed"),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => panic!("reading the byte endpoint: {e}"),
        };
        raw.extend_from_slice(&bytes);
        for chunk in wire.push_collect(&bytes) {
            let frame = match chunk {
                WireChunk::Frame(frame) => frame,
                WireChunk::Error(error) => panic!("a packed frame did not decode: {error}"),
                WireChunk::Desync(dropped) => panic!("a clean link lost step: {dropped:?}"),
                WireChunk::Line(_) => continue,
            };
            let message: WireServerMessage =
                lpc_wire::json::from_str(&frame.json).expect("a message parses");
            let now_ms = started.elapsed().as_millis() as u64;
            let step = opt_in.observe(&message, frame.is_packed(), now_ms);
            if let Some(ask) = step.send {
                send(&mut socket, ask.id, ask.msg);
            }
            received.push((message.id, frame.is_packed(), frame.json));
        }
    }
    drop(socket);
    serve.shutdown();

    // 1. The replies after the opt-in came packed, and decode to exactly
    //    the JSON the typed message serializes to.
    for id in [3, 4] {
        let (_, packed, json) = received
            .iter()
            .find(|(i, ..)| *i == id)
            .unwrap_or_else(|| panic!("no reply {id}"));
        assert!(
            *packed,
            "reply {id} came as JSON:\n{}",
            received_summary(&received)
        );
        let message: WireServerMessage = lpc_wire::json::from_str(json).unwrap();
        assert_eq!(&lpc_wire::json::to_string(&message).unwrap(), json);
    }
    let packed_seen = received.iter().filter(|(_, p, _)| *p).count();

    // 2. `wire unpack --sizes` over the client's bytes: one `M!` line per
    //    frame, every other byte as it came.
    let (stdout, stderr) = lp_cli_wire_unpack(&["--sizes"], &raw);
    assert!(!stdout.contains(&0), "a frame byte survived the unpack");
    let text = String::from_utf8(stdout).expect("unpacked text is UTF-8");
    for (_, _, json) in &received {
        assert!(text.contains(&format!("M!{json}\n")), "missing M!{json}");
    }
    let stderr = String::from_utf8(stderr).unwrap();
    assert_eq!(
        stderr.lines().filter(|l| l.starts_with("frame ")).count(),
        packed_seen,
        "{stderr}"
    );
    assert!(
        stderr.contains(&format!("total frames {packed_seen} packed ")),
        "{stderr}"
    );
    assert!(
        stderr.trim_end().ends_with("unreadable 0 errors 0"),
        "{stderr}"
    );

    // 3. The door's tap recorded the frames raw and annotated each; after
    //    `wire unpack --tap` it reads like a tap of a link that never packed.
    let tap = std::fs::read(tap_dir.path().join("c6-a.tap")).expect("the tap was written");
    let annotations = tap_headers(&tap)
        .iter()
        .filter(|h| h.split(' ').nth(1) == Some("P"))
        .count();
    assert_eq!(annotations, packed_seen, "one P record per packed frame");
    let (unpacked_tap, stderr) = lp_cli_wire_unpack(&["--tap"], &tap);
    assert!(stderr.is_empty(), "{}", String::from_utf8_lossy(&stderr));
    assert!(
        tap_headers(&unpacked_tap)
            .iter()
            .all(|h| matches!(h.split(' ').nth(1), Some("<" | ">"))),
        "the annotations are dropped"
    );
    assert!(
        !unpacked_tap.contains(&0),
        "a frame byte survived in the tap"
    );
    let (_, _, list_json) = received.iter().find(|(i, ..)| *i == 4).unwrap();
    let unpacked_tap = String::from_utf8_lossy(&unpacked_tap);
    assert!(unpacked_tap.contains(&format!("M!{list_json}\n")));

    eprintln!(
        "json-pack door: {} B read, {} messages, {packed_seen} packed",
        raw.len(),
        received.len()
    );
}

/// A packed frame torn in flight (the door's `LP_EMU_WIRE_TEAR` fault, the
/// loss the real C6 link showed): the client's reader notices at the next
/// frame's header, drops what it cannot read, asks for a reset through
/// [`PackOptIn::desynced`], and decodes every packed reply after the board's
/// reset byte for byte (plan `lp2025/2026-09-25-0006-learned-wire-dictionary`).
#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6` runs it"]
fn a_torn_packed_frame_desyncs_the_reader_until_the_board_resets() {
    const ASK_AGAIN: Duration = Duration::from_secs(1);
    let elf = match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => path,
        Err(reason) => {
            eprintln!("emu_usb_json_pack: skipped — {reason}");
            return;
        }
    };
    // The link's first packed frame loses 5 bytes: it taught the board every
    // name it carried, so every frame after it is out of step on the host.
    let serve = Serve::start_specs_with_env(
        &[format!("c6-a={}", elf.display())],
        &[],
        support::scratch(),
        &[("LP_EMU_WIRE_TEAR", std::path::Path::new("1"))],
    );
    let mut socket = serve.bytes("c6-a");
    socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a read timeout");

    let started = Instant::now();
    let mut wire = WireStream::new();
    let mut opt_in = PackOptIn::new(true);
    let mut next_id = 10;
    let mut asked_at: Option<Instant> = None;
    let mut opt_ins = 0;
    let mut torn = 0;
    let mut desynced = 0;
    // Packed replies decoded after the first desync.
    let mut recovered: Vec<String> = Vec::new();
    let deadline = Instant::now() + support::NET;

    while recovered.len() < 3 {
        assert!(
            Instant::now() < deadline,
            "no recovery within the wall net: {torn} torn, {desynced} desynced, \
             {opt_ins} opt-ins, {} recovered",
            recovered.len()
        );
        // A steady trickle of requests: the board's replies are what the
        // reader learns from (and loses step on). Not `Hello`: a Hello reply
        // starts a new table epoch on its own (PackedLink::prepare_reply),
        // which would recover the reader without the reset request this
        // test is about.
        if asked_at.is_none_or(|at| at.elapsed() >= ASK_AGAIN) {
            send(&mut socket, next_id, ClientRequest::ListLoadedProjects);
            next_id += 1;
            asked_at = Some(Instant::now());
        }
        let bytes = match socket.read() {
            Ok(Message::Binary(bytes)) => bytes,
            Ok(Message::Close(_)) => panic!("the byte endpoint closed"),
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => panic!("reading the byte endpoint: {e}"),
        };
        let now_ms = started.elapsed().as_millis() as u64;
        for chunk in wire.push_collect(&bytes) {
            let ask = match chunk {
                WireChunk::Line(_) => None,
                WireChunk::Error(error) => {
                    eprintln!("json-pack tear: dropped: {error}");
                    torn += 1;
                    None
                }
                WireChunk::Desync(dropped) => {
                    eprintln!("json-pack tear: out of step: {}", dropped.reason);
                    desynced += 1;
                    opt_in.desynced(now_ms)
                }
                WireChunk::Frame(frame) => {
                    let message: WireServerMessage =
                        lpc_wire::json::from_str(&frame.json).expect("a decoded frame parses");
                    // Byte-exact where the host serializer prints what the
                    // board's does (no floats; a heartbeat's differ).
                    if matches!(message.msg, ServerMsgBody::Hello(_)) {
                        assert_eq!(
                            lpc_wire::json::to_string(&message).unwrap(),
                            frame.json,
                            "a decoded frame is byte-exact"
                        );
                    }
                    let packed = frame.is_packed();
                    if packed && desynced > 0 {
                        recovered.push(frame.json);
                    }
                    opt_in.observe(&message, packed, now_ms).send
                }
            };
            if let Some(ask) = ask {
                opt_ins += 1;
                send(&mut socket, ask.id, ask.msg);
            }
        }
    }
    drop(socket);
    serve.shutdown();

    assert!(torn + desynced > 0, "the tear was noticed");
    assert!(desynced > 0, "the frames after the tear were out of step");
    assert!(
        opt_ins >= 2,
        "the opt-in, then the reset request: {opt_ins}"
    );
    eprintln!(
        "json-pack tear: {torn} torn, {desynced} dropped out of step, {opt_ins} opt-ins, \
         then {} packed replies decoded",
        recovered.len()
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

/// Split the delivered bytes into lines and frames the way a host reader
/// does: one [`WireStream`] for the whole capture, which learns the link's
/// table as the frames go by.
fn split_link(bytes: &[u8]) -> Vec<Item> {
    let mut items = Vec::new();
    WireStream::new().push(bytes, |chunk| match chunk {
        WireChunk::Line(line) if line.is_empty() => {}
        WireChunk::Line(line) => items.push(Item::Console(line)),
        WireChunk::Frame(frame) if frame.is_packed() => items.push(Item::Packed(frame.json)),
        WireChunk::Frame(frame) => items.push(Item::Json(frame.json)),
        WireChunk::Error(error) => panic!("a frame was dropped: {error}"),
        WireChunk::Desync(dropped) => panic!("a clean capture lost step: {dropped:?}"),
    });
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

/// Write one request as its `M!` line.
fn send(socket: &mut tungstenite::WebSocket<std::net::TcpStream>, id: u64, msg: ClientRequest) {
    let line = to_serial_line(&ClientMessage { id, msg }).expect("framing a request");
    socket
        .send(Message::Binary(line.into_bytes()))
        .expect("writing a request");
}

/// `lp-cli wire unpack <args>` over `input`: (stdout, stderr).
fn lp_cli_wire_unpack(args: &[&str], input: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args(["wire", "unpack"])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawning lp-cli wire unpack");
    let mut stdin = child.stdin.take().expect("piped");
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().expect("lp-cli wire unpack ran");
    writer.join().unwrap().expect("writing its stdin");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (output.stdout, output.stderr)
}

/// Every record header of a wire tap, in order.
fn tap_headers(tap: &[u8]) -> Vec<String> {
    let mut headers = Vec::new();
    let mut at = 0;
    while at < tap.len() {
        let nl = at
            + tap[at..]
                .iter()
                .position(|&b| b == b'\n')
                .expect("a header");
        let header = String::from_utf8(tap[at..nl].to_vec()).expect("a text header");
        let len: usize = header.split(' ').nth(2).expect("a length").parse().unwrap();
        headers.push(header);
        at = nl + 1 + len + 1;
    }
    headers
}

fn received_summary(received: &[(u64, bool, String)]) -> String {
    received
        .iter()
        .map(|(id, packed, json)| {
            let form = if *packed { "packed" } else { "json  " };
            format!("{form} id {id} {}", &json[..json.len().min(100)])
        })
        .collect::<Vec<_>>()
        .join("\n")
}
