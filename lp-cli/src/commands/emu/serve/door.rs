//! The WebSocket door: `GET /boards`, `/board/<id>/bytes`,
//! `/board/<id>/control`, and `GET /lans/<name>/browse`.
//!
//! Everything here is a **pump**. A binary frame's payload on `/bytes` is
//! exactly the bytes, both ways — no length prefix, no JSON envelope, no
//! control verb in band. `/control` carries the line protocol of
//! `lp-emu/esp/lp-emu-esp32c6/src/control.rs` verbatim: one command per line,
//! one reply per line, and this door filters no verb. It adds exactly one,
//! `renumber`, because the LAN is the door's and not the machine's: "this
//! board's next DHCP lease is a different address" (walk step W9), answered
//! `ok renumber lan=<name>` in its place in the reply order, or `err
//! renumber: …` for a board on no served LAN.
//!
//! `GET /lans/<name>/browse` asks a served LAN's probe for a DNS-SD service
//! ([`super::lan_browse`]).
//!
//! The coupling rule falls out for free (plan two PD2): the loopback TCP
//! client this door opens when a WebSocket arrives **is** the byte client the
//! machine's `service_host` watches, so connect ⇒ `open` and disconnect ⇒
//! `close` are the machine's own, not a re-implementation. `attach` and
//! `detach` are never implied by a socket, here or anywhere.
//!
//! The HTTP head is parsed by hand rather than by a framework because the
//! door has exactly three routes and one of them is an upgrade: pulling in a
//! server stack to answer `GET /boards` would be more moving parts than the
//! whole shim.
//!
//! Every plain HTTP reply carries `Access-Control-Allow-Origin: *`, so a
//! Studio page served from another origin can `fetch()` `/boards`: the door
//! is loopback-only dev tooling (`--listen 127.0.0.1:…` by default) with
//! nothing behind it that an origin check would protect.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;

use super::board::{Board, format_mac};
use super::lan_browse;
use super::served_lan::ServedLan;
use super::wire_tap::{TapDirection, WireTap};
use super::wire_tear::WireTear;

/// The largest HTTP request head this door will read before giving up. A
/// handshake is a few hundred bytes; this is a bound, not a budget.
const MAX_HEAD: usize = 8 * 1024;

/// Bytes read off the loopback link in one go. The link is a serial port's
/// worth of traffic, so this is generous.
const PUMP_BUF: usize = 16 * 1024;

/// The boards this door serves, in the order they were spelled, and the
/// LANs they share.
pub struct Registry {
    pub boards: Vec<Board>,
    pub lans: Vec<ServedLan>,
}

impl Registry {
    fn find(&self, id: &str) -> Option<&Board> {
        self.boards.iter().find(|b| b.id == id)
    }

    fn find_lan(&self, name: &str) -> Option<&ServedLan> {
        self.lans.iter().find(|l| l.name == name)
    }

    /// The running boards on LAN `name`: how many answers a browse waits for.
    fn running_on(&self, name: &str) -> usize {
        self.boards
            .iter()
            .filter(|b| b.lan.as_ref().is_some_and(|l| l.name == name))
            .filter(|b| !b.stopped.load(Ordering::SeqCst))
            .count()
    }

