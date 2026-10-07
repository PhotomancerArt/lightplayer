//! Each Bluetooth session's lp-link end, and the loop that services it.
//!
//! Since `WIRE_PROTO_VERSION` 37 a board's Bluetooth links run lp-link on
//! [`LinkConfig::ble`]'s datagrams (plan
//! `lp2025/2026-09-28-1445-ble-on-lp-link`, P4). This is Web Serial's
//! per-port loop (`browser_serial_esp32/browser_serial.rs`), repeated for a
//! GATT session with the SAME [`LinkPortService`] (D1):
//!
//! - **One link per connection.** `browser_ble.js` moves a session's
//!   `generation` with every connect, drop and close. A new generation gets
//!   a new link with a fresh nonce, and the board — which makes its end at
//!   the subscribe — starts a new session with it. What the old connection
//!   left unread is dropped with it: a new connection is not the rest of the
//!   old one.
//! - **One frame per notification, one frame per write.** Every
//!   notification goes to [`LinkPortService::on_datagram`] whole; every frame
//!   the link has to send is one GATT write ([`browser_ble::write_frame`]).
//!   The board's SYN carries its payload size (`min(180, ATT MTU − 11)`), and
//!   lp-link cuts this end's frames to it — nothing here knows the MTU.
//! - **Room, not a queue.** A GATT write is awaited, one at a time, so a
//!   frame handed to the page waits there behind the one in flight. At most
//!   [`WRITE_ROOM`] frames are ever queued in the page; the rest stay in the
//!   link, where a resend is still a choice and an acknowledgement is always
//!   the latest.
//! - **One write policy per link** ([`BleWritePolicy`], chosen when the
//!   connection's link is made: the browser's default, or the page's
//!   `?ble-writes=`). It sets the link's transmit window — the cap on frames
//!   in flight — and, per frame, whether the write asks for a response: data
//!   frames without one on a desktop browser, every SYN and ACK-only frame
//!   with one, and every frame with one while the link hears nothing.
//! - **Serviced from the start of a connection, not from the model's open.**
//!   The board starts its 10 s login clock at the subscribe, and its link
//!   sends SYNs from then on, so the session is serviced as soon as anything
//!   holds a handle to it ([`attach`]): a loop at most every
//!   [`SERVICE_TICK_CAP`], plus a pass on every JS activity (a notification,
//!   a finished write, the link up or down — a hidden tab throttles timers,
//!   not these). The reads wait here, the board's hello among them, until
//!   the model's link or a borrowed conversation drains them.
//!
//! - **Channel 3 rides the same link** (the over-the-air update, M7 P12):
//!   [`send_update`] and [`take_updates`] are Web Serial's, through the same
//!   [`LinkPortService`], so the update channel is refused until the board
//!   announced it on this connection (DS9) and a borrowed conversation never
//!   eats an update message. This end keeps at most the policy's
//!   `in_flight` frames in flight (DS11; 16 on a desktop browser).
//!
//! **A GATT disconnect is Bluetooth's link reset.** Both ends lose the
//! session together (the board drops its `Link` on disconnect), and the page
//! hears it as `bluetooth link lost: …` (`browser_ble.js`), which fails what
//! is in flight and closes the model's link. A reset *within* a connection —
//! the board's link gave up on a frame, or a page reload on the same GATT
//! connection restarted this end — is the link's own `Reset`, read as
//! [`WireRead::LinkReset`] exactly as over Web Serial (the USB cut-over's
//! D9). Per the board (P3), a reset on the same connection keeps its link id
//! and login tier: nothing here re-logs in.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use lpc_wire::lp_link::{LinkConfig, Micros};
use wasm_bindgen::prelude::*;

use super::browser_ble;
use crate::device_link::link_port_edge::{
    SERVICE_TICK_CAP, now_micros, random_nonce, spawn_service_loop,
};
use crate::device_link::link_port_service::LinkPortService;
use crate::device_link::wire_reader::{WireRead, device_log_level, packed_replies_wanted};
use crate::providers::browser_ble_write_policy::{BleWritePolicy, ble_write_policy_for};

/// Frames the page may hold on its write chain at once: the one being
/// written and the one after it, so the next write starts the moment the
/// last one is acknowledged, without waiting for a service pass.
pub const WRITE_ROOM: u32 = 2;

/// Reads a session keeps for a drainer that is not draining (a connected
/// board whose model link is not open heartbeats every 5 s). Past this the
/// oldest go, and the journal is told once.
const READ_QUEUE_CAP: usize = 1_024;

