//! Each LAN connection's secure lp-link end, and the loop that services it.
//!
//! Web Bluetooth's per-session loop (`browser_ble/ble_link_port.rs`),
//! repeated for a WebSocket with the SAME [`LinkPortService`] — built secure,
//! on [`LinkConfig::ws`]'s datagrams:
//!
//! - **One link per connection.** `browser_websocket.js` moves a session's
//!   `generation` with every connect, drop and close; a new generation gets a
//!   new link with a fresh nonce and a fresh key walk, and the board — which
//!   makes its end per socket — starts a new session with it.
//! - **One frame per message, both ways.** Every binary message goes to
//!   [`LinkPortService::on_datagram`] whole; every frame the link sends is
//!   one message ([`browser_websocket::write_frame`]). The board's SYN
//!   carries its payload size and lp-link cuts this end's frames to it.
//! - **The key walk.** A new link presents the app's best key
//!   ([`KeyWalk`] over [`LinkKeys::keys_for`]); a refusal moves it on (an
//!   unknown key costs the board nothing; a wrong one is reported back so it
//!   is never presented there again), or waits out the board's backoff. The
//!   anonymous key is always last, so an open board — and a locked one, which
//!   comes up holding nothing until a password is typed — is always reached.
//! - **A new generation of keys reaches a link that is up.** When the app's
//!   keys for this address change (a password typed for a locked board) and
//!   the link holds a key that is no longer the best, the link is rekeyed in
//!   place: the session ends — read as a link reset, as over Web Serial — and
//!   a new one starts presenting the new key, on the same socket.
//! - **Serviced from the start of a connection**, at most every
//!   [`SERVICE_TICK_CAP`] plus a pass on every JS activity (a message, the
//!   link up or down), so the handshake does not wait for the model's open.
//!
//! **A socket close is the LAN's link reset.** Both ends lose the session
//! together, and the page hears it as `wi-fi link lost: …`, which fails what
//! is in flight and closes the model's link.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use js_sys::{Function, Reflect, Uint8Array};
use lpc_wire::lp_link::{LinkConfig, Micros};
use wasm_bindgen::{JsCast, JsValue, prelude::*};

use super::browser_websocket;
use super::ws_link_keys::link_keys;
use crate::device_link::link_port_edge::{
    SERVICE_TICK_CAP, now_micros, random_nonce, spawn_service_loop,
};
use crate::device_link::link_port_service::{LinkPortService, SecureLinkEvent};
use crate::device_link::wire_reader::{WireRead, device_log_level, packed_replies_wanted};
use crate::providers::network_link::{KeyRefusal, KeyWalk, KeyWalkStep, LinkKey, LinkKeys};

/// Reads a session keeps for a drainer that is not draining. Past this the
/// oldest go, and the journal is told once.
const READ_QUEUE_CAP: usize = 1_024;

/// The note a board that runs a plain link gets, once per connection.
pub const PLAIN_LINK_NOTE: &str =
    "wi-fi: this board runs a plain link; Studio reaches boards on Wi-Fi only over a secure one";

thread_local! {
    /// One link per LAN session, shared by every drainer of it — the model's
    /// link and, while it holds the wire, a conversation.
    static SESSIONS: RefCell<HashMap<u32, ServedSession>> = RefCell::new(HashMap::new());
}

/// A session's link, the connection it is for, its key walk, what it has
/// read, and whether its loop is running.
struct ServedSession {
    generation: u32,
    /// The board's socket URL: what its keys are looked up by.
    address: String,
    service: LinkPortService,
    walk: KeyWalk,
    /// A key to present once the board's backoff has passed.
    retry_at: Option<(Micros, LinkKey)>,
    reads: VecDeque<WireRead>,
    notes: Vec<String>,
    dropped_reads: usize,
    running: Rc<Cell<bool>>,
    wake: Option<WakeOnActivity>,
}