    /// The `GET /boards` body. Boring on purpose, and the ids are stable:
    /// this is what M3's in-page picker lists.
    pub fn boards_json(&self) -> String {
        let boards: Vec<serde_json::Value> = self
            .boards
            .iter()
            .map(|b| {
                let seams = b.seams.lock().expect("seam report poisoned").clone();
                serde_json::json!({
                    "id": b.id,
                    "mac": format_mac(&b.mac),
                    "chip": "esp32c6",
                    "bytes": format!("/board/{}/bytes", b.id),
                    "control": format!("/board/{}/control", b.id),
                    // Recomputed on every request, not once at power-on: a
                    // board flashed through esptool-js says `loaded` from
                    // the first flush after the write (plan two M5).
                    "flash": b.flash_state(),
                    "boot": b.boot,
                    "link": "usb-serial-jtag",
                    "state": if b.stopped.load(Ordering::SeqCst) { "stopped" } else { "running" },
                    // Auditable, never a gate: how many times this board has
                    // rebooted since the server started it (PD11).
                    "reboots": b.reboots.load(Ordering::SeqCst),
                    // …and how many of those took the LP domain with them
                    // (the `power-cycle` verb). A `reset` and a `power-cycle`
                    // are different things to this chip, and a page that
                    // offers both should be able to see which one it got.
                    "power_cycles": b.power_cycles.load(Ordering::SeqCst),
                    // The run's configuration label (`…+led=fast` with an
                    // emulator seam engaged) and this chip start's `SEAM`
                    // lines, so a page can say what it is running.
                    "configuration": seams.label,
                    "seams": seams.lines,
                    // Its seam endpoints, `<board>/<seam>` — what a medium
                    // would join. Empty until a capability seam engages.
                    "endpoints": b.endpoints(),
                    // On a served LAN (`lan=<name>`): its name, the board's
                    // forward (`lan:127.0.0.1:<port>`, what `lp-cli … lan:`
                    // takes), and its address once DHCP has bound one. All
                    // three null for a board on no served LAN.
                    "lan": b.lan.as_ref().map(|l| l.name.clone()),
                    "forward": b.lan.as_ref().map(|l| l.forward_spec()),
                    "address": b.lan.as_ref().and_then(|l| l.address()).map(|ip| ip.to_string()),
                })
            })
            .collect();
        serde_json::json!({ "boards": boards }).to_string()
    }
}

/// Which door a path names.
#[derive(Debug, PartialEq, Eq)]
enum Route<'a> {
    Boards,
    Bytes(&'a str),
    Control(&'a str),
    /// `/lans/<name>/browse`.
    Browse(&'a str),
    Unknown,
}

fn route(path: &str) -> Route<'_> {
    let path = path.split('?').next().unwrap_or(path);
    if path == "/boards" || path == "/boards/" {
        return Route::Boards;
    }
    if let Some(name) = path
        .strip_prefix("/lans/")
        .and_then(|rest| rest.strip_suffix("/browse"))
        && !name.is_empty()
        && !name.contains('/')
    {
        return Route::Browse(name);
    }
    if let Some(rest) = path.strip_prefix("/board/") {
        if let Some(id) = rest.strip_suffix("/bytes") {
            if !id.is_empty() && !id.contains('/') {
                return Route::Bytes(id);
            }
        }
        if let Some(id) = rest.strip_suffix("/control") {
            if !id.is_empty() && !id.contains('/') {
                return Route::Control(id);
            }
        }
    }
    Route::Unknown
}

/// Accept forever. One task per connection.
pub async fn run(listener: TcpListener, registry: Arc<Registry>) {
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let registry = Arc::clone(&registry);
                tokio::spawn(async move {
                    if let Err(e) = handle(stream, registry).await {
                        log::debug!("emu serve: {peer}: {e}");
                    }
                });
            }
            Err(e) => {
                eprintln!("emu serve: accept failed: {e}");
                return;
            }
        }
    }
}

/// One HTTP request head, byte at a time.
///
/// Byte at a time because the very next thing on a successful upgrade is a
/// WebSocket frame, and a buffered read that swallowed part of it would lose
/// bytes the pump is supposed to be moving unchanged.
async fn read_head(stream: &mut TcpStream) -> Result<String> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while head.len() < MAX_HEAD {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            bail!("the client hung up before finishing its request");
        }
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            return Ok(String::from_utf8_lossy(&head).into_owned());
        }
    }
    bail!("request head over {MAX_HEAD} bytes")
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
}

/// The head for every plain HTTP reply this door writes, `/boards`'s JSON
/// included as much as its 404s and 409s.
///
/// `Access-Control-Allow-Origin: *` is a plain `GET` with no custom
/// headers — a CORS "simple request" — so this alone is enough to let a
/// cross-origin page read the reply; it needs no `OPTIONS` preflight
/// handling alongside it. `*` is correct because the door only ever listens
/// on loopback by default: there is no origin here that a narrower value
/// would be protecting.
fn http_head(status: &str, kind: &str, body_len: usize) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {body_len}\r\n\
         Access-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    )
}

async fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) -> Result<()> {
    let mut response = http_head(status, kind, body.len());
    response.push_str(body);
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

async fn handle(mut stream: TcpStream, registry: Arc<Registry>) -> Result<()> {
    stream.set_nodelay(true)?;
    let head = read_head(&mut stream).await?;
    let mut parts = head.lines().next().unwrap_or_default().split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let upgrading = header(&head, "upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let key = header(&head, "sec-websocket-key").map(str::to_string);

    if method != "GET" {
        return respond(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain",
            "GET only\n",
        )
        .await;
    }

    match (route(&path), upgrading) {
        (Route::Boards, false) => {
            respond(
                &mut stream,
                "200 OK",
                "application/json",
                &registry.boards_json(),
            )
            .await
        }
        (Route::Boards, true) => {
            respond(
                &mut stream,
                "400 Bad Request",
                "text/plain",
                "/boards is a plain GET, not a WebSocket\n",
            )
            .await
        }
        // A board that does not exist is a 404 whatever was asked of it:
        // "upgrade required" on a name nobody has would send a client
        // hunting for a handshake bug it does not have.
        (Route::Bytes(id) | Route::Control(id), false) => {
            if registry.find(id).is_none() {
                return not_a_board(&mut stream, id).await;
            }
            respond(
                &mut stream,
                "426 Upgrade Required",
                "text/plain",
                &format!("/board/{id}/… is a WebSocket endpoint\n"),
            )
            .await
        }
        (Route::Bytes(id), true) => {
            let Some(board) = registry.find(id) else {
                return not_a_board(&mut stream, id).await;
            };
            let Some(key) = key else {
                return respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    "no Sec-WebSocket-Key\n",
                )
                .await;
            };
            // One client at a time, like `TcpHost`. A second byte client is
            // REFUSED rather than silently multiplexed: two applications
            // holding one serial port is not a state a board can be in.
            let Some(_claim) = Claim::take(&board.bytes_busy) else {
                return busy(&mut stream, id, "bytes").await;
            };
            let ws = accept(stream, &key).await?;
            pump_bytes(ws, board).await
        }
        (Route::Control(id), true) => {
            let Some(board) = registry.find(id) else {
                return not_a_board(&mut stream, id).await;
            };
            let Some(key) = key else {
                return respond(
                    &mut stream,
                    "400 Bad Request",
                    "text/plain",
                    "no Sec-WebSocket-Key\n",
                )
                .await;
            };
            let Some(_claim) = Claim::take(&board.control_busy) else {
                return busy(&mut stream, id, "control").await;
            };
            let ws = accept(stream, &key).await?;
            pump_control(ws, board).await
        }
        (Route::Browse(name), false) => {
            let Some(lan) = registry.find_lan(name) else {
                return respond(
                    &mut stream,
                    "404 Not Found",
                    "text/plain",
                    &format!("no LAN `{name}` — `emu serve --lan <name>=<fixture>` declares one\n"),
                )
                .await;
            };
            let query = path.split_once('?').map(|(_, q)| q).unwrap_or_default();
            match lan_browse::parse_query(query) {
                Ok(query) => {
                    let answer = lan_browse::browse(lan, &query, registry.running_on(name)).await;
                    respond(
                        &mut stream,
                        "200 OK",
                        "application/json",
                        &answer.to_string(),
                    )
                    .await
                }
                Err(why) => {
                    respond(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain",
                        &format!("{why}\n"),
                    )
                    .await
                }
            }
        }
        (Route::Browse(_), true) => {
            respond(
                &mut stream,
                "400 Bad Request",
                "text/plain",
                "/lans/<name>/browse is a plain GET, not a WebSocket\n",
            )
            .await
        }
        (Route::Unknown, _) => {
            respond(
                &mut stream,
                "404 Not Found",
                "text/plain",
                "GET /boards, ws /board/<id>/bytes, ws /board/<id>/control, \
                 GET /lans/<name>/browse\n",
            )
            .await
        }
    }
}

async fn not_a_board(stream: &mut TcpStream, id: &str) -> Result<()> {
    respond(
        stream,
        "404 Not Found",
        "text/plain",
        &format!("no board `{id}` — GET /boards lists them\n"),
    )
    .await
}

async fn busy(stream: &mut TcpStream, id: &str, which: &str) -> Result<()> {
    respond(
        stream,
        "409 Conflict",
        "text/plain",
        &format!(
            "board `{id}` already has a {which} client; one at a time, as a serial port has\n"
        ),
    )
    .await
}

/// Holds an endpoint's "one client at a time" flag for as long as it lives.
struct Claim(Arc<AtomicBool>);

impl Claim {
    fn take(flag: &Arc<AtomicBool>) -> Option<Claim> {
        flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Claim(Arc::clone(flag)))
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Write the 101 by hand and hand the socket to tungstenite already
/// upgraded. The head is read once, so nothing can eat a frame.
async fn accept(mut stream: TcpStream, key: &str) -> Result<WebSocketStream<TcpStream>> {
    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\
         Connection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        derive_accept_key(key.as_bytes())
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(WebSocketStream::from_raw_socket(stream, Role::Server, None).await)
}

/// `/board/<id>/bytes` — bytes stay bytes.
async fn pump_bytes(ws: WebSocketStream<TcpStream>, board: &Board) -> Result<()> {
    // Connecting to the machine's own byte socket is what opens the port:
    // the coupling rule is the machine's, and this pump only carries it.
    let link = TcpStream::connect(board.bytes_addr).await?;
    link.set_nodelay(true)?;
    let (mut link_read, mut link_write) = link.into_split();
    let (mut ws_write, mut ws_read) = ws.split();
    let mut buf = vec![0u8; PUMP_BUF];
    // Off (and free) unless `LP_EMU_WIRE_TAP` names a directory.
    let mut tap = WireTap::from_env(&board.id);
    // Off (and free) unless `LP_EMU_WIRE_TEAR` asks for a fault; counted
    // per connection, from its first byte. The tap records what the host
    // was sent, torn or not.
    let mut tear = WireTear::from_env()?;

    let result = loop {
        tokio::select! {
            incoming = ws_read.next() => match incoming {
                Some(Ok(Message::Binary(bytes))) => {
                    tap.record(TapDirection::ToBoard, &bytes);
                    link_write.write_all(&bytes).await?
                }
                // A text frame is accepted and its UTF-8 goes through as
                // bytes: a client that types into the port is not wrong,
                // and refusing it would be a dialect.
                Some(Ok(Message::Text(text))) => {
                    tap.record(TapDirection::ToBoard, text.as_bytes());
                    link_write.write_all(text.as_bytes()).await?
                }
                Some(Ok(Message::Ping(payload))) => ws_write.send(Message::Pong(payload)).await?,
                Some(Ok(Message::Pong(_) | Message::Frame(_))) => {}
                Some(Ok(Message::Close(_))) | None => break Ok(()),
                Some(Err(e)) => break Err(e.into()),
            },
            read = link_read.read(&mut buf) => match read {
                Ok(0) => break Ok(()),
                Ok(n) => {
                    let sent = tear.filter(&buf[..n]);
                    if !sent.is_empty() {
                        tap.record(TapDirection::ToHost, &sent);
                        ws_write.send(Message::Binary(sent)).await?
                    }
                }
                Err(e) => break Err(e.into()),
            },
        }
    };
    // The application closed the port: the right moment to write the flash
    // back, so a server that is killed next has already kept the project.
    board.flush_now.store(true, Ordering::SeqCst);
    let _ = ws_write.send(Message::Close(None)).await;
    result
}

/// `/board/<id>/control` — the line protocol, unchanged, plus the door's
/// own `renumber`.
///
/// One command per line in, one reply per line out, in order. A command from
/// a client that has hung up is dropped rather than queued, which is what
/// `TcpHost::write_to_client` already does by answering `false`. The machine
/// answers every command it is sent with exactly one line and says nothing
/// unasked, so a door verb's reply waits its turn behind the machine's
/// replies still owed ([`ReplyOrder`]).
async fn pump_control(ws: WebSocketStream<TcpStream>, board: &Board) -> Result<()> {
    let link = TcpStream::connect(board.control_addr).await?;
    link.set_nodelay(true)?;
    let (mut link_read, mut link_write) = link.into_split();
    let (mut ws_write, mut ws_read) = ws.split();
    let mut buf = vec![0u8; PUMP_BUF];
    let mut pending = Vec::<u8>::new();
    let mut order = ReplyOrder::default();

    let result = loop {
        tokio::select! {
            incoming = ws_read.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let forward = order.commands(text.as_bytes(), |l| door_verb(l, board));
                    link_write.write_all(&forward).await?;
                }
                Some(Ok(Message::Binary(bytes))) => {
                    let forward = order.commands(&bytes, |l| door_verb(l, board));
                    link_write.write_all(&forward).await?;
                }
                Some(Ok(Message::Ping(payload))) => ws_write.send(Message::Pong(payload)).await?,
                Some(Ok(Message::Pong(_) | Message::Frame(_))) => {}
                Some(Ok(Message::Close(_))) | None => break Ok(()),
                Some(Err(e)) => break Err(e.into()),
            },
            read = link_read.read(&mut buf) => match read {
                Ok(0) => break Ok(()),
                Ok(n) => {
                    pending.extend_from_slice(&buf[..n]);
                    // One reply per frame, in order: a reader that split on
                    // frame boundaries instead of newlines would be reading
                    // the network's timing rather than the protocol.
                    while let Some(at) = pending.iter().position(|b| *b == b'\n') {
                        let line: Vec<u8> = pending.drain(..=at).collect();
                        let line = String::from_utf8_lossy(&line).trim_end().to_string();
                        order.machine_replied(line);
                    }
                }
                Err(e) => break Err(e.into()),
            },
        }
        for line in order.ready() {
            ws_write.send(Message::Text(line)).await?;
        }
    };
    let _ = ws_write.send(Message::Close(None)).await;
    result
}