thread_local! {
    /// One link per Bluetooth session, shared by every drainer of it — the
    /// model's link and, while it holds the wire, a conversation.
    static SESSIONS: RefCell<HashMap<u32, ServedSession>> = RefCell::new(HashMap::new());
}

/// A session's link, the connection it is for, what it has read, and
/// whether its loop is running.
struct ServedSession {
    generation: u32,
    service: LinkPortService,
    reads: VecDeque<WireRead>,
    notes: Vec<String>,
    /// Reads dropped since the last note about it.
    dropped_reads: usize,
    running: Rc<Cell<bool>>,
    wake: Option<WakeOnActivity>,
    /// How this connection's link writes (see the module docs).
    policy: BleWritePolicy,
    /// Where the last bulk-traffic console line left off.
    traffic: TrafficLine,
}

impl ServedSession {
    fn new(generation: u32) -> Self {
        let policy = ble_write_policy_for(&browser_ble::browser_kind());
        Self {
            generation,
            service: fresh_service(policy),
            reads: VecDeque::new(),
            notes: vec![policy_note(policy)],
            dropped_reads: 0,
            running: Rc::default(),
            wake: None,
            policy,
            traffic: TrafficLine::default(),
        }
    }

    /// A new connection: a new lp-link session, under the policy as it is
    /// now. The old link's bulk traffic gets its last line first.
    fn reconnected(&mut self, generation: u32, now: Micros) {
        self.notes.extend(self.traffic.finish(&self.service, now));
        self.generation = generation;
        self.policy = ble_write_policy_for(&browser_ble::browser_kind());
        self.service = fresh_service(self.policy);
        self.reads.clear();
        self.traffic = TrafficLine::default();
        self.notes.push(policy_note(self.policy));
    }

    /// Move what the link read onto the session's queues, keeping the reads
    /// bounded.
    fn collect(&mut self) {
        self.reads.extend(self.service.take_reads());
        while self.reads.len() > READ_QUEUE_CAP {
            self.reads.pop_front();
            self.dropped_reads += 1;
        }
        self.notes.extend(self.service.take_notes());
        if self.dropped_reads > 0 && self.reads.len() < READ_QUEUE_CAP / 2 {
            self.notes.push(format!(
                "bluetooth: {} reads nobody drained were dropped",
                self.dropped_reads
            ));
            self.dropped_reads = 0;
        }
    }
}

/// A subscription to the session's JS activity that services it.
struct WakeOnActivity {
    callback: Option<Closure<dyn FnMut()>>,
    stop: js_sys::Function,
}

impl Drop for WakeOnActivity {
    /// Unsubscribe, and LEAK the closure rather than free it: the session can
    /// be dropped from inside its own callback (a pass that finds it gone),
    /// and freeing a closure JS is still running is undefined. One small
    /// closure per session.
    fn drop(&mut self) {
        let _ = self.stop.call0(&JsValue::NULL);
        if let Some(callback) = self.callback.take() {
            callback.forget();
        }
    }
}

/// The host's Bluetooth preset under `policy`: [`LinkConfig::ble`] with this
/// end's transmit window at the policy's cap on frames in flight (M7 DS11:
/// the OTA spike's best on Mac Chrome is 16 — S5c, 2026-10-02). The board
/// advertises how many it takes in its SYN — 8 while its engine runs, 32 in
/// core-only — and the link never sends more than the smaller of the two.
/// Every frame still goes out as one awaited write with at most
/// [`WRITE_ROOM`] queued in the page; the rest wait in the link.
pub fn host_link_config(policy: BleWritePolicy) -> LinkConfig {
    LinkConfig {
        tx_window: policy.in_flight,
        ..LinkConfig::ble()
    }
}

/// A link for a new connection: a fresh nonce, [`host_link_config`]'s
/// datagrams, and the page's wire flags as they are now.
fn fresh_service(policy: BleWritePolicy) -> LinkPortService {
    LinkPortService::new(
        host_link_config(policy),
        random_nonce(),
        packed_replies_wanted(),
        device_log_level(),
    )
}

/// The journal line a new link opens with: how it writes.
fn policy_note(policy: BleWritePolicy) -> String {
    format!("bluetooth: {}", policy.describe())
}

/// Bulk traffic, one journal line at most every [`TrafficLine::EVERY`]: each
/// direction's rate, the frames this end resent and the link's smoothed
/// round trip, so an update's speed and its losses read on the board's card
/// (and in a `?record=` session) with no capture. A quiet link — heartbeats,
/// an idle editor — never prints.
#[derive(Default)]
struct TrafficLine {
    at: Option<Micros>,
    bytes_tx: u64,
    bytes_rx: u64,
    frames_tx: u32,
    resends: u32,
    /// Whether this link ever printed (so its end prints too).
    printed: bool,
}

