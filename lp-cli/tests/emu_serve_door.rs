//! Plan two M1 — `lp-cli emu serve`'s WebSocket door, end to end.
//!
//! **Nothing here asserts a cycle, a duration, or an ordering between the two
//! sockets.** A socket is not deterministic and cannot be
//! (`lp-emu/esp/README.md` §Determinism): a command lands at whichever slice
//! boundary the host's poll fell on, and the reply names the cycle only so a
//! session is *auditable*. Every assertion below is about an **outcome** —
//! what a board answered, what its identity is, what survived a restart. The
//! wall timeouts are a safety net, never an input.
//!
//! The door is driven the way anything else would drive it: `curl`-shaped
//! HTTP for `GET /boards`, and `tungstenite` for the two WebSocket endpoints.
//! It runs the release-or-debug `lp-cli` binary as a child process, so what
//! is under test is the shipped door and not a library call.
//!
//! `#[ignore]`d for the usual reason: it needs the reference firmware image
//! (`scripts/emu/build-reference-image.sh`, via `just test-emu-c6`), which is
//! a riscv32 build of a pinned commit rather than something a bare
//! `cargo test` should start.

mod support;

use support::{Serve, read_until, reference_elf, state_field};
use tungstenite::Message;

/// **Gate 1** — the door exists and lists its boards.
///
/// Two boards, two ids, **two MACs**: a registry of N boards that all answer
/// with one identity is one board N times, and `s9-two-boards` is about two
/// identities.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn the_door_lists_its_boards_and_they_are_two_boards() {
    let Some(elf) = reference_elf("the_door_lists_its_boards") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a", "c6-b"], &[]);

    let body = serve.get("/boards");
    let json: serde_json::Value = serde_json::from_str(&body).expect("`/boards` is JSON");
    let boards = json["boards"].as_array().expect("a `boards` array");
    assert_eq!(boards.len(), 2, "two --board flags, two boards: {body}");

    assert_eq!(boards[0]["id"], "c6-a");
    assert_eq!(boards[1]["id"], "c6-b");
    assert_eq!(boards[0]["bytes"], "/board/c6-a/bytes");
    assert_eq!(boards[0]["control"], "/board/c6-a/control");
    assert_eq!(boards[1]["bytes"], "/board/c6-b/bytes");
    assert_eq!(boards[1]["control"], "/board/c6-b/control");
    assert_eq!(boards[0]["chip"], "esp32c6");
    assert_eq!(boards[0]["link"], "usb-serial-jtag");
    assert_ne!(
        boards[0]["mac"], boards[1]["mac"],
        "two boards, two identities: {body}"
    );

    // An id that is not a board is a 404, not a hang and not a board.
    assert!(
        serve.get_status("/board/nope/bytes") != 200,
        "an unknown board must not answer 200"
    );
}

/// **Gate 3** — the control channel answers, one line per line, in order,
/// including for `err`.
///
/// The vocabulary is `control.rs`'s and this door neither filters a verb nor
/// adds one, so `wait` — the scripted form's only — must reach the machine's
/// parser and come back as the machine's own `err`.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn the_control_channel_answers_one_line_per_line_in_order() {
    let Some(elf) = reference_elf("the_control_channel_answers") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a"], &["--usb-host", "attached-idle"]);
    let mut control = serve.control("c6-a");

    // Cable in, port closed: `attached-idle` is the state the coupling rule
    // is about.
    let state = control.cmd("state");
    assert!(
        state.starts_with("ok state ") && state.contains("host=attached"),
        "state: {state}"
    );
    assert!(state.contains("draining=false"), "port closed: {state}");
    assert!(
        state.contains("in_pending="),
        "the full state grammar: {state}"
    );
    assert!(
        state.contains("out_queued="),
        "the full state grammar: {state}"
    );

    // A cable that is already in cannot be plugged in again, and the machine
    // says which precondition failed rather than quietly doing nothing.
    let attach = control.cmd("attach");
    assert_eq!(
        attach, "err attach: a host is already attached",
        "attach on an attached host"
    );

    // Out and back in: the cable moves only when asked.
    assert!(control.cmd("detach").starts_with("ok detach "));
    let gone = control.cmd("state");
    assert!(gone.contains("host=absent"), "after detach: {gone}");
    assert!(control.cmd("attach").starts_with("ok attach "));
    let back = control.cmd("state");
    assert!(back.contains("host=attached"), "after attach: {back}");

    // One reply per command, including for `err`, and the reason is the
    // machine's own.
    let nonsense = control.cmd("nonsense");
    assert!(
        nonsense.starts_with("err unknown command `nonsense`"),
        "nonsense: {nonsense}"
    );
    // `wait` is in `ControlCommand::VERBS` but is the scripted form's only.
    // The door passes it through; the machine answers.
    let wait = control.cmd("wait 5");
    assert!(wait.starts_with("err "), "wait on a socket: {wait}");

    // …and the channel is still in step afterwards: an `err` consumed
    // exactly one reply.
    assert!(control.cmd("state").starts_with("ok state "));
    // The pads are the bus's, so `pins` works on this door too.
    assert!(control.cmd("pins").starts_with("ok pins "));
}

