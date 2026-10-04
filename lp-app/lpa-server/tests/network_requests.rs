//! Wi-Fi settings on the board: `NetworkStatus` / `NetworkSet` /
//! `NetworkForget` end to end through `LpServer::tick_and_send`, and the
//! network file's write-only gate on the trusted link.
//!
//! The board keeps the settings in `/.lp/network.json`; the answer is the
//! status, which never carries the password; nothing below edit is
//! answered; a board holding its files for an update refuses changes; and
//! no fs request on any link returns the file's bytes. The tier table over
//! every link state (keyed links included) is `access_gate.rs`.

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
use lpa_server::network_store::{HELD_BOARD_REFUSAL, NEW_NETWORK_NEEDS_PASSWORD, NO_NETWORK_SAVED};
use lpa_server::{LpGraphics, LpServer};
use lpc_access::{NetworkFile, OpenTo, Tier};
use lpc_model::{AsLpPath, AsLpPathBuf, FsVersion};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, LinkTrust, ServerTransport};
use lpc_wire::server::{FsRequest, FsResponse, NetworkStatus, StationState, WifiInfo};
use lpc_wire::{
    ClientMessage, ClientRequest, FsBootState, TransportError, WifiPassword, WireServerMessage,
    WireServerMsgBody,
};
use lpfs::LpFsMemory;

const USB: Link = Link::PRIMARY;
const BLE: Link = Link {
    id: LinkId::new(7),
    trust: LinkTrust::Untrusted,
};

const SSID: &str = "lp-walk-net";
const PASSWORD: &str = "correct-horse-42";

#[test]
fn a_fresh_board_has_no_network_and_says_it_cannot_join() {
    let mut rig = Rig::new();
    let status = rig.status(USB);
    assert_eq!(
        status,
        NetworkStatus {
            wifi: None,
            lan_only: false,
            station: StationState::Unsupported,
        }
    );
    assert!(!rig.file_exists(), "a status is a read: it creates no file");
}

#[test]
fn set_saves_the_network_and_answers_without_the_password() {
    let mut rig = Rig::new();
    let reply = rig.request(USB, set(Some(SSID), Some(PASSWORD), None, None));
    let json = lpc_wire::json::to_string(&reply).unwrap();
    assert!(
        !json.contains(PASSWORD),
        "the password left the board: {json}"
    );
    let WireServerMsgBody::NetworkStatus(status) = reply else {
        panic!("expected a status, got {json}");
    };
    assert_eq!(
        status.wifi,
        Some(WifiInfo {
            ssid: String::from(SSID),
            has_password: true,
            enabled: true,
        })
    );
    // The board holds the password itself, in its own file.
    let stored = rig.stored();
    let wifi = stored.wifi.expect("a saved network");
    assert_eq!(wifi.ssid, SSID);
    assert_eq!(wifi.password, PASSWORD);
    assert!(wifi.enabled);
    // And it reads back the same, again without it.
    assert_eq!(rig.status(USB), status);
}

#[test]
fn no_reply_carries_the_password() {
    let mut rig = Rig::new();
    for request in [
        set(Some(SSID), Some(PASSWORD), None, None),
        ClientRequest::NetworkStatus,
        set(None, None, Some(false), None),
        set(None, None, None, Some(true)),
        set(None, Some(PASSWORD), None, None),
        ClientRequest::NetworkForget,
    ] {
        let reply = rig.request(USB, request);
        let json = lpc_wire::json::to_string(&reply).unwrap();
        let shown = alloc::format!("{reply:?}");
        assert!(json.contains("\"networkStatus\""), "{json}");
        assert!(!json.contains(PASSWORD), "{json}");
        assert!(!shown.contains(PASSWORD), "{shown}");
    }
}

#[test]
fn partial_updates_change_only_what_is_given() {
    let mut rig = Rig::new();
    rig.set(USB, Some(SSID), Some(PASSWORD), None, None);

    let status = rig.set(USB, None, None, Some(false), None);
    assert!(!status.wifi.as_ref().unwrap().enabled);
    assert!(!status.lan_only);
    assert_eq!(rig.stored().wifi.unwrap().password, PASSWORD);

    let status = rig.set(USB, None, None, None, Some(true));
    assert!(status.lan_only);
    assert!(
        !status.wifi.as_ref().unwrap().enabled,
        "an absent field is left as it was"
    );

    // A new password for the same network, named again or not.
    rig.set(USB, None, Some("another-pass-99"), None, None);
    assert_eq!(rig.stored().wifi.unwrap().password, "another-pass-99");
    rig.set(USB, Some(SSID), Some("third-pass-77"), None, None);
    assert_eq!(rig.stored().wifi.unwrap().password, "third-pass-77");
    // Naming the saved network again without a password keeps it.
    rig.set(USB, Some(SSID), None, Some(true), None);
    let stored = rig.stored().wifi.unwrap();
    assert_eq!(stored.password, "third-pass-77");
    assert!(stored.enabled);
}

