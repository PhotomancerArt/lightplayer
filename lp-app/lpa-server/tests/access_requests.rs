//! Who has access, on the board: `AccessList` / `AccessAdd` /
//! `AccessRemove` / `AccessSetSwitches` end to end through
//! `LpServer::tick_and_send`.
//!
//! The device store is merged here, by salt, so two browsers adding keys
//! never erase each other's; the answer never carries a key; and none of it
//! is answered below edit.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
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
    DeviceAccessFile, LoginMac, LoginOutcome, MAX_SECRETS_PER_FILE, SecretEntry, SecretKind, Tier,
    derive_login_key,
};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, LinkTrust, ServerTransport};
use lpc_wire::server::AccessEntryInfo;
use lpc_wire::{
    ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
};
use lpfs::{LpFs, LpFsMemory};

const USB: Link = Link::PRIMARY;
const BLE: Link = Link {
    id: LinkId::new(7),
    trust: LinkTrust::Untrusted,
};

/// A version-1 device store as the v1 writer wrote it: one edit password,
/// "mine" = `hunter2`, salt 9s, 3 iterations.
const V1_STORE: &str = "{\"version\":1,\"secrets\":[{\"label\":\"mine\",\"tier\":\"edit\",\
    \"salt\":\"CQkJCQkJCQkJCQkJCQkJCQ==\",\"iterations\":3,\
    \"k\":\"pZ2gub4bSR+JKUDoo7R8TI1TQxtWxa3rc2kPufJ978E=\"}],\"bleEnabled\":true,\"open\":false}";

#[test]
fn a_device_with_no_store_lists_bluetooth_on_locked_and_no_keys() {
    let mut rig = Rig::new(None);
    let list = rig.list(USB);
    assert!(list.ble_enabled);
    assert!(!list.open);
    assert!(list.entries.is_empty());
    assert!(
        !rig.store_exists(),
        "a list is a read: it must not create the store"
    );
}

#[test]
fn a_listed_entry_is_the_one_added() {
    let mut rig = Rig::new(None);
    let key = browser_key("Yona's MacBook", 1).with_added_at(1_790_000_000);
    let list = rig.add(USB, key.clone());
    assert_eq!(list.entries, vec![AccessEntryInfo::from(&key)]);
    assert_eq!(rig.list(USB).entries, list.entries);
    // The store was written, at v2, starting from fresh: Bluetooth on.
    let stored = rig.stored();
    assert_eq!(stored.secrets, vec![key]);
    assert!(stored.ble_enabled);
}

#[test]
fn two_browsers_adding_keys_keep_both() {
    let mut rig = Rig::new(None);
    rig.add(USB, browser_key("Chrome on Mac", 1));
    let list = rig.add(USB, browser_key("Safari on iPhone", 2));
    let labels: Vec<_> = list
        .entries
        .iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(labels, ["Chrome on Mac", "Safari on iPhone"]);
}

#[test]
fn adding_the_same_salt_replaces_the_entry() {
    let mut rig = Rig::new(None);
    rig.add(USB, browser_key("Chrome on Mac", 1));
    rig.add(USB, browser_key("Other", 2));
    let list = rig.add(USB, browser_key("Yona's MacBook", 1));
    let labels: Vec<_> = list
        .entries
        .iter()
        .map(|entry| entry.label.as_str())
        .collect();
    assert_eq!(labels, ["Yona's MacBook", "Other"]);
}

#[test]
fn remove_drops_by_salt_and_a_missing_salt_is_a_no_op() {
    let mut rig = Rig::new(None);
    rig.add(USB, browser_key("a", 1));
    rig.add(USB, browser_key("b", 2));
    let list = rig.remove(USB, [1; 16]);
    assert_eq!(list.entries.len(), 1);
    assert_eq!(list.entries[0].label, "b");
    let again = rig.remove(USB, [1; 16]);
    assert_eq!(again.entries, list.entries);
}