/// **The coupling rule** — connecting the byte socket opens the port and
/// disconnecting closes it, and neither ever moves the cable.
///
/// Run against `attached-idle`, which is the state that makes the rule
/// visible: the port starts closed, so `draining` moving is the byte
/// client's own edge and nothing else's.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn a_byte_client_opens_the_port_and_never_moves_the_cable() {
    let Some(elf) = reference_elf("a_byte_client_opens_the_port") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a"], &["--usb-host", "attached-idle"]);
    let mut control = serve.control("c6-a");

    let closed = control.cmd("state");
    assert!(closed.contains("draining=false"), "before: {closed}");
    assert!(closed.contains("host=attached"), "before: {closed}");

    {
        let _bytes = serve.bytes("c6-a");
        let open = serve.wait_for_state(&mut control, "draining=true");
        assert!(open.contains("draining=true"), "with a byte client: {open}");
        // The cable did not move. That is the whole point of the second
        // socket: a client on the byte socket is an application, not a hand.
        assert!(open.contains("host=attached"), "with a byte client: {open}");
    }

    let shut = serve.wait_for_state(&mut control, "draining=false");
    assert!(shut.contains("draining=false"), "after close: {shut}");
    assert!(shut.contains("host=attached"), "after close: {shut}");

    // …and again, because a port that can only be opened once is not a port.
    {
        let _bytes = serve.bytes("c6-a");
        let reopened = serve.wait_for_state(&mut control, "draining=true");
        assert!(
            reopened.contains("draining=true"),
            "second open: {reopened}"
        );
        assert!(
            reopened.contains("host=attached"),
            "second open: {reopened}"
        );
    }
}

/// **Attach and detach are never implied by a socket.**
///
/// The byte socket comes and goes; `host=` only ever moves when the control
/// channel says so.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn only_the_control_channel_moves_the_cable() {
    let Some(elf) = reference_elf("only_the_control_channel_moves_the_cable") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a"], &["--usb-host", "attached-idle"]);
    let mut control = serve.control("c6-a");

    for _ in 0..3 {
        {
            let _bytes = serve.bytes("c6-a");
            let held = serve.wait_for_state(&mut control, "draining=true");
            assert!(held.contains("host=attached"), "{held}");
        }
        let dropped = serve.wait_for_state(&mut control, "draining=false");
        assert!(dropped.contains("host=attached"), "{dropped}");
    }
}

/// **One byte client at a time.** A second WebSocket on `/bytes` is refused,
/// never silently multiplexed — two applications holding one serial port is
/// not a state a board can be in.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn a_second_byte_client_is_refused_rather_than_multiplexed() {
    let Some(elf) = reference_elf("a_second_byte_client_is_refused") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a"], &[]);
    let _first = serve.bytes("c6-a");
    let second = serve.try_bytes("c6-a");
    assert!(
        second.is_err(),
        "a second byte client must be refused, not multiplexed"
    );
}