#[test]
fn a_new_network_without_a_password_is_refused_and_nothing_is_written() {
    let mut rig = Rig::new();
    rig.set(USB, Some(SSID), Some(PASSWORD), None, None);
    let before = rig.raw();
    assert_eq!(
        rig.error(USB, set(Some("neighbours-net"), None, None, None)),
        NEW_NETWORK_NEEDS_PASSWORD
    );
    assert_eq!(rig.raw(), before);
}

#[test]
fn an_empty_password_is_an_open_network() {
    let mut rig = Rig::new();
    let status = rig.set(USB, Some("cafe-open"), Some(""), None, None);
    assert_eq!(
        status.wifi,
        Some(WifiInfo {
            ssid: String::from("cafe-open"),
            has_password: false,
            enabled: true,
        })
    );
}

#[test]
fn every_broken_rule_is_refused_with_its_sentence_and_nothing_is_written() {
    let mut rig = Rig::new();
    rig.set(USB, Some(SSID), Some(PASSWORD), None, None);
    let before = rig.raw();
    let long_ssid = "a".repeat(33);
    let long_password = "x".repeat(64);
    for (request, words) in [
        (set(Some(""), Some(PASSWORD), None, None), "name is empty"),
        (
            set(Some(&long_ssid), Some(PASSWORD), None, None),
            "33 bytes",
        ),
        (set(None, Some("short"), None, None), "5 characters"),
        (set(None, Some(&long_password), None, None), "64 hex digits"),
        (
            set(None, Some("pässwörd-long"), None, None),
            "printable ASCII",
        ),
    ] {
        let error = rig.error(USB, request);
        assert!(error.contains(words), "{error}");
        assert_eq!(rig.raw(), before, "{error}");
    }
}

#[test]
fn enabled_or_password_with_no_network_saved_is_refused() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.error(USB, set(None, None, Some(true), None)),
        NO_NETWORK_SAVED
    );
    assert_eq!(
        rig.error(USB, set(None, Some(PASSWORD), None, None)),
        NO_NETWORK_SAVED
    );
    assert!(!rig.file_exists());
    // `lanOnly` alone is fine with nothing saved.
    let status = rig.set(USB, None, None, None, Some(true));
    assert!(status.lan_only);
    assert!(status.wifi.is_none());
}

#[test]
fn a_network_saved_switched_off_stays_off() {
    let mut rig = Rig::new();
    let status = rig.set(USB, Some(SSID), Some(PASSWORD), Some(false), None);
    assert!(!status.wifi.unwrap().enabled);
}

#[test]
fn forget_drops_the_network_and_keeps_lan_only() {
    let mut rig = Rig::new();
    rig.set(USB, Some(SSID), Some(PASSWORD), None, Some(true));
    let status = rig.forget(USB);
    assert!(status.wifi.is_none());
    assert!(status.lan_only);
    let raw = rig.raw();
    assert!(!raw.contains(PASSWORD), "{raw}");
    assert!(!raw.contains(SSID), "{raw}");
    // Forgetting with nothing saved is not an error.
    assert_eq!(rig.forget(USB), status);
}

#[test]
fn a_damaged_file_reads_as_no_network_until_the_next_set_replaces_it() {
    let mut rig = Rig::new();
    rig.write_raw("{\"version\":1,\"wifi\":");
    assert!(rig.status(USB).wifi.is_none());
    assert_eq!(
        rig.raw(),
        "{\"version\":1,\"wifi\":",
        "a read never rewrites"
    );
    rig.set(USB, Some(SSID), Some(PASSWORD), None, None);
    assert_eq!(rig.stored().wifi.unwrap().ssid, SSID);

    // A newer format is no network too, and is left alone by a status.
    rig.write_raw("{\"version\":2}");
    assert!(rig.status(USB).wifi.is_none());
    assert_eq!(rig.raw(), "{\"version\":2}");
}