impl TrafficLine {
    /// Time between lines.
    const EVERY: Micros = 15_000_000;
    /// Bytes either way in one interval below which nothing prints.
    const BULK: u64 = 8 * 1024;

    /// A line if one is due.
    fn tick(&mut self, service: &LinkPortService, now: Micros) -> Option<String> {
        match self.at {
            None => {
                self.mark(service, now);
                None
            }
            Some(at) if now.saturating_sub(at) >= Self::EVERY => self.line(service, now, false),
            Some(_) => None,
        }
    }

    /// The link is over: its last line, if it ever carried bulk traffic.
    fn finish(&mut self, service: &LinkPortService, now: Micros) -> Option<String> {
        (self.printed || self.moved(service) >= Self::BULK)
            .then(|| self.line(service, now, true))
            .flatten()
    }

    fn moved(&self, service: &LinkPortService) -> u64 {
        let c = service.counters();
        (c.bytes_tx - self.bytes_tx) + (c.bytes_rx - self.bytes_rx)
    }

    fn line(&mut self, service: &LinkPortService, now: Micros, ended: bool) -> Option<String> {
        let at = self.at.unwrap_or(now);
        let c = service.counters();
        let secs = (now.saturating_sub(at) as f64 / 1e6).max(1e-3);
        let line = (ended || self.moved(service) >= Self::BULK).then(|| {
            let frames = c.frames_tx - self.frames_tx;
            let resent = c.resends - self.resends;
            let share = if frames == 0 {
                0.0
            } else {
                100.0 * f64::from(resent) / f64::from(frames)
            };
            self.printed = true;
            format!(
                "bluetooth: out {:.1} KiB/s, in {:.1} KiB/s over {secs:.0} s · {resent} of \
                 {frames} frames resent ({share:.1} %) · srtt {} ms · this link: {} resent of \
                 {} frames{}",
                (c.bytes_tx - self.bytes_tx) as f64 / 1024.0 / secs,
                (c.bytes_rx - self.bytes_rx) as f64 / 1024.0 / secs,
                service.srtt() / 1_000,
                c.resends,
                c.frames_tx,
                if ended { " (link ended)" } else { "" },
            )
        });
        self.mark(service, now);
        line
    }

    fn mark(&mut self, service: &LinkPortService, now: Micros) {
        let c = service.counters();
        self.at = Some(now);
        self.bytes_tx = c.bytes_tx;
        self.bytes_rx = c.bytes_rx;
        self.frames_tx = c.frames_tx;
        self.resends = c.resends;
    }
}

/// What one service pass found.
enum Serviced {
    /// The page has no such session (forgotten): its link is dropped.
    Gone,
    /// The session exists but is not connected (a drop, a close, a connect
    /// still in flight).
    Down,
    /// Connected; come back within this long.
    Up(Micros),
}

/// Start servicing a session, if nothing has yet. Idempotent.
pub(crate) fn attach(session: u32) {
    service(session);
}

/// Stop servicing a session and drop its link (the device was forgotten).
pub(crate) fn detach(session: u32) {
    SESSIONS.with(|sessions| sessions.borrow_mut().remove(&session));
}

/// Queue one request (its JSON, no `M!`, no newline) on the session's link
/// and write what the link has to send now. Errors when the session is not
/// connected, or when the link will not take the message (its send budget is
/// full, or the message is larger than a link message may be).
pub(crate) fn send_client_json(session: u32, json: &str) -> Result<(), String> {
    if !matches!(service(session), Serviced::Up(_)) {
        return Err("the bluetooth link is not connected".to_string());
    }
    SESSIONS
        .with(|sessions| {
            sessions
                .borrow_mut()
                .get_mut(&session)
                .map(|served| served.service.send_client_json(json))
        })
        .unwrap_or_else(|| Err("the bluetooth link is not connected".to_string()))?;
    service(session);
    Ok(())
}

/// Queue one channel-3 (update) message on the session's link and write
/// what the link has to send now (M7 P12, the Web Serial port's twin).
/// `Ok(false)`: the board has not announced the update channel on this
/// connection, so nothing was queued (DS9; the link notes it). Errors like
/// [`send_client_json`]'s.
pub(crate) fn send_update(session: u32, message: &[u8]) -> Result<bool, String> {
    if !matches!(service(session), Serviced::Up(_)) {
        return Err("the bluetooth link is not connected".to_string());
    }
    let queued = SESSIONS
        .with(|sessions| {
            sessions
                .borrow_mut()
                .get_mut(&session)
                .map(|served| served.service.send_update(message))
        })
        .unwrap_or_else(|| Err("the bluetooth link is not connected".to_string()))?;
    service(session);
    Ok(queued)
}