/// **Bytes stay bytes.** What the client writes reaches the guest unchanged
/// and what the guest writes reaches the client unchanged — no length
/// prefix, no envelope, no framing added in either direction.
///
/// The proof is the board's own hello, which the client reads off the wire
/// and which parses as the exact JSON line
/// [`lpc_wire::json::to_serial_line`] would have produced. **Gate 2's
/// substance**: the identity in it is board A's MAC, and board B's is board
/// B's.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn a_board_says_hello_over_the_byte_endpoint_with_its_own_identity() {
    let Some(elf) = reference_elf("a_board_says_hello") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a", "c6-b"], &[]);
    let listed = serve.get("/boards");
    let json: serde_json::Value = serde_json::from_str(&listed).expect("JSON");
    let mac_a = json["boards"][0]["mac"]
        .as_str()
        .expect("a mac")
        .to_string();
    let mac_b = json["boards"][1]["mac"]
        .as_str()
        .expect("a mac")
        .to_string();

    let hello_a = serve.hello("c6-a");
    let hello_b = serve.hello("c6-b");
    assert!(
        hello_a.contains(&format!("\"baseMac\":\"{mac_a}\"")),
        "board A's hello carries board A's MAC ({mac_a}):\n{hello_a}"
    );
    assert!(
        hello_b.contains(&format!("\"baseMac\":\"{mac_b}\"")),
        "board B's hello carries board B's MAC ({mac_b}):\n{hello_b}"
    );
    assert_ne!(mac_a, mac_b, "two boards, two identities");
    // The line is a whole `M!` line and nothing has been wrapped around it.
    assert!(
        hello_a.starts_with("M!{"),
        "no envelope was added: {hello_a}"
    );
}

/// **Gate 6** — `reset` reboots; it does not end the run (PD11).
///
/// The server is still alive afterwards, the board answers again, and its
/// reboot count has moved. A `setSignals()` dance that kills the server is
/// not a board.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn reset_reboots_the_board_and_the_server_stays_up() {
    let Some(elf) = reference_elf("reset_reboots_the_board") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a", "c6-b"], &[]);
    let mut control = serve.control("c6-a");

    assert_eq!(serve.reboots("c6-a"), 0, "nothing has reset yet");
    let reply = control.cmd("reset");
    assert!(reply.starts_with("ok reset "), "reset: {reply}");

    // The board comes back. Not "within N milliseconds" — a socket has no
    // schedule — just: it comes back.
    serve.wait_for_reboot("c6-a", 1);
    assert_eq!(serve.reboots("c6-a"), 1, "the board rebooted");
    assert_eq!(
        serve.reboots("c6-b"),
        0,
        "a reset on one board is not a reset on the other"
    );

    // The server did not exit, and the board still answers.
    let state = control.cmd("state");
    assert!(state.starts_with("ok state "), "after the reboot: {state}");
    assert_eq!(serve.board("c6-a")["state"], "running");

    // …and it says hello again, which is the half a browser cares about.
    let hello = serve.hello("c6-a");
    assert!(hello.starts_with("M!{"), "a second hello: {hello}");
}

/// **`power-cycle` beside `reset`** — additive, and a different thing.
///
/// The door is a pump: a verb the machine's parser knows reaches it, and this
/// door neither filters one nor adds one. So the whole of P3's control-surface
/// work is visible here as one new verb that answers `ok power-cycle …` and
/// moves a counter `reset` does not move.
///
/// The board is clean, so both restarts are boots that work; what this asserts
/// is that the door can ask for each kind and see which one it got. The
/// LP-domain half — the wedge that survives one and not the other — is
/// `lp-emu/esp/lp-emu-esp32c6/tests/bootloader_hang.rs`, where a deterministic
/// machine can hold it.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn power_cycle_is_a_second_verb_beside_reset_and_the_door_says_which_it_got() {
    let Some(elf) = reference_elf("power_cycle_is_a_second_verb") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a", "c6-b"], &[]);
    let mut control = serve.control("c6-a");

    assert_eq!((serve.reboots("c6-a"), serve.power_cycles("c6-a")), (0, 0));

    // A plain reset first, so the two are compared on one board.
    assert!(control.cmd("reset").starts_with("ok reset "));
    serve.wait_for_reboot("c6-a", 1);
    assert_eq!(
        serve.power_cycles("c6-a"),
        0,
        "a chip_rst is not a power cycle"
    );

    // Then the supply. No `attach`, no port state, no DTR/RTS: the hand that
    // can cut the power is on the plug, not on the port.
    let reply = control.cmd("power-cycle");
    assert!(reply.starts_with("ok power-cycle "), "power-cycle: {reply}");
    serve.wait_for_reboot("c6-a", 2);
    assert_eq!(
        serve.power_cycles("c6-a"),
        1,
        "the door reports which kind of restart it got"
    );
    assert_eq!(
        (serve.reboots("c6-b"), serve.power_cycles("c6-b")),
        (0, 0),
        "a power cycle on one board is not one on the other"
    );

    // The server is still up, the board still answers, and it still says
    // hello — a power cycle is a board coming back, not a board disappearing.
    assert!(control.cmd("state").starts_with("ok state "));
    assert_eq!(serve.board("c6-a")["state"], "running");
    assert!(serve.hello("c6-a").starts_with("M!{"));

    // And the verb takes no arguments, from the machine's own parser through
    // the door unchanged.
    let reply = control.cmd("power-cycle now");
    assert!(reply.starts_with("err "), "power-cycle now: {reply}");
}

