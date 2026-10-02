//! A secure network link's login, end to end on the host: a secure lp-link
//! initiator (lpc-wire's `WireLinkPort::new_secure`, as lp-cli or Studio
//! would hold one) against `LpServer` behind a secure responder
//! (`support/secure_link_transport.rs`), over a reliable in-memory pipe (a
//! WebSocket's shape). The handshake's key match is the login: the hello on
//! every `Up` reports the tier the key's entry grants.
//!
//! Both ends use `usb()`'s framing: `WireLinkPort` is a stream port with
//! selective repeat, and a stream rides a WebSocket as well as a cable. The
//! no-ARQ `ws()` link is covered in lp-link's own tests.

extern crate alloc;

#[path = "support/secure_link_transport.rs"]
mod secure_link_transport;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_access::{
    DeviceAccessFile, LoginOutcome, ProjectAccessFile, SecretEntry, SecretKind, Tier, link_psk,
};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, LinkTrust, ServerTransport};
use lpc_wire::lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent};
use lpc_wire::lp_link::{LinkConfig, LinkState, Micros};
use lpc_wire::{
    ClientMessage, ClientRequest, HelloAuth, PortRead, TransportError, WireLinkPort,
    WireServerMessage, WireServerMsgBody,
};
use lpfs::{LpFs, LpFsMemory};

use secure_link_transport::SecureLinkTransport;

// ---- Grants -----------------------------------------------------------------

#[test]
fn a_play_key_gets_play_and_is_refused_edit() {
    let mut w = World::new(false, key(PLAY_SALT));
    w.run_until_up();
    assert_eq!(
        w.last_hello_auth(),
        HelloAuth {
            required: true,
            granted: Some(Tier::Play)
        }
    );
    assert!(matches!(
        w.request(ClientRequest::ListLoadedProjects),
        WireServerMsgBody::ListLoadedProjects { .. }
    ));
    assert!(matches!(
        w.request(ClientRequest::AccessList),
        WireServerMsgBody::NotPermitted { needs: Tier::Edit }
    ));
}

#[test]
fn an_edit_key_gets_edit() {
    let mut w = World::new(false, key(EDIT_SALT));
    w.run_until_up();
    assert_eq!(w.last_hello_auth().granted, Some(Tier::Edit));
    assert!(matches!(
        w.request(ClientRequest::AccessList),
        WireServerMsgBody::AccessList { .. }
    ));
}

#[test]
fn the_anonymous_key_gets_what_open_gives() {
    for (open, granted) in [(false, None), (true, Some(Tier::Play))] {
        let mut w = World::new(open, (KeyId::ANONYMOUS, Psk::ANONYMOUS));
        w.run_until_up();
        assert_eq!(
            w.last_hello_auth(),
            HelloAuth {
                required: true,
                granted
            },
            "open = {open}"
        );
        assert!(matches!(
            w.request(ClientRequest::AccessList),
            WireServerMsgBody::NotPermitted { needs: Tier::Edit }
        ));
    }
}

#[test]
fn a_key_in_a_loaded_projects_sidecar_grants_its_tier() {
    let mut w = World::new(false, key(SIDECAR_SALT));
    w.write_sidecar_and_load_project();
    w.run_until_up();
    assert_eq!(w.last_hello_auth().granted, Some(Tier::Play));
}

// ---- Refusals and backoff --------------------------------------------------

#[test]
fn an_unknown_key_is_refused_unknown_and_costs_nothing() {
    let mut w = World::new(false, (KeyId([0x77; 16]), Psk::new([1; 32])));
    // Far more unknown attempts than the free ones: none is charged.
    for _ in 0..6 {
        w.run_for(100_000);
        assert_eq!(w.refusal(), Some((RefusalReason::UnknownKey, 0)));
        w.port.restart(w.now);
    }
    let (k, psk) = key(EDIT_SALT);
    w.port.retry_with(k, psk);
    w.run_until_up();
    assert_eq!(w.last_hello_auth().granted, Some(Tier::Edit));
}

