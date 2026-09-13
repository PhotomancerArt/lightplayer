//! **The socket form of the S3's link**, end to end: a byte client and a
//! control client driving one machine.
//!
//! This is the one test in the crate that spawns the binary and speaks TCP,
//! and the one place a wall clock is allowed: a socket's bytes arrive when
//! the host writes them, so the run is not deterministic and the timeouts
//! here are a safety net, not an input. What it proves is that the two
//! listeners exist, that a byte client and a control client can be connected
//! at once, that the **coupling rule** holds — a client on the byte socket is
//! an application opening the port, and a cable is never implied — and that
//! the three host states and the transitions between them read back through
//! `state`.
//!
//! ⚠️ **What a client receives here is the boot log, not an answer to a
//! request.** The firmware's server loop is behind `mount_filesystem`, which
//! is behind P06's flash engine — see `tests/boot_idle.rs` for the register
//! the boot ends up spinning on. So the OUT path is exercised as far as it
//! can honestly be exercised in this phase: host bytes cross the socket,
//! land as an OUT packet, and `state`'s `out_queued` says so. A guest that
//! answers them is P06's.
//!
//! The emulated timeout is long and the child is killed when the assertions
//! are done, so the test costs what it uses rather than what it asked for.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::time::{Duration, Instant};

use lp_emu_esp32s3::test_support::{fw_esp32s3_image, skip_notice};

/// The wall-clock net on every read here. Generous: a loaded CI box is slow,
/// and a flake in a socket test costs more than a slow one.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

struct Emulator {
    child: Child,
    usb_port: u16,
    control_port: u16,
}

impl Drop for Emulator {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Read the two `… listening on 127.0.0.1:<port>` lines the machine prints
/// when it binds, so the test never has to pick a port and never races
/// another one.
fn bound_port(reader: &mut BufReader<ChildStderr>, prefix: &str) -> u16 {
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        if reader
            .read_line(&mut line)
            .expect("reading the child's stderr")
            == 0
        {
            break;
        }
        if let Some(rest) = line.trim().strip_prefix(prefix)
            && let Some((_, port)) = rest.rsplit_once(':')
        {
            return port.parse().expect("a port");
        }
    }
    panic!("the emulator never printed `{prefix}…`");
}