#[test]
fn a_held_board_refuses_changes_and_answers_the_status() {
    let mut rig = Rig::new();
    rig.server.set_fs_boot_state(FsBootState::LegacyHeld);
    assert_eq!(
        rig.error(USB, set(Some(SSID), Some(PASSWORD), None, None)),
        HELD_BOARD_REFUSAL
    );
    assert_eq!(
        rig.error(USB, ClientRequest::NetworkForget),
        HELD_BOARD_REFUSAL
    );
    assert!(!rig.file_exists());
    assert!(rig.status(USB).wifi.is_none());
}

#[test]
fn the_station_probe_is_reported_verbatim() {
    fn joined() -> StationState {
        StationState::Joined {
            ip: String::from("192.168.1.40"),
            rssi: -61,
        }
    }
    let mut rig = Rig::new();
    assert_eq!(rig.status(USB).station, StationState::Unsupported);
    rig.server.set_station_probe(Some(joined));
    assert_eq!(rig.status(USB).station, joined());
    assert_eq!(
        rig.set(USB, Some(SSID), Some(PASSWORD), None, None).station,
        joined()
    );
    rig.server.set_station_probe(None);
    assert_eq!(rig.status(USB).station, StationState::Unsupported);
}

#[test]
fn play_and_no_tier_links_are_refused_and_change_nothing() {
    for open in [OpenTo::Nobody, OpenTo::Play] {
        let mut rig = Rig::new();
        rig.write_device_store(open);
        rig.set(USB, Some(SSID), Some(PASSWORD), None, None);
        let before = rig.raw();
        for request in [
            ClientRequest::NetworkStatus,
            set(Some("intruder-net"), Some("intruder-pass"), None, None),
            ClientRequest::NetworkForget,
        ] {
            assert!(
                matches!(
                    rig.request(BLE, request),
                    WireServerMsgBody::NotPermitted { needs: Tier::Edit }
                ),
                "{open:?}"
            );
        }
        assert_eq!(rig.raw(), before, "{open:?}: a refused request wrote");
    }
}

#[test]
fn an_edit_link_over_bluetooth_may_set_the_network() {
    // A fresh board is open at edit: anyone nearby holds edit (an accepted
    // exposure, as for everything else on an open board).
    let mut rig = Rig::new();
    assert_eq!(rig.server.link_tier(BLE), Some(Tier::Edit));
    let status = rig.set(BLE, Some(SSID), Some(PASSWORD), None, None);
    assert_eq!(status.wifi.unwrap().ssid, SSID);
}

/// The write-only gate, end to end on the TRUSTED link: a read is refused,
/// a listing may name the file, a changes walk skips it, and a hash over it
/// is refused. No reply carries a byte of the password.
#[test]
fn the_network_file_is_write_only_on_the_trusted_link() {
    let mut rig = Rig::new();
    rig.set(USB, Some(SSID), Some(PASSWORD), None, None);

    let mut replies = Vec::new();
    for path in [
        "/.lp/network.json",
        ".lp/network.json",
        "/.lp/./network.json",
        "/.lp/../.lp/network.json",
        "//.lp//network.json/",
        "/.LP/Network.JSON",
    ] {
        let reply = rig.request(
            USB,
            fs(FsRequest::Read {
                path: path.as_path_buf(),
            }),
        );
        match &reply {
            WireServerMsgBody::Filesystem(FsResponse::Read { data, error, .. }) => {
                assert_eq!(*data, None, "{path}: bytes left the device");
                assert!(error.is_some(), "{path}");
            }
            other => panic!("{path}: {other:?}"),
        }
        replies.push(reply);
    }

    let listed = rig.request(
        USB,
        fs(FsRequest::ListDir {
            path: "/.lp".as_path_buf(),
            recursive: false,
        }),
    );
    match &listed {
        WireServerMsgBody::Filesystem(FsResponse::ListDir { entries, .. }) => assert!(
            entries
                .iter()
                .any(|entry| entry.as_str().ends_with("network.json")),
            "a listing may name the file: {entries:?}"
        ),
        other => panic!("{other:?}"),
    }
    replies.push(listed);

    for prefix in ["/", "/.lp"] {
        let changes = rig.request(
            USB,
            fs(FsRequest::ChangesSince {
                prefix: prefix.as_path_buf(),
                since: FsVersion::new(0),
                cursor: None,
            }),
        );
        match &changes {
            WireServerMsgBody::Filesystem(FsResponse::Changes { entries, .. }) => {
                assert!(
                    !entries
                        .iter()
                        .any(|entry| entry.path.as_str().ends_with("network.json")),
                    "{prefix}: the file rode a changes walk"
                );
            }
            other => panic!("{prefix}: {other:?}"),
        }
        replies.push(changes);
    }

    // A hash rooted at `.lp` would cover the file: refused.
    let hash = rig.request(
        USB,
        fs(FsRequest::HashPackage {
            prefix: "/.lp".as_path_buf(),
        }),
    );
    match &hash {
        WireServerMsgBody::Filesystem(FsResponse::PackageHash { hash, error, .. }) => {
            assert!(error.is_some(), "hashed over the network file");
            assert!(hash.is_empty());
        }
        other => panic!("{other:?}"),
    }
    replies.push(hash);
    // A hash of the device root leaves the root's own `.lp/` out (the
    // package hash rules), so it never depends on the file: a new password
    // does not move it.
    let root_hash = |rig: &mut Rig| match rig.request(
        USB,
        fs(FsRequest::HashPackage {
            prefix: "/".as_path_buf(),
        }),
    ) {
        WireServerMsgBody::Filesystem(FsResponse::PackageHash { hash, .. }) => hash,
        other => panic!("{other:?}"),
    };
    let before = root_hash(&mut rig);
    rig.set(USB, None, Some("another-pass-99"), None, None);
    assert_eq!(
        root_hash(&mut rig),
        before,
        "the root hash covers the network file"
    );

    for reply in &replies {
        let json = lpc_wire::json::to_string(reply).unwrap();
        assert!(!json.contains(PASSWORD), "{json}");
    }
}