#[test]
fn a_wrong_key_is_refused_wrong_and_charged_to_the_backoff() {
    let mut w = World::new(false, (KeyId(EDIT_SALT), Psk::new([0xEE; 32])));
    for attempt in 1..=4 {
        w.run_for(100_000);
        assert_eq!(
            w.refusal(),
            Some((RefusalReason::WrongKey, 0)),
            "attempt {attempt}"
        );
        w.port.restart(w.now);
    }
    // Three free failures, then the fourth imposed a wait: the next lookup
    // is refused without being tried, the right key included.
    let (k, psk) = key(EDIT_SALT);
    w.port.retry_with(k, psk);
    w.run_for(100_000);
    match w.refusal() {
        Some((RefusalReason::Backoff, ms)) => assert!(ms > 0 && ms <= 2_000, "{ms}"),
        other => panic!("expected a backoff refusal, got {other:?}"),
    }
    assert_ne!(w.port.state(), LinkState::Established);
    // After the wait, the right key gets in.
    w.run_for(2_100_000);
    let (k, psk) = key(EDIT_SALT);
    w.port.retry_with(k, psk);
    w.run_until_up();
    assert_eq!(w.last_hello_auth().granted, Some(Tier::Edit));
}

// ---- The old login on a keyed link -----------------------------------------

#[test]
fn login_answer_is_refused_on_a_keyed_link_and_login_begin_takes_no_slot() {
    let mut w = World::new(false, (KeyId::ANONYMOUS, Psk::ANONYMOUS));
    w.run_until_up();
    // LoginBegin gives the offers (a typed-password client's salts) …
    let offers = match w.request(ClientRequest::LoginBegin) {
        WireServerMsgBody::LoginChallenge { offers, .. } => offers,
        other => panic!("expected the offers, got {other:?}"),
    };
    let salts: Vec<[u8; 16]> = offers.iter().map(|o| o.salt).collect();
    assert_eq!(salts, vec![PLAY_SALT, EDIT_SALT]);
    // … and registers no login: a BLE link can still begin one now.
    let ble = Link {
        id: LinkId::new(7),
        trust: LinkTrust::Untrusted,
    };
    assert!(matches!(
        w.request_on_other_link(ble, ClientRequest::LoginBegin),
        WireServerMsgBody::LoginChallenge { .. }
    ));
    // An HMAC answer, even a right one, never grants on a keyed link.
    let answer = w.request(ClientRequest::LoginAnswer { macs: Vec::new() });
    assert!(
        matches!(
            answer,
            WireServerMsgBody::LoginResult(LoginOutcome::Refused { retry_after_ms: 0 })
        ),
        "{answer:?}"
    );
    assert_eq!(w.server.link_tier(w.transport.link()), None);
}

// ---- Sessions ----------------------------------------------------------------

#[test]
fn a_reset_re_handshakes_and_re_grants() {
    let mut w = World::new(false, key(EDIT_SALT));
    w.run_until_up();
    let first = w.transport.link();
    w.port.restart(w.now);
    w.run_until_up();
    let second = w.transport.link();
    assert_ne!(first.id, second.id, "a new session is a new server link");
    assert_eq!(
        w.server.link_tier(first),
        None,
        "the old grant went with it"
    );
    assert_eq!(w.last_hello_auth().granted, Some(Tier::Edit));
    assert_eq!(w.server.link_tier(second), Some(Tier::Edit));
}

#[test]
fn a_key_removed_between_sessions_grants_nothing_on_the_next() {
    let mut w = World::new(false, key(PLAY_SALT));
    w.run_until_up();
    assert_eq!(w.last_hello_auth().granted, Some(Tier::Play));
    // Over USB, the play key is removed.
    w.request_on_other_link(
        Link::PRIMARY,
        ClientRequest::AccessRemove { salt: PLAY_SALT },
    );
    w.port.restart(w.now);
    w.run_for(200_000);
    assert_eq!(w.refusal(), Some((RefusalReason::UnknownKey, 0)));
    assert_ne!(w.port.state(), LinkState::Established);
}

// ---- The rig ------------------------------------------------------------------