impl ServedSession {
    fn new(generation: u32, address: String, keys: &dyn LinkKeys) -> Self {
        let walk = KeyWalk::new(keys.keys_for(&address), keys.generation(&address));
        Self {
            generation,
            service: fresh_service(walk.current()),
            address,
            walk,
            retry_at: None,
            reads: VecDeque::new(),
            notes: Vec::new(),
            dropped_reads: 0,
            running: Rc::default(),
            wake: None,
        }
    }

    /// A new connection: a new link, a new walk. What the old one read goes
    /// with it.
    fn restart(&mut self, generation: u32, keys: &dyn LinkKeys) {
        self.generation = generation;
        self.walk = KeyWalk::new(keys.keys_for(&self.address), keys.generation(&self.address));
        self.service = fresh_service(self.walk.current());
        self.retry_at = None;
        self.reads.clear();
    }

    /// Answer what the handshake said: the next key, or a wait.
    fn answer_handshake(&mut self, now: Micros, keys: &dyn LinkKeys) {
        while let Some(event) = self.service.poll_secure_event() {
            match event {
                SecureLinkEvent::Refused(refusal) => {
                    if refusal == KeyRefusal::WrongKey && !self.walk.is_anonymous() {
                        keys.refused_wrong(&self.address, self.walk.current());
                    }
                    match self.walk.on_refused(refusal) {
                        KeyWalkStep::Present(key) => self.service.retry_with(&key),
                        KeyWalkStep::PresentAfter { key, after_ms } => {
                            self.retry_at = Some((now + Micros::from(after_ms) * 1_000, key));
                        }
                    }
                }
                SecureLinkEvent::PeerNotSecure => self.notes.push(PLAIN_LINK_NOTE.to_string()),
            }
        }
        if let Some((at, _)) = &self.retry_at
            && now >= *at
            && let Some((_, key)) = self.retry_at.take()
        {
            self.service.retry_with(&key);
        }
    }

    /// The app's keys for this address changed: walk the new ones, and move
    /// a link that holds anything but the new best key onto it.
    fn follow_keys(&mut self, now: Micros, keys: &dyn LinkKeys) {
        let generation = keys.generation(&self.address);
        if generation == self.walk.generation() {
            return;
        }
        let walk = KeyWalk::new(keys.keys_for(&self.address), generation);
        let best = walk.current().clone();
        let holds_best = walk.first_is(&self.walk.current().key_id);
        self.walk = walk;
        if holds_best && self.retry_at.is_none() {
            return;
        }
        self.retry_at = None;
        if self.service.is_up() {
            self.service.rekey(now, &best);
        } else {
            self.service.retry_with(&best);
        }
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
                "wi-fi: {} reads nobody drained were dropped",
                self.dropped_reads
            ));
            self.dropped_reads = 0;
        }
    }

    fn wake_in(&self, now: Micros) -> Micros {
        let link = self.service.wake_in(now, SERVICE_TICK_CAP);
        match &self.retry_at {
            Some((at, _)) => link.min(at.saturating_sub(now)),
            None => link,
        }
    }
}

/// A subscription to the session's JS activity that services it.
struct WakeOnActivity {
    callback: Option<Closure<dyn FnMut()>>,
    stop: Function,
}

impl Drop for WakeOnActivity {
    /// Unsubscribe, and LEAK the closure rather than free it: the session can
    /// be dropped from inside its own callback, and freeing a closure JS is
    /// still running is undefined. One small closure per session.
    fn drop(&mut self) {
        let _ = self.stop.call0(&JsValue::NULL);
        if let Some(callback) = self.callback.take() {
            callback.forget();
        }
    }
}

/// A secure link for a new connection: a fresh nonce, `ws()`'s datagrams,
/// the page's wire flags as they are now, and `key`.
fn fresh_service(key: &LinkKey) -> LinkPortService {
    LinkPortService::new_secure(
        LinkConfig::ws(),
        random_nonce(),
        packed_replies_wanted(),
        device_log_level(),
        key,
        fill_random,
    )
}