#[test]
fn the_seventeenth_key_is_refused_and_nothing_changes() {
    let mut rig = Rig::new(None);
    for salt in 1..=MAX_SECRETS_PER_FILE as u8 {
        rig.add(USB, browser_key("key", salt));
    }
    let before = rig.stored();
    match rig.request(
        USB,
        ClientRequest::AccessAdd {
            entry: browser_key("one too many", 0xff),
        },
    ) {
        WireServerMsgBody::Error { error } => {
            assert!(error.contains("at most 16"), "{error}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert_eq!(rig.stored(), before);
    // Replacing one is still fine at the cap.
    let list = rig.add(USB, browser_key("renamed", 16));
    assert_eq!(list.entries.len(), MAX_SECRETS_PER_FILE);
}

#[test]
fn switches_set_what_is_given_and_leave_the_rest() {
    let mut rig = Rig::new(None);
    let list = rig.switches(USB, Some(false), None);
    assert!(!list.ble_enabled);
    assert!(!list.open);
    let list = rig.switches(USB, None, Some(true));
    assert!(!list.ble_enabled, "an absent switch is left as it was");
    assert!(list.open);
    // `open` is live at once: an untrusted link now holds play.
    assert_eq!(rig.server.link_tier(BLE), Some(Tier::Play));
    rig.switches(USB, None, Some(false));
    assert_eq!(rig.server.link_tier(BLE), None);
}

#[test]
fn nothing_below_edit_is_answered() {
    let store = DeviceAccessFile {
        secrets: vec![SecretEntry::from_password(
            "camp",
            Tier::Play,
            b"camp",
            [3; 16],
            1,
        )],
        ..DeviceAccessFile::fresh()
    };
    let mut rig = Rig::new(Some(store.to_json().unwrap()));
    for request in access_requests() {
        assert!(matches!(
            rig.request(BLE, request.clone()),
            WireServerMsgBody::NotPermitted { needs: Tier::Edit }
        ));
    }
    assert!(matches!(
        rig.login(BLE, b"camp"),
        LoginOutcome::Granted {
            tier: Tier::Play,
            ..
        }
    ));
    for request in access_requests() {
        assert!(matches!(
            rig.request(BLE, request),
            WireServerMsgBody::NotPermitted { needs: Tier::Edit }
        ));
    }
    assert_eq!(rig.stored(), store, "a refused request changed the store");
}

#[test]
fn a_key_added_over_usb_unlocks_over_bluetooth() {
    let mut rig = Rig::new(None);
    rig.add(USB, browser_key("Yona's MacBook", 1));
    assert_eq!(rig.server.link_tier(BLE), None);
    assert!(matches!(
        rig.login(BLE, b"browser-secret"),
        LoginOutcome::Granted { tier: Tier::Edit, ref label } if label == "Yona's MacBook"
    ));
    // Unlocked with edit, the phone can see the list too.
    assert_eq!(rig.list(BLE).entries.len(), 1);
}

#[test]
fn no_reply_carries_a_key() {
    let mut rig = Rig::new(None);
    let key = browser_key("Yona's MacBook", 1);
    let k_base64 = key_as_the_wire_spells_it(&key);
    for request in [
        ClientRequest::AccessAdd { entry: key.clone() },
        ClientRequest::AccessList,
        ClientRequest::AccessRemove { salt: [9; 16] },
        ClientRequest::AccessSetSwitches {
            ble_enabled: Some(true),
            open: None,
        },
    ] {
        let reply = rig.request(USB, request);
        let json = lpc_wire::json::to_string(&reply).unwrap();
        assert!(json.contains("\"accessList\""), "{json}");
        assert!(!json.contains(&k_base64), "a key left the device: {json}");
        assert!(!json.contains("\"k\""), "{json}");
        assert!(!json.contains("iterations"), "{json}");
    }
}

#[test]
fn a_v1_store_lists_as_passwords_and_is_rewritten_as_v2_on_the_first_add() {
    let mut rig = Rig::new(Some(String::from(V1_STORE)));
    let list = rig.list(USB);
    assert_eq!(list.entries.len(), 1);
    assert_eq!(list.entries[0].label, "mine");
    assert_eq!(list.entries[0].kind, SecretKind::Password);
    assert_eq!(list.entries[0].added_at, None);
    assert_eq!(
        rig.raw_store(),
        V1_STORE,
        "a list does not rewrite the store"
    );

    rig.add(USB, browser_key("Yona's MacBook", 1));
    let raw = rig.raw_store();
    assert!(raw.starts_with("{\"version\":2,"), "{raw}");
    let stored = rig.stored();
    assert_eq!(stored.secrets.len(), 2);
    assert_eq!(stored.secrets[0].kind, SecretKind::Password);
    // The v1 entry still logs in after the rewrite.
    assert!(matches!(
        rig.login(BLE, b"hunter2"),
        LoginOutcome::Granted {
            tier: Tier::Edit,
            ..
        }
    ));
}

// --- the rig ----------------------------------------------------------------

/// What an `AccessList` answer carries.
struct Listed {
    ble_enabled: bool,
    open: bool,
    entries: Vec<AccessEntryInfo>,
}

struct Rig {
    server: LpServer,
    transport: LinkTransport,
    next_id: u64,
}

impl Rig {
    /// A server whose device store holds `store` bytes, or none at all.
    fn new(store: Option<String>) -> Self {
        let fs = LpFsMemory::new();
        if let Some(store) = store {
            fs.write_file(DeviceAccessFile::PATH.as_path(), store.as_bytes())
                .unwrap();
        }
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
        Self {
            server,
            transport: LinkTransport::default(),
            next_id: 100,
        }
    }

    fn request(&mut self, link: Link, request: ClientRequest) -> WireServerMsgBody {
        self.next_id += 1;
        let id = self.next_id;
        self.transport.sent.clear();
        block_on(self.server.tick_and_send(
            16,
            vec![Incoming::on(link, ClientMessage { id, msg: request })],
            &mut self.transport,
        ))
        .expect("tick");
        let (reply_link, reply) = self.transport.sent.pop().expect("a reply");
        assert!(self.transport.sent.is_empty(), "one reply per request");
        assert_eq!(reply_link, link.id);
        assert_eq!(reply.id, id);
        reply.msg
    }

    fn listed(&mut self, link: Link, request: ClientRequest) -> Listed {
        match self.request(link, request) {
            WireServerMsgBody::AccessList {
                ble_enabled,
                open,
                entries,
            } => Listed {
                ble_enabled,
                open,
                entries,
            },
            other => panic!("expected an access list, got {other:?}"),
        }
    }

    fn list(&mut self, link: Link) -> Listed {
        self.listed(link, ClientRequest::AccessList)
    }

    fn add(&mut self, link: Link, entry: SecretEntry) -> Listed {
        self.listed(link, ClientRequest::AccessAdd { entry })
    }

    fn remove(&mut self, link: Link, salt: [u8; 16]) -> Listed {
        self.listed(link, ClientRequest::AccessRemove { salt })
    }

    fn switches(&mut self, link: Link, ble_enabled: Option<bool>, open: Option<bool>) -> Listed {
        self.listed(link, ClientRequest::AccessSetSwitches { ble_enabled, open })
    }

    fn login(&mut self, link: Link, secret: &[u8]) -> LoginOutcome {
        let (nonce, offers) = match self.request(link, ClientRequest::LoginBegin) {
            WireServerMsgBody::LoginChallenge { nonce, offers } => (nonce, offers),
            other => panic!("expected a challenge, got {other:?}"),
        };
        let macs = offers
            .iter()
            .map(|offer| {
                LoginMac::compute(
                    &derive_login_key(secret, &offer.salt, offer.iterations),
                    &nonce,
                )
            })
            .collect();
        match self.request(link, ClientRequest::LoginAnswer { macs }) {
            WireServerMsgBody::LoginResult(outcome) => outcome,
            other => panic!("expected a login result, got {other:?}"),
        }
    }

    fn store_exists(&self) -> bool {
        self.server
            .base_fs()
            .file_exists(DeviceAccessFile::PATH.as_path())
            .unwrap()
    }

    fn raw_store(&self) -> String {
        let bytes = self
            .server
            .base_fs()
            .read_file(DeviceAccessFile::PATH.as_path())
            .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    fn stored(&self) -> DeviceAccessFile {
        DeviceAccessFile::from_json(self.raw_store().as_bytes()).unwrap()
    }
}

#[derive(Default)]
struct LinkTransport {
    sent: Vec<(LinkId, WireServerMessage)>,
}

impl ServerTransport for LinkTransport {
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
        vec![USB, BLE]
    }

    fn take_closed_links(&mut self) -> Vec<LinkId> {
        Vec::new()
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

// --- helpers ------------------------------------------------------------------

/// A generated browser key: 32 "random" bytes as the secret, the browser's
/// one salt, `iterations: 1`.
fn browser_key(label: &str, salt_byte: u8) -> SecretEntry {
    SecretEntry::from_password(label, Tier::Edit, b"browser-secret", [salt_byte; 16], 1)
        .with_kind(SecretKind::Browser)
}

fn access_requests() -> [ClientRequest; 4] {
    [
        ClientRequest::AccessList,
        ClientRequest::AccessAdd {
            entry: browser_key("intruder", 0x55),
        },
        ClientRequest::AccessRemove { salt: [3; 16] },
        ClientRequest::AccessSetSwitches {
            ble_enabled: Some(false),
            open: Some(true),
        },
    ]
}

/// The base64 `k` an entry serializes with, lifted out of its own JSON.
fn key_as_the_wire_spells_it(entry: &SecretEntry) -> String {
    let json = lpc_wire::json::to_string(entry).unwrap();
    let start = json.find("\"k\":\"").expect("an entry carries k") + 5;
    let end = start + json[start..].find('"').expect("closing quote");
    json[start..end].into()
}

/// Test entropy: a different fill every call, never a real RNG.
fn counting_entropy(buf: &mut [u8]) {
    static COUNTER: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
    let next = COUNTER
        .fetch_add(1, core::sync::atomic::Ordering::Relaxed)
        .wrapping_add(1);
    buf.fill(next);
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match Future::poll(Pin::as_mut(&mut future), &mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}

fn noop_waker() -> Waker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);

    unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) }
}