/// **Boards are independent.** Two boards, two flash files, two MACs, and
/// nothing one does appears in the other's state.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn two_boards_are_two_boards() {
    let Some(elf) = reference_elf("two_boards_are_two_boards") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a", "c6-b"], &["--usb-host", "attached-idle"]);
    let mut a = serve.control("c6-a");
    let mut b = serve.control("c6-b");

    assert!(a.cmd("detach").starts_with("ok detach "));
    let a_state = a.cmd("state");
    let b_state = b.cmd("state");
    assert!(a_state.contains("host=absent"), "A was detached: {a_state}");
    assert!(
        b_state.contains("host=attached"),
        "B's cable is B's: {b_state}"
    );

    // Two flash files, side by side, both the board's own.
    serve.wait_for_file("c6-a.flash.bin");
    serve.wait_for_file("c6-b.flash.bin");
    let mut names: Vec<String> = std::fs::read_dir(serve.state_dir())
        .expect("the state dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".flash.bin"))
        .collect();
    names.sort();
    assert_eq!(names, vec!["c6-a.flash.bin", "c6-b.flash.bin"]);
}

/// `GET /boards` is a plain GET, and the byte endpoints are not. Getting
/// this wrong is the first thing a hand-written client does, so it gets an
/// answer rather than a hang.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn the_door_says_what_each_route_is() {
    let Some(elf) = reference_elf("the_door_says_what_each_route_is") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a"], &[]);
    assert_eq!(serve.get_status("/boards"), 200);
    assert_eq!(serve.get_status("/board/c6-a/bytes"), 426);
    assert_eq!(serve.get_status("/board/c6-a/control"), 426);
    assert_eq!(serve.get_status("/board/c6-z/bytes"), 404);
    assert_eq!(serve.get_status("/"), 404);
}

/// A binary frame on `/bytes` is delivered to the guest as bytes: the board
/// answers a `M!` request the test builds with `lpc_wire`'s own framing,
/// which is the only place a frame is ever built.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn a_frame_the_client_writes_is_answered_by_the_guest() {
    let Some(elf) = reference_elf("a_frame_the_client_writes_is_answered") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a"], &[]);
    let mut bytes = serve.bytes("c6-a");

    // `lpc_wire::json::to_serial_line` builds every `M!` line (PR #538); the
    // emulator never builds one and neither does the door.
    let request = lpc_wire::json::to_serial_line(&lpc_wire::ClientMessage {
        id: 1,
        msg: lpc_wire::ClientRequest::Hello,
    })
    .expect("a hello frames");
    bytes
        .send(Message::Binary(request.clone().into_bytes()))
        .expect("writing the request");

    let answer = read_until(&mut bytes, |text| text.contains("\"id\":1"));
    assert!(
        answer.contains("\"id\":1"),
        "the guest answered request 1:\n{answer}"
    );
    assert!(
        !answer.contains("dropping unparseable"),
        "the guest lost bytes:\n{answer}"
    );
}