/// The handshake's entropy: `crypto.getRandomValues`. Every page that can
/// open a WebSocket to a board has it; one without it gets no handshake
/// rather than a predictable one.
fn fill_random(buf: &mut [u8]) {
    let global = js_sys::global();
    let bytes = Uint8Array::new_with_length(buf.len() as u32);
    let filled = Reflect::get(&global, &JsValue::from_str("crypto"))
        .ok()
        .filter(|crypto| crypto.is_object())
        .and_then(|crypto| {
            let fill = Reflect::get(&crypto, &JsValue::from_str("getRandomValues")).ok()?;
            fill.dyn_into::<Function>()
                .ok()?
                .call1(&crypto, &bytes)
                .ok()
        })
        .is_some();
    assert!(filled, "a secure link needs crypto.getRandomValues");
    bytes.copy_to(buf);
}

/// What one service pass found.
enum Serviced {
    /// The page has no such session (forgotten): its link is dropped.
    Gone,
    /// The session exists but is not connected.
    Down,
    /// Connected; come back within this long.
    Up(Micros),
}

/// Start servicing a session, if nothing has yet. Idempotent.
pub(crate) fn attach(session: u32) {
    service(session);
}

/// Stop servicing a session and drop its link (the session was forgotten).
pub(crate) fn detach(session: u32) {
    SESSIONS.with(|sessions| sessions.borrow_mut().remove(&session));
}

/// Queue one request (its JSON, no `M!`, no newline) on the session's link
/// and send what the link has to send now.
pub(crate) fn send_client_json(session: u32, json: &str) -> Result<(), String> {
    if !matches!(service(session), Serviced::Up(_)) {
        return Err("the wi-fi link is not connected".to_string());
    }
    SESSIONS
        .with(|sessions| {
            sessions
                .borrow_mut()
                .get_mut(&session)
                .map(|served| served.service.send_client_json(json))
        })
        .unwrap_or_else(|| Err("the wi-fi link is not connected".to_string()))?;
    service(session);
    Ok(())
}

/// Everything the session's link has read since the last drain, in order.
/// Services the session first, so a drainer never waits a tick for frames
/// already in the page.
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

/// What the session's link has said about itself since the last ask.
pub(crate) fn take_notes(session: u32) -> Vec<String> {
    SESSIONS.with(|sessions| {
        sessions
            .borrow_mut()
            .get_mut(&session)
            .map(|served| std::mem::take(&mut served.notes))
            .unwrap_or_default()
    })
}

/// Whether the session's lp-link is up (its secure handshake is done).
pub(crate) fn is_up(session: u32) -> bool {
    service(session);
    SESSIONS.with(|sessions| {
        sessions
            .borrow()
            .get(&session)
            .is_some_and(|served| served.service.is_up())
    })
}

/// One pass over a session: take what the page received, feed the link,
/// answer its handshake, follow the app's keys, and send what it has.
fn service(session: u32) -> Serviced {
    let Ok(taken) = browser_websocket::take_frames(session) else {
        detach(session);
        return Serviced::Gone;
    };
    let now = now_micros();
    let keys = link_keys();
    let (frames, wake, running) = SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let served = sessions
            .entry(session)
            .or_insert_with(|| ServedSession::new(taken.generation, taken.url.clone(), &*keys));
        if served.generation != taken.generation {
            served.restart(taken.generation, &*keys);
        }
        for frame in &taken.frames {
            served.service.on_datagram(now, frame);
        }
        served.answer_handshake(now, &*keys);
        served.follow_keys(now, &*keys);
        let mut out = Vec::new();
        if taken.connected {
            served
                .service
                .transmit(now, |frame| out.push(frame.to_vec()));
        }
        served.collect();
        (out, served.wake_in(now), Rc::clone(&served.running))
    });
    // No borrow is held past here: the writes and the loop call back into
    // JS, and the loop into `service`.
    for frame in frames {
        let _ = browser_websocket::write_frame(session, &frame);
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

/// Subscribe the session to its JS activity, once per session.
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
    let Ok(stop) = browser_websocket::on_activity(session, &callback) else {
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