/// The board's channel-3 (update) messages since the last take, this
/// connection's only (a new connection is a new link, and the old one's
/// messages go with it). Drained by the model's link pump alone: a
/// conversation borrowing the wire never sees them.
pub(crate) fn take_updates(session: u32) -> Vec<Vec<u8>> {
    SESSIONS.with(|sessions| {
        sessions
            .borrow_mut()
            .get_mut(&session)
            .map(|served| served.service.take_updates())
            .unwrap_or_default()
    })
}

/// Everything the session's link has read since the last drain, in order:
/// wire messages (packed or not) and link resets. Services the session
/// first, so a drainer never waits a tick for frames already in the page.
pub(crate) fn take_reads(session: u32) -> Vec<WireRead> {
    service(session);
    SESSIONS.with(|sessions| {
        sessions
            .borrow_mut()
            .get_mut(&session)
            .map(|served| served.reads.drain(..).collect())
            .unwrap_or_default()
    })
}

/// What the session's link has said about itself since the last ask (up, a
/// stall, the packed opt-in's outcome — one note per change).
pub(crate) fn take_notes(session: u32) -> Vec<String> {
    SESSIONS.with(|sessions| {
        sessions
            .borrow_mut()
            .get_mut(&session)
            .map(|served| std::mem::take(&mut served.notes))
            .unwrap_or_default()
    })
}

/// Whether the session's lp-link is up (its handshake is done).
pub(crate) fn is_up(session: u32) -> bool {
    service(session);
    SESSIONS.with(|sessions| {
        sessions
            .borrow()
            .get(&session)
            .is_some_and(|served| served.service.is_up())
    })
}

/// One pass over a session: take what the page was notified, feed the link,
/// and write the frames it has room for. See the module docs.
fn service(session: u32) -> Serviced {
    let Ok(taken) = browser_ble::take_frames(session) else {
        detach(session);
        return Serviced::Gone;
    };
    let now = now_micros();
    let (frames, wake, running) = SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let served = sessions
            .entry(session)
            .or_insert_with(|| ServedSession::new(taken.generation));
        if served.generation != taken.generation {
            // A new connection: a new lp-link session. The old one's reads
            // and its link go with it (its loss was already said, as
            // `bluetooth link lost`, by the JS).
            served.reconnected(taken.generation, now);
        }
        for frame in &taken.frames {
            served.service.on_datagram(now, frame);
        }
        let mut out = Vec::new();
        if taken.connected {
            let room = WRITE_ROOM.saturating_sub(taken.writes_pending) as usize;
            // Every frame with response while the board is silent: the probe
            // a link only the page believes in fails (`ble_write_policy`).
            let stalled = served.service.is_stalled(now);
            let policy = served.policy;
            served.service.transmit_up_to(now, room, |frame| {
                out.push((frame.to_vec(), policy.with_response(frame, stalled)));
            });
            let line = served.traffic.tick(&served.service, now);
            served.notes.extend(line);
        }
        served.collect();
        (
            out,
            served.service.wake_in(now, SERVICE_TICK_CAP),
            Rc::clone(&served.running),
        )
    });
    // No borrow is held past here: the writes and the loop call back into
    // JS, and the loop into `service`.
    for (frame, with_response) in frames {
        // `false` means the connection went away since the take; the drop's
        // own activity services the session again.
        let _ = browser_ble::write_frame(session, &frame, with_response);
    }
    wake_on_activity(session);
    if !taken.connected {
        return Serviced::Down;
    }
    spawn_service_loop(running, move || match service(session) {
        Serviced::Up(wake) => Some(wake),
        Serviced::Gone | Serviced::Down => None,
    });
    Serviced::Up(wake)
}

/// Subscribe the session to its JS activity, once per session (see the
/// module docs).
fn wake_on_activity(session: u32) {
    let subscribed = SESSIONS.with(|sessions| {
        sessions
            .borrow()
            .get(&session)
            .is_none_or(|served| served.wake.is_some())
    });
    if subscribed {
        return;
    }
    let callback = Closure::<dyn FnMut()>::new(move || {
        service(session);
    });
    let Ok(stop) = browser_ble::on_activity(session, &callback) else {
        return;
    };
    let wake = WakeOnActivity {
        callback: Some(callback),
        stop,
    };
    SESSIONS.with(|sessions| {
        if let Some(served) = sessions.borrow_mut().get_mut(&session) {
            served.wake = Some(wake);
        }
    });
}