const PLAY_SALT: [u8; 16] = [1; 16];
const EDIT_SALT: [u8; 16] = [2; 16];
const SIDECAR_SALT: [u8; 16] = [3; 16];

/// The access entry each salt names: a generated browser key.
fn entry(label: &str, tier: Tier, salt: [u8; 16]) -> SecretEntry {
    SecretEntry::from_password(label, tier, label.as_bytes(), salt, 1)
        .with_kind(SecretKind::Browser)
}

fn entry_for(salt: [u8; 16]) -> SecretEntry {
    match salt {
        PLAY_SALT => entry("play browser", Tier::Play, salt),
        EDIT_SALT => entry("edit browser", Tier::Edit, salt),
        _ => entry("project browser", Tier::Play, salt),
    }
}

/// What a client holding the entry with `salt` connects with.
fn key(salt: [u8; 16]) -> (KeyId, Psk) {
    (KeyId(salt), Psk::new(link_psk(&entry_for(salt).k)))
}

struct World {
    server: LpServer,
    transport: SecureLinkTransport,
    port: WireLinkPort,
    now: Micros,
    reads: Vec<PortRead>,
    other: OtherLinks,
    next_id: u64,
}

impl World {
    fn new(open: bool, (key_id, psk): (KeyId, Psk)) -> Self {
        let store = DeviceAccessFile {
            version: DeviceAccessFile::VERSION,
            secrets: vec![entry_for(PLAY_SALT), entry_for(EDIT_SALT)],
            ble_enabled: true,
            open,
        };
        let fs = LpFsMemory::new();
        fs.write_file(
            DeviceAccessFile::PATH.as_path(),
            store.to_json().unwrap().as_bytes(),
        )
        .unwrap();
        let mut server = LpServer::new(
            Rc::new(RefCell::new(MemoryOutputProvider::new())),
            Box::new(fs),
            "/projects/".as_path(),
            None,
            None,
            Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
                as Arc<dyn LpGraphics>,
        );
        server.set_entropy_source(Some(counting_entropy));
        World {
            server,
            transport: SecureLinkTransport::new(LinkConfig::usb(), 0xB0A2_D001, counting_entropy),
            port: WireLinkPort::new_secure(
                LinkConfig::usb(),
                0xC11E_0001,
                false,
                key_id,
                psk,
                counting_entropy,
            ),
            now: 0,
            reads: Vec::new(),
            other: OtherLinks::default(),
            next_id: 100,
        }
    }

    /// One millisecond of everything: the client's frames to the board, a
    /// server tick, the edge's hello for a link that came up, the board's
    /// frames to the client.
    fn step(&mut self) {
        while let Some(frame) = self.port.poll_transmit(self.now) {
            let frame = frame.to_vec();
            self.transport.on_bytes(self.now, &frame);
        }
        let incoming = block_on(self.transport.receive_all()).unwrap();
        block_on(self.server.tick_and_send(1, incoming, &mut self.transport)).expect("tick");
        for id in self.transport.take_came_up() {
            let hello = self.server.hello_for_link(Link {
                id,
                trust: LinkTrust::Keyed,
            });
            self.transport
                .send_now(&WireServerMessage::new(0, WireServerMsgBody::Hello(hello)));
        }
        while let Some(frame) = self.transport.poll_transmit(self.now) {
            self.port.on_bytes(self.now, &frame);
        }
        while let Some(read) = self.port.poll_read() {
            self.reads.push(read);
        }
        self.now += 1_000;
    }

    fn run_for(&mut self, us: Micros) {
        let end = self.now + us;
        while self.now < end {
            self.step();
        }
    }

    /// Run until the port is up and the session's hello has arrived.
    fn run_until_up(&mut self) {
        self.reads.clear();
        for _ in 0..3_000 {
            self.step();
            if self.port.state() == LinkState::Established && self.has_hello() {
                return;
            }
        }
        panic!(
            "not up: port {:?}, refusal {:?}",
            self.port.state(),
            self.port.poll_secure_event()
        );
    }

    fn has_hello(&self) -> bool {
        self.reads.iter().any(is_hello)
    }

