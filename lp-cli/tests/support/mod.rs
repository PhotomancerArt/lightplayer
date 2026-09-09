//! A `lp-cli emu serve` under test, and the two clients that drive it.
//!
//! Shared by `emu_serve_door.rs` and `emu_serve_walk.rs`. Everything here is
//! a **safety net with a wall clock**, never an input: a socket is not
//! deterministic (`lp-emu/esp/README.md` §Determinism), so a test waits for
//! an *outcome* to appear and gives up after a generous while rather than
//! asserting when it appeared.

#![allow(dead_code, reason = "each test file uses part of this")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::time::{Duration, Instant};

use tungstenite::{Message, WebSocket};

/// The wall-clock net on everything here. Generous on purpose: a loaded box
/// is slow, and a flake in a socket test costs more than a slow one.
pub const NET: Duration = Duration::from_secs(60);

/// How long to keep asking `/boards` or `state` before giving up on an
/// outcome that has not appeared.
const POLL: Duration = Duration::from_millis(100);

/// The reference image the committed emulator claims were captured against,
/// or `None` with a skip notice printed — the same honest skip
/// `tests/upload_walk_usb.rs` makes.
pub fn reference_elf(test: &str) -> Option<PathBuf> {
    use lp_emu_esp32c6::test_support::{ReferenceImage, reference_image, skip_notice};
    match reference_image(&ReferenceImage::SHIPPED_USB) {
        Ok(path) => Some(path),
        Err(reason) => {
            skip_notice(test, &reason);
            None
        }
    }
}

pub fn skip(test: &str, reason: &str) {
    lp_emu_esp32c6::test_support::skip_notice(test, reason);
}