/// **No client means nobody is reading** — the default door delivers no
/// pre-attach backlog.
///
/// `docs/defects/2026-09-23-emulated-usb-port-drains-with-no-client-attached.md`:
/// the door used to power its boards on with the port open and draining and
/// nobody connected, so every write the firmware made succeeded and the
/// first client was replayed all of it — 299 heartbeats (~160 KB) in the
/// run that found it. A board on a desk with no application holding the
/// port drops those frames instead: its own write timeouts latch "host not
/// draining", and a later application gets at most what the 64-byte IN FIFO
/// still holds.
///
/// So: the DEFAULT serve (no `--usb-host`), three heartbeat intervals of the
/// board's own clock with no client, then connect. The port must still be
/// closed with at most a FIFO's worth waiting, and the first whole heartbeat
/// the client reads must be one the board wrote after it connected, which
/// its own `uptime_ms` says. A replayed backlog would open with the first
/// heartbeat of the boot. Every clock here is the guest's; the wall net only
/// ends a run that never gets there.
#[test]
#[ignore = "needs the shipped reference image; run through `just test-emu-serve`"]
fn with_no_client_the_default_port_is_closed_and_nothing_is_replayed() {
    let Some(elf) = reference_elf("with_no_client_the_default_port_is_closed") else {
        return;
    };
    let serve = Serve::start(&elf, &["c6-a"], &[]);
    let mut control = serve.control("c6-a");

    let power_on = control.cmd("state");
    assert!(
        power_on.contains("host=attached") && power_on.contains("draining=false"),
        "the default is a cable in and a port nobody has opened: {power_on}"
    );

    // Three heartbeat intervals of guest time with nobody on the byte
    // socket. The firmware wrote its boot hello and at least two heartbeats
    // in here; none of them may be waiting for the client below.
    let before = serve.wait_for_guest_micros(&mut control, 3 * HEARTBEAT_INTERVAL_MICROS);
    assert!(
        before.contains("draining=false"),
        "no client, so the port is still closed: {before}"
    );
    let waiting = state_field(&before, "in_pending").expect("`state` names in_pending");
    assert!(
        waiting <= IN_FIFO_BYTES,
        "at most the IN FIFO may hold anything for a later reader: {before}"
    );

    let mut bytes = serve.bytes("c6-a");
    let text = read_until(&mut bytes, |text| first_heartbeat(text).is_some());
    let (stale_prefix, first_uptime) = first_heartbeat(&text).expect("read until one arrived");
    eprintln!(
        "no-client backlog: {waiting} B pending in the IN FIFO at attach, {stale_prefix} B \
         before the first whole heartbeat (uptime_ms {first_uptime})"
    );

    assert!(
        first_uptime >= 2 * HEARTBEAT_INTERVAL_MICROS / 1_000,
        "the first heartbeat the client read was written before it connected — the \
         pre-attach backlog was replayed ({stale_prefix} B ahead of it):\n{text}"
    );
    assert!(
        !text[..stale_prefix].contains("\"hello\""),
        "the boot hello was replayed to a client that connected after it:\n{text}"
    );
    assert!(
        stale_prefix <= MAX_BYTES_BEFORE_FIRST_FRESH_FRAME,
        "{stale_prefix} B arrived ahead of the first fresh heartbeat:\n{text}"
    );
}

/// Nothing in this file asserts a cycle, a microsecond or a duration — a
/// socket is not deterministic, and a test that pinned one would be pinning
/// the host's clock. This checks itself.
#[test]
fn no_test_in_this_file_asserts_a_cycle() {
    let source = include_str!("emu_serve_door.rs");
    for (n, line) in source.lines().enumerate() {
        let line = line.trim();
        if !line.starts_with("assert") {
            continue;
        }
        for forbidden in ["cyc=", "us=", "elapsed", "Duration", "millis", "secs("] {
            assert!(
                !line.contains(forbidden),
                "line {}: a socket is not deterministic, so `{forbidden}` is not something to \
                 assert (lp-emu/esp/README.md §Determinism): {line}",
                n + 1
            );
        }
    }
    // …and the guard is only worth having if it can see the file at all.
    assert!(source.contains("fn no_test_in_this_file_asserts_a_cycle"));
}

/// `fw-esp32-common`'s `HEARTBEAT_INTERVAL_MS`, in guest microseconds.
const HEARTBEAT_INTERVAL_MICROS: u64 = 5_000_000;

/// USB-Serial-JTAG's IN endpoint FIFO: the most an unopened port can still
/// be holding when an application opens it.
const IN_FIFO_BYTES: u64 = 64;

/// What may precede the first fresh heartbeat on a port that was opened
/// late: the FIFO's leftovers, and the log lines the firmware writes between
/// the open and its next heartbeat (the not-draining latch lifting, the idle
/// loop's own chatter). A replayed backlog is the boot console plus a hello
/// plus two heartbeats — many kilobytes — so the gap is wide.
const MAX_BYTES_BEFORE_FIRST_FRESH_FRAME: usize = 1024;

/// The first whole, parseable heartbeat line in `text`: how many bytes came
/// before it, and its `uptime_ms`. A line the IN FIFO's stale bytes were
/// glued onto does not parse, and so does not count.
fn first_heartbeat(text: &str) -> Option<(usize, u64)> {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let start = at;
        at += line.len();
        if !line.ends_with('\n') {
            break;
        }
        let Some(json) = line.trim_end().strip_prefix("M!") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
            continue;
        };
        if let Some(uptime) = value["msg"]["heartbeat"]["uptime_ms"].as_u64() {
            return Some((start, uptime));
        }
    }
    None
}