fn spawn() -> Option<(Emulator, BufReader<ChildStderr>)> {
    let elf = match fw_esp32s3_image() {
        Ok(path) => path,
        Err(reason) => {
            skip_notice("usb_socket", &reason);
            return None;
        }
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_lp-emu-esp32s3"))
        .args(["--elf", elf.to_str().expect("a utf-8 path")])
        // A cable in, port closed: the client's connect is what opens it,
        // which is the coupling rule under test.
        .args(["--usb-host", "attached-idle"])
        .args(["--usb-sj", "tcp:127.0.0.1:0"])
        .args(["--control", "tcp:127.0.0.1:0"])
        .args(["--timeout", "600s", "--wall-timeout", "120s"])
        .arg("--strict-bus")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawning lp-emu-esp32s3");
    let mut reader = BufReader::new(child.stderr.take().expect("piped"));
    let usb_port = bound_port(&mut reader, "usb-sj listening on ");
    let control_port = bound_port(&mut reader, "control listening on ");
    Some((
        Emulator {
            child,
            usb_port,
            control_port,
        },
        reader,
    ))
}

/// A control client: one command per line, one reply per command.
struct Control(BufReader<TcpStream>);

impl Control {
    fn connect(port: u16) -> Self {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connecting to --control");
        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .expect("timeout");
        Self(BufReader::new(stream))
    }

    fn cmd(&mut self, line: &str) -> String {
        self.0
            .get_mut()
            .write_all(format!("{line}\n").as_bytes())
            .expect("writing a command");
        self.0.get_mut().flush().expect("flushing");
        let mut reply = String::new();
        let n = self.0.read_line(&mut reply).expect("reading the reply");
        assert!(n > 0, "the channel closed instead of answering `{line}`");
        let reply = reply.trim_end().to_string();
        eprintln!("> {line}\n< {reply}");
        reply
    }

    /// The `state` reply's fields, as `key=value` pairs.
    fn state(&mut self) -> std::collections::BTreeMap<String, String> {
        let reply = self.cmd("state");
        assert!(reply.starts_with("ok state "), "{reply}");
        reply
            .split_whitespace()
            .filter_map(|w| w.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }
}

/// Poll `state` until `holds`, or fail with the last reply. The guest is
/// running while the test talks and a socket has no cycle of its own, so
/// anything the test asserts about the machine's state is a thing it waits
/// for rather than a thing it assumes.
fn wait_until(
    control: &mut Control,
    what: &str,
    holds: impl Fn(&std::collections::BTreeMap<String, String>) -> bool,
) {
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut last = String::new();
    while Instant::now() < deadline {
        let state = control.state();
        last = format!("{state:?}");
        if holds(&state) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("{what} never became true; last state {last}");
}

/// The common case: one field reaching one value.
fn wait_for(control: &mut Control, key: &str, want: &str) {
    let (k, w) = (key.to_string(), want.to_string());
    wait_until(control, &format!("`{key}` = `{want}`"), move |state| {
        state.get(&k).map(String::as_str) == Some(w.as_str())
    });
}

/// **The three states, the transitions, and the coupling rule**, over two
/// sockets at once.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn a_byte_client_and_a_control_client_drive_one_machine() {
    let Some((emu, _stderr)) = spawn() else {
        return;
    };
    let mut control = Control::connect(emu.control_port);

    // Before anyone connects: the cable is in, the port is closed, and the
    // first `[INIT]` line comes to sit in the IN endpoint because nobody
    // takes it. That is the attached-idle signature, read over a socket.
    let state = control.state();
    assert_eq!(state["host"], "attached");
    assert_eq!(state["draining"], "false");
    assert_eq!(state["sof"], "on", "frames arrive as soon as a cable does");
    wait_until(&mut control, "a packet waiting for a reader", |state| {
        state["in_pending"] != "0"
    });

    // Connecting to the byte socket IS an application opening the port.
    let mut bytes =
        TcpStream::connect(("127.0.0.1", emu.usb_port)).expect("connecting to --usb-sj");
    bytes.set_read_timeout(Some(READ_TIMEOUT)).expect("timeout");
    wait_for(&mut control, "draining", "true");

    // And what it then reads is what a host receives: **the held packet**,
    // released by the connect itself.
    //
    // ⚠️ The needle is the first line and not the last, on purpose. This
    // test runs in a **debug** build of the emulator, where reaching the end
    // of the `[INIT]` chain is tens of seconds of wall clock on a loaded box
    // — and what this test is about is the coupling, not how far the boot
    // gets. The whole chain, at guest speed and byte for byte, is
    // `tests/boot_idle.rs`'s.
    let mut got = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = Instant::now() + READ_TIMEOUT;
    while !String::from_utf8_lossy(&got).contains("[INIT] fw-esp32s3 boot")
        && Instant::now() < deadline
    {
        let n = bytes.read(&mut buf).expect("reading the byte socket");
        assert!(n > 0, "the byte socket closed early");
        got.extend_from_slice(&buf[..n]);
    }
    let text = String::from_utf8_lossy(&got);
    assert!(
        text.starts_with("[INIT] fw-esp32s3 boot"),
        "the packet held while the port was closed came out the moment a client \
         opened it: {:?}",
        &text[..text.len().min(60)]
    );

    // ⚠️ The OUT path, as far as it goes in P05: the client's bytes cross
    // the socket, land as an OUT packet, and `out_queued` says so. Nothing
    // answers them — the server loop is behind P06's flash — and asserting
    // the queue rather than an answer is the honest version of that.
    bytes
        .write_all(b"M!{\"id\":1,\"msg\":\"hello\"}\n")
        .expect("writing to the byte socket");
    bytes.flush().expect("flushing");
    wait_until(&mut control, "the host's bytes landed", |state| {
        state["out_queued"] != "0"
    });

    // `close` and `open` move the same state the coupling moved.
    assert!(control.cmd("close").starts_with("ok close "));
    assert_eq!(control.state()["draining"], "false");
    assert!(control.cmd("open").starts_with("ok open "));
    assert_eq!(control.state()["draining"], "true");

    // The cable is a separate thing from the port: `detach` takes it out
    // from under a client that is still connected.
    assert!(control.cmd("detach").starts_with("ok detach "));
    let state = control.state();
    assert_eq!(state["host"], "absent");
    assert_eq!(state["draining"], "false");
    assert_eq!(state["sof"], "off", "no frames without a cable");

    // A command that cannot be applied answers `err` and changes nothing,
    // and so does one that is not a command at all.
    let refused = control.cmd("open");
    assert!(
        refused.starts_with("err open: no host is attached"),
        "{refused}"
    );
    assert!(control.cmd("nonsense").starts_with("err unknown command "));
    assert!(
        control.cmd("wait 5").starts_with("err "),
        "`wait` is a --usb-script command, not a socket one"
    );
    // ⚠️ And the two verbs the C6 has that this machine does not: refused by
    // NAME, with the phase that owns them, so a script written against the
    // other machine says what is missing rather than "unknown command".
    for pad_verb in ["pin 20 1", "pins"] {
        let reply = control.cmd(pad_verb);
        assert!(reply.contains("pad fabric"), "{reply}");
        assert!(reply.contains("P07"), "{reply}");
    }
    assert_eq!(control.state()["host"], "absent", "nothing moved");

    // Plugged back in, the same client's socket is still the byte path.
    assert!(control.cmd("attach").starts_with("ok attach "));
    assert!(control.cmd("open").starts_with("ok open "));
    assert_eq!(control.state()["host"], "attached");
}

/// **The coupling's other edge**: a byte client that hangs up closes the
/// port, and reconnecting opens it — without ever moving the cable.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn a_byte_client_that_hangs_up_closes_the_port_and_reconnecting_opens_it() {
    let Some((emu, _stderr)) = spawn() else {
        return;
    };
    let mut control = Control::connect(emu.control_port);

    let bytes = TcpStream::connect(("127.0.0.1", emu.usb_port)).expect("connecting to --usb-sj");
    wait_for(&mut control, "draining", "true");
    drop(bytes);
    wait_for(&mut control, "draining", "false");
    // The cable never moved: only the application went away.
    assert_eq!(control.state()["host"], "attached");

    let _second = TcpStream::connect(("127.0.0.1", emu.usb_port)).expect("reconnecting");
    wait_for(&mut control, "draining", "true");
}

/// **The reset the guest cannot refuse.** On the C6 a guest that has set
/// `chip_rst` bit 2 makes this answer `err`; the S3 has no such bit, so the
/// verb is unconditional — and with `--reboot-on-reset` absent the machine
/// reports the request and ends the run, which is exit code 2.
#[test]
#[ignore = "needs LP_EMU_ESP32S3_ELF; run through `just test-emu-esp32s3-boot`"]
fn a_reset_over_the_control_channel_is_unconditional_on_this_chip() {
    let Some((mut emu, _stderr)) = spawn() else {
        return;
    };
    let mut control = Control::connect(emu.control_port);
    wait_for(&mut control, "sof", "on");

    let reply = control.cmd("reset");
    assert!(
        reply.starts_with("ok reset "),
        "no `chip_rst` on this part means no refusal to report: {reply}"
    );
    // The machine ends the run and names who asked (exit code 2): performing
    // it needs `--reboot-on-reset`, and even then there is no ROM-up boot to
    // choose a strap with until P06.
    let status = emu.child.wait().expect("the emulator exits");
    assert_eq!(
        status.code(),
        Some(2),
        "a reset this machine reports and cannot yet perform"
    );
}