/// The control channel's replies, in the order their commands came.
#[derive(Default)]
struct ReplyOrder {
    /// One per command not yet answered to the client: `None` is owed by
    /// the machine, `Some` is the door's own reply waiting its turn.
    owed: std::collections::VecDeque<Option<String>>,
    /// Replies in order, not yet sent.
    out: Vec<String>,
}

impl ReplyOrder {
    /// A client frame: answer the door's verbs in place, and return what
    /// goes on to the machine.
    fn commands(&mut self, bytes: &[u8], door: impl Fn(&[u8]) -> Option<String>) -> Vec<u8> {
        let mut forward = Vec::new();
        for line in as_lines(bytes)
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
        {
            match door(line) {
                Some(reply) => self.owed.push_back(Some(reply)),
                None => {
                    self.owed.push_back(None);
                    forward.extend_from_slice(line);
                    forward.push(b'\n');
                }
            }
        }
        self.settle();
        forward
    }

    /// One line from the machine: the reply to the oldest command it owes.
    fn machine_replied(&mut self, line: String) {
        match self.owed.iter().position(Option::is_none) {
            Some(at) => {
                // Everything before it is the door's, already in order.
                for _ in 0..at {
                    if let Some(Some(reply)) = self.owed.pop_front() {
                        self.out.push(reply);
                    }
                }
                self.owed.pop_front();
                self.out.push(line);
            }
            // The machine never speaks unasked; if it did, pass it on.
            None => self.out.push(line),
        }
        self.settle();
    }