/// A running `lp-cli emu serve`, killed when it goes out of scope.
pub struct Serve {
    child: Child,
    port: u16,
    dir: PathBuf,
}

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Serve {
    /// Start a server holding one board per id, with its own scratch state
    /// and console directory.
    pub fn start(elf: &Path, ids: &[&str], extra: &[&str]) -> Serve {
        let dir = scratch();
        Serve::start_in(elf, ids, extra, dir)
    }

    /// Start a server on an existing state directory — what gate 5 needs to
    /// ask whether the flash survived the last one.
    pub fn start_in(elf: &Path, ids: &[&str], extra: &[&str], dir: PathBuf) -> Serve {
        std::fs::create_dir_all(&dir).expect("the scratch dir");
        let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
        command.args(["emu", "serve", "--listen", "127.0.0.1:0"]);
        for id in ids {
            command.arg("--board");
            command.arg(format!("{id}={}", elf.display()));
        }
        command.arg("--state-dir").arg(&dir);
        command.arg("--console-dir").arg(&dir);
        command.args(extra);
        let mut child = command
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawning lp-cli emu serve");

        // The bound port, read off the server's own banner, so no test ever
        // picks a number and no two ever collide.
        let mut reader = BufReader::new(child.stderr.take().expect("piped"));
        let port = bound_port(&mut reader);
        // Keep draining stderr, or the child blocks on a full pipe once the
        // boards start talking.
        std::thread::Builder::new()
            .name("emu-serve-stderr".to_string())
            .spawn(move || {
                let mut sink = String::new();
                let _ = reader.read_to_string(&mut sink);
            })
            .expect("the stderr drain");

        Serve { child, port, dir }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn state_dir(&self) -> &Path {
        &self.dir
    }

    pub fn url(&self, path: &str) -> String {
        format!("ws://127.0.0.1:{}{path}", self.port)
    }

    /// `GET <path>`, body only.
    pub fn get(&self, path: &str) -> String {
        let (_status, body) = self.request(path);
        body
    }

    pub fn get_status(&self, path: &str) -> u16 {
        self.request(path).0
    }

    fn request(&self, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connecting");
        stream.set_read_timeout(Some(NET)).expect("timeout");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
        )
        .expect("writing the request");
        let mut text = String::new();
        stream.read_to_string(&mut text).expect("reading the reply");
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        let body = text.split_once("\r\n\r\n").map_or("", |(_, b)| b).to_string();
        (status, body)
    }

    /// The `/boards` entry for `id`.
    pub fn board(&self, id: &str) -> serde_json::Value {
        let body = self.get("/boards");
        let json: serde_json::Value = serde_json::from_str(&body).expect("`/boards` is JSON");
        json["boards"]
            .as_array()
            .expect("a boards array")
            .iter()
            .find(|b| b["id"] == id)
            .unwrap_or_else(|| panic!("no board `{id}` in {body}"))
            .clone()
    }

    pub fn reboots(&self, id: &str) -> u64 {
        self.board(id)["reboots"].as_u64().expect("a count")
    }

    /// A byte client on `/board/<id>/bytes`. Connecting is the application
    /// opening the port; dropping it is the application closing it.
    pub fn bytes(&self, id: &str) -> WebSocket<TcpStream> {
        self.try_bytes(id).expect("the byte endpoint accepted")
    }

    pub fn try_bytes(&self, id: &str) -> Result<WebSocket<TcpStream>, String> {
        self.open_ws(&format!("/board/{id}/bytes"))
    }

    pub fn try_control(&self, id: &str) -> Result<WebSocket<TcpStream>, String> {
        self.open_ws(&format!("/board/{id}/control"))
    }

    fn open_ws(&self, path: &str) -> Result<WebSocket<TcpStream>, String> {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).map_err(|e| e.to_string())?;
        stream.set_read_timeout(Some(NET)).map_err(|e| e.to_string())?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        let (socket, _response) = tungstenite::client::client(
            tungstenite::client::IntoClientRequest::into_client_request(self.url(path))
                .map_err(|e| e.to_string())?,
            stream,
        )
        .map_err(|e| e.to_string())?;
        Ok(socket)
    }

    /// A control client: one command per line, one reply per line.
    pub fn control(&self, id: &str) -> Control {
        Control(self.try_control(id).expect("the control endpoint accepted"))
    }

    /// The board's hello, read off a fresh byte client. The board says one
    /// periodically, so this is "wait for the next one" rather than "wait
    /// for the first" — which is also what a browser does.
    ///
    /// A frame boundary is not a line boundary: a WebSocket frame carries
    /// whatever the link had ready, so this waits for a **whole** line —
    /// a `\n` after the `"hello"` — rather than for the first frame the word
    /// appears in.
    pub fn hello(&self, id: &str) -> String {
        let mut bytes = self.bytes(id);
        let text = read_until(&mut bytes, |text| {
            text.match_indices("\"hello\"")
                .any(|(at, _)| text[at..].contains('\n'))
        });
        text.lines()
            .find(|line| line.contains("\"hello\""))
            .unwrap_or_default()
            .to_string()
    }

    /// Wait for a file to appear under the state dir. The flash is written
    /// back on a cadence, so "it is there" is an outcome to wait for and
    /// never a duration to assert.
    pub fn wait_for_file(&self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        let deadline = Instant::now() + NET;
        while Instant::now() < deadline {
            if path.is_file() {
                return path;
            }
            std::thread::sleep(POLL);
        }
        panic!("{} never appeared", path.display());
    }

    /// Ask `state` until the reply contains `needle`, then hand it back.
    /// A socket has no schedule, so this waits for the outcome rather than
    /// asserting when it arrived.
    pub fn wait_for_state(&self, control: &mut Control, needle: &str) -> String {
        let deadline = Instant::now() + NET;
        let mut last = String::new();
        while Instant::now() < deadline {
            last = control.cmd("state");
            if last.contains(needle) {
                return last;
            }
            std::thread::sleep(POLL);
        }
        panic!("`state` never reported `{needle}`; last was: {last}");
    }

    pub fn wait_for_reboot(&self, id: &str, at_least: u64) {
        let deadline = Instant::now() + NET;
        while Instant::now() < deadline {
            if self.reboots(id) >= at_least {
                return;
            }
            std::thread::sleep(POLL);
        }
        panic!("board `{id}` never reported {at_least} reboot(s)");
    }

    /// Wait for `needle` to appear in the board's console transcript, which
    /// the server rewrites on its own cadence.
    pub fn wait_for_console(&self, id: &str, needle: &str) -> String {
        let path = self.dir.join(format!("{id}.console.log"));
        let deadline = Instant::now() + NET;
        let mut text = String::new();
        while Instant::now() < deadline {
            text = std::fs::read(&path)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            if text.contains(needle) {
                return text;
            }
            std::thread::sleep(POLL);
        }
        panic!(
            "`{needle}` never appeared in {}; it held {} bytes:\n{}",
            path.display(),
            text.len(),
            tail(&text)
        );
    }

    pub fn console(&self, id: &str) -> String {
        std::fs::read(self.dir.join(format!("{id}.console.log")))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    /// Stop the server the way a person would, and wait for it to write
    /// every board's flash back.
    pub fn shutdown(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One command per line, one reply per line, on `/board/<id>/control`.
pub struct Control(WebSocket<TcpStream>);

impl Control {
    pub fn cmd(&mut self, line: &str) -> String {
        self.0
            .send(Message::Text(line.to_string()))
            .unwrap_or_else(|e| panic!("writing `{line}`: {e}"));
        let deadline = Instant::now() + NET;
        while Instant::now() < deadline {
            match self.0.read() {
                Ok(Message::Text(reply)) => return reply,
                Ok(Message::Binary(bytes)) => {
                    return String::from_utf8_lossy(&bytes).trim_end().to_string();
                }
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
                Ok(Message::Close(_)) => panic!("the control channel closed instead of answering `{line}`"),
                Err(e) => panic!("reading the reply to `{line}`: {e}"),
            }
        }
        panic!("no reply to `{line}` within the wall net")
    }
}

/// Read frames off a byte client until `done` says the text so far is
/// enough, then hand back everything read.
pub fn read_until(
    socket: &mut WebSocket<TcpStream>,
    done: impl Fn(&str) -> bool,
) -> String {
    let deadline = Instant::now() + NET;
    let mut text = String::new();
    while Instant::now() < deadline {
        match socket.read() {
            Ok(Message::Binary(bytes)) => text.push_str(&String::from_utf8_lossy(&bytes)),
            Ok(Message::Text(more)) => text.push_str(&more),
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => continue,
            Ok(Message::Close(_)) => break,
            Err(e) => panic!("reading the byte endpoint: {e}\n{}", tail(&text)),
        }
        if done(&text) {
            return text;
        }
    }
    panic!("the board never said it:\n{}", tail(&text))
}

fn tail(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}

/// The `emu serve: listening on http://127.0.0.1:<port>` line the server
/// prints when it binds, so no test picks a port.
fn bound_port(reader: &mut BufReader<ChildStderr>) -> u16 {
    const PREFIX: &str = "emu serve: listening on http://";
    let deadline = Instant::now() + NET;
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        if reader.read_line(&mut line).expect("reading stderr") == 0 {
            break;
        }
        if let Some(rest) = line.trim().strip_prefix(PREFIX)
            && let Some((_, port)) = rest.rsplit_once(':')
        {
            return port.parse().expect("a port");
        }
    }
    panic!("the server never printed `{PREFIX}…`");
}

/// A scratch directory of this process's own, so two tests never share a
/// board's flash.
pub fn scratch() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    std::env::temp_dir().join(format!("lp-emu-serve-{}-{n}", std::process::id()))
}