// --- the rig ----------------------------------------------------------------

struct Rig {
    server: LpServer,
    transport: LinkTransport,
    next_id: u64,
}

impl Rig {
    /// A server on an empty memory fs: no device store (open at edit), no
    /// network file.
    fn new() -> Self {
        let mut server = LpServer::new(
            Rc::new(RefCell::new(MemoryOutputProvider::new())),
            Box::new(LpFsMemory::new()),
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

    fn answered(&mut self, link: Link, request: ClientRequest) -> NetworkStatus {
        match self.request(link, request) {
            WireServerMsgBody::NetworkStatus(status) => status,
            other => panic!("expected a network status, got {other:?}"),
        }
    }

    fn error(&mut self, link: Link, request: ClientRequest) -> String {
        match self.request(link, request) {
            WireServerMsgBody::Error { error } => error,
            other => panic!("expected an error, got {other:?}"),
        }
    }

    fn status(&mut self, link: Link) -> NetworkStatus {
        self.answered(link, ClientRequest::NetworkStatus)
    }

    fn set(
        &mut self,
        link: Link,
        ssid: Option<&str>,
        password: Option<&str>,
        enabled: Option<bool>,
        lan_only: Option<bool>,
    ) -> NetworkStatus {
        self.answered(link, set(ssid, password, enabled, lan_only))
    }

    fn forget(&mut self, link: Link) -> NetworkStatus {
        self.answered(link, ClientRequest::NetworkForget)
    }

    fn file_exists(&self) -> bool {
        self.server
            .base_fs()
            .file_exists(NetworkFile::PATH.as_path())
            .unwrap()
    }

    fn raw(&self) -> String {
        let bytes = self
            .server
            .base_fs()
            .read_file(NetworkFile::PATH.as_path())
            .unwrap();
        String::from_utf8(bytes).unwrap()
    }

    fn write_raw(&mut self, text: &str) {
        self.server
            .base_fs()
            .write_file(NetworkFile::PATH.as_path(), text.as_bytes())
            .unwrap();
    }

    fn stored(&self) -> NetworkFile {
        NetworkFile::from_json(self.raw().as_bytes()).unwrap()
    }

    /// Open the device to `open` — what an untrusted link holds.
    fn write_device_store(&mut self, open: OpenTo) {
        self.request(
            USB,
            ClientRequest::AccessSetSwitches {
                ble_enabled: None,
                open: Some(open),
            },
        );
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

fn set(
    ssid: Option<&str>,
    password: Option<&str>,
    enabled: Option<bool>,
    lan_only: Option<bool>,
) -> ClientRequest {
    ClientRequest::NetworkSet {
        ssid: ssid.map(String::from),
        password: password.map(WifiPassword::new),
        enabled,
        lan_only,
    }
}

fn fs(request: FsRequest) -> ClientRequest {
    ClientRequest::Filesystem(request)
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