    /// The door's replies at the front go out as soon as nothing is owed
    /// before them.
    fn settle(&mut self) {
        while let Some(Some(_)) = self.owed.front() {
            if let Some(Some(reply)) = self.owed.pop_front() {
                self.out.push(reply);
            }
        }
    }

    fn ready(&mut self) -> Vec<String> {
        std::mem::take(&mut self.out)
    }
}

/// A verb the door answers itself, or `None` for the machine's. Only
/// `renumber`: the LAN is the door's.
fn door_verb(line: &[u8], board: &Board) -> Option<String> {
    let line = std::str::from_utf8(line).ok()?.trim();
    let mut words = line.split_whitespace();
    if words.next()? != "renumber" {
        return None;
    }
    if words.next().is_some() {
        return Some("err renumber: takes no arguments".to_string());
    }
    Some(match &board.lan {
        Some(lan) => {
            lan.renumber_next_lease();
            format!(
                "ok renumber lan={} board={} (its next DHCP lease is a different address)",
                lan.name, board.id
            )
        }
        None => format!(
            "err renumber: board `{}` is on no served LAN (`lan=<name>` on its --board)",
            board.id
        ),
    })
}

/// Whatever the client wrote, as `\n`-terminated lines.
///
/// A WebSocket frame is not a line: a client may send `state` with no
/// newline, or three commands in one frame. Both mean the same thing to the
/// machine's parser, and this is where they are made to.
fn as_lines(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    for line in bytes.split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_routes_and_nothing_else() {
        assert_eq!(route("/boards"), Route::Boards);
        assert_eq!(route("/board/c6-a/bytes"), Route::Bytes("c6-a"));
        assert_eq!(route("/board/c6-b/control"), Route::Control("c6-b"));
        assert_eq!(route("/board/c6-a/bytes?x=1"), Route::Bytes("c6-a"));
        assert_eq!(route("/lans/home/browse"), Route::Browse("home"));
        assert_eq!(
            route("/lans/home/browse?service=_lightplayer._tcp.local"),
            Route::Browse("home")
        );
        assert_eq!(route("/lans//browse"), Route::Unknown);
        assert_eq!(route("/lans/a/b/browse"), Route::Unknown);
        assert_eq!(route("/board//bytes"), Route::Unknown);
        assert_eq!(route("/board/c6-a"), Route::Unknown);
        assert_eq!(route("/"), Route::Unknown);
    }

    #[test]
    fn a_frame_becomes_lines_whatever_shape_it_arrived_in() {
        assert_eq!(as_lines(b"state"), b"state\n");
        assert_eq!(as_lines(b"state\n"), b"state\n");
        assert_eq!(as_lines(b"state\r\n"), b"state\n");
        assert_eq!(as_lines(b"attach\nstate\n"), b"attach\nstate\n");
        assert_eq!(as_lines(b"\n\n"), b"");
    }

    #[test]
    fn headers_are_read_case_insensitively() {
        let head = "GET /x HTTP/1.1\r\nUpgrade: WebSocket\r\nSec-WebSocket-Key: abc\r\n\r\n";
        assert_eq!(header(head, "upgrade"), Some("WebSocket"));
        assert_eq!(header(head, "sec-websocket-key"), Some("abc"));
        assert_eq!(header(head, "host"), None);
    }

    #[test]
    fn boards_json_carries_the_cors_header_so_a_studio_page_can_read_it() {
        let head = http_head("200 OK", "application/json", 2);
        assert!(
            head.contains("Access-Control-Allow-Origin: *\r\n"),
            "GET /boards reply is missing Access-Control-Allow-Origin: {head}"
        );
    }

    #[test]
    fn a_404_carries_the_cors_header_too() {
        let head = http_head("404 Not Found", "text/plain", 0);
        assert!(
            head.contains("Access-Control-Allow-Origin: *\r\n"),
            "404 reply is missing Access-Control-Allow-Origin: {head}"
        );
    }

    /// The door's `renumber` answers in its place in the reply order: after
    /// the machine's replies still owed for the commands before it, before
    /// the ones after it.
    #[test]
    fn a_door_verb_waits_its_turn_behind_the_machines_replies() {
        let door = |line: &[u8]| (line == b"renumber").then(|| "ok renumber".to_string());
        let mut order = ReplyOrder::default();
        let forwarded = order.commands(
            b"state
renumber
reset
",
            door,
        );
        assert_eq!(
            forwarded,
            b"state
reset
",
            "the door's verb never reaches the machine"
        );
        assert!(order.ready().is_empty(), "`state` is still owed");
        order.machine_replied("ok state".to_string());
        assert_eq!(order.ready(), ["ok state", "ok renumber"]);
        order.machine_replied("ok reset".to_string());
        assert_eq!(order.ready(), ["ok reset"]);
        // Nothing owed: the door's reply goes at once.
        assert!(order.commands(b"renumber", door).is_empty());
        assert_eq!(order.ready(), ["ok renumber"]);
    }
}