    /// The `auth` of the newest hello the client read.
    fn last_hello_auth(&self) -> HelloAuth {
        self.reads
            .iter()
            .rev()
            .find_map(|read| match read {
                PortRead::Message(payload) => match &payload.message {
                    Ok(WireServerMessage {
                        msg: WireServerMsgBody::Hello(hello),
                        ..
                    }) => Some(hello.auth),
                    _ => None,
                },
                _ => None,
            })
            .expect("a hello")
    }

    /// The newest refusal the port reported, draining its events.
    fn refusal(&mut self) -> Option<(RefusalReason, u32)> {
        let mut last = None;
        while let Some(event) = self.port.poll_secure_event() {
            if let SecureEvent::Refused {
                reason,
                retry_after_ms,
            } = event
            {
                last = Some((reason, retry_after_ms));
            }
        }
        last
    }

    /// One request over the secure link; its reply.
    fn request(&mut self, msg: ClientRequest) -> WireServerMsgBody {
        self.next_id += 1;
        let id = self.next_id;
        self.reads.clear();
        self.port.send_client(&ClientMessage { id, msg }).unwrap();
        for _ in 0..200 {
            self.step();
            let found = self.reads.iter().position(|read| {
                matches!(read, PortRead::Message(payload)
                    if payload.message.as_ref().is_ok_and(|m| m.id == id))
            });
            if let Some(at) = found
                && let PortRead::Message(payload) = self.reads.remove(at)
                && let Ok(message) = payload.message
            {
                return message.msg;
            }
        }
        panic!("no reply to request {id}");
    }

    /// One request on another link (USB, BLE) of the same server.
    fn request_on_other_link(&mut self, link: Link, msg: ClientRequest) -> WireServerMsgBody {
        self.next_id += 1;
        let id = self.next_id;
        block_on(self.server.tick_and_send(
            1,
            vec![Incoming::on(link, ClientMessage { id, msg })],
            &mut self.other,
        ))
        .expect("tick");
        let (to, reply) = self.other.sent.pop().expect("a reply");
        assert_eq!((to, reply.id), (link.id, id));
        reply.msg
    }

    fn write_sidecar_and_load_project(&mut self) {
        let fs = self.server.base_fs_mut();
        let sidecar = ProjectAccessFile::new(vec![entry_for(SIDECAR_SALT)]);
        fs.write_file(
            "/projects/demo/.lp/access.json".as_path(),
            sidecar.to_json().unwrap().as_bytes(),
        )
        .unwrap();
        let manifest = alloc::format!("{{\"format\":{}}}", lpc_model::PROJECT_FORMAT_VERSION);
        fs.write_file("/projects/demo/project.json".as_path(), manifest.as_bytes())
            .unwrap();
        fs.write_file(
            "/projects/demo/module.json".as_path(),
            br#"{"kind":"Module","nodes":{}}"#,
        )
        .unwrap();
        self.server
            .load_project("projects/demo".as_path())
            .expect("the demo project loads");
    }
}

fn is_hello(read: &PortRead) -> bool {
    matches!(
        read,
        PortRead::Message(payload)
            if matches!(&payload.message, Ok(WireServerMessage { msg: WireServerMsgBody::Hello(_), .. }))
    )
}

/// The server's other links (USB, BLE), answered directly.
#[derive(Default)]
struct OtherLinks {
    sent: Vec<(LinkId, WireServerMessage)>,
}

impl ServerTransport for OtherLinks {
    async fn send(&mut self, link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push((link, msg));
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(Vec::new())
    }

    fn links(&self) -> Vec<Link> {
        vec![Link::PRIMARY]
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Test entropy: a different fill every call, never a real RNG.
fn counting_entropy(buf: &mut [u8]) {
    static NEXT: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(1);
    for b in buf {
        *b = NEXT.fetch_add(41, core::sync::atomic::Ordering::Relaxed);
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(output) = Future::poll(Pin::as_mut(&mut future), &mut cx) {
            return output;
        }
    }
}

fn noop_waker() -> Waker {
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    fn noop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    // SAFETY: every vtable function ignores the (null) data pointer.
    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}
