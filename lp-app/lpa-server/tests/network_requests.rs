//! Wi-Fi settings on the board: `NetworkStatus` / `NetworkScan` /
//! `NetworkAdd` / `NetworkForget` / `NetworkSet` end to end through
//! `LpServer::tick_and_send`, and the network file's write-only gate on the
//! trusted link.
//!
//! The board keeps up to eight networks in `/.lp/network.json`; the answer
//! is the status, which never carries a password; nothing below edit is
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
use lpa_server::network_store::HELD_BOARD_REFUSAL;
use lpa_server::{LpGraphics, LpServer};
use lpc_access::{NetworkFile, OpenTo, Tier};
use lpc_model::{AsLpPath, AsLpPathBuf, FsVersion};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, LinkTrust, ServerTransport};
use lpc_wire::server::{
    FsRequest, FsResponse, HeardNetwork, LastAttempt, NetworkScan, NetworkStatus, SavedNetworkInfo,
    StationState,
};
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
const SECOND_SSID: &str = "lp-back-office";
const SECOND_PASSWORD: &str = "staple-battery-7";

#[test]
fn a_fresh_board_has_no_network_and_says_it_cannot_connect() {
    let mut rig = Rig::new();
    let status = rig.status(USB);
    assert_eq!(
        status,
        NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks: Vec::new(),
            station: StationState::Unsupported,
        }
    );
    assert!(!rig.file_exists(), "a status is a read: it creates no file");
}

#[test]
fn add_saves_the_network_and_answers_without_the_password() {
    let mut rig = Rig::new();
    let reply = rig.request(USB, add(SSID, PASSWORD));
    let json = lpc_wire::json::to_string(&reply).unwrap();
    assert!(
        !json.contains(PASSWORD),
        "the password left the board: {json}"
    );
    let WireServerMsgBody::NetworkStatus(status) = reply else {
        panic!("expected a status, got {json}");
    };
    assert_eq!(
        status.networks,
        [SavedNetworkInfo {
            ssid: String::from(SSID),
            has_password: true,
            hidden: false,
            last: None,
        }]
    );
    // The board holds the password itself, in its own file.
    let stored = rig.stored();
    assert_eq!(stored.networks.len(), 1);
    assert_eq!(stored.networks[0].ssid, SSID);
    assert_eq!(stored.networks[0].password, PASSWORD);
    // And it reads back the same, again without it.
    assert_eq!(rig.status(USB), status);
}

#[test]
fn no_reply_carries_a_password() {
    let mut rig = Rig::new();
    for request in [
        add(SSID, PASSWORD),
        add(SECOND_SSID, SECOND_PASSWORD),
        ClientRequest::NetworkStatus,
        ClientRequest::NetworkScan,
        switches(Some(false), None),
        switches(None, Some(false)),
        add(SSID, "another-pass-99"),
        forget(SECOND_SSID),
    ] {
        let reply = rig.request(USB, request);
        let json = lpc_wire::json::to_string(&reply).unwrap();
        let shown = alloc::format!("{reply:?}");
        assert!(
            json.contains("\"networkStatus\"") || json.contains("\"networkScan\""),
            "{json}"
        );
        for password in [PASSWORD, SECOND_PASSWORD, "another-pass-99"] {
            assert!(!json.contains(password), "{json}");
            assert!(!shown.contains(password), "{shown}");
        }
    }
}

#[test]
fn networks_are_listed_in_the_order_they_were_added() {
    let mut rig = Rig::new();
    rig.add(USB, SSID, PASSWORD);
    let status = rig.answered(
        USB,
        ClientRequest::NetworkAdd {
            ssid: String::from(SECOND_SSID),
            password: WifiPassword::new(""),
            hidden: Some(true),
        },
    );
    let names: Vec<&str> = status.networks.iter().map(|n| n.ssid.as_str()).collect();
    assert_eq!(names, [SSID, SECOND_SSID]);
    assert!(status.networks[0].has_password);
    assert!(!status.networks[1].has_password, "an open network");
    assert!(status.networks[1].hidden);
}

#[test]
fn adding_a_saved_name_again_changes_its_password_in_place() {
    let mut rig = Rig::new();
    rig.add(USB, SSID, PASSWORD);
    rig.add(USB, SECOND_SSID, SECOND_PASSWORD);
    let status = rig.add(USB, SSID, "another-pass-99");
    let names: Vec<&str> = status.networks.iter().map(|n| n.ssid.as_str()).collect();
    assert_eq!(names, [SSID, SECOND_SSID], "it keeps its place");
    let stored = rig.stored();
    assert_eq!(stored.networks.len(), 2);
    assert_eq!(stored.networks[0].password, "another-pass-99");
    assert_eq!(stored.networks[1].password, SECOND_PASSWORD);
}

#[test]
fn a_ninth_network_is_refused_and_nothing_is_written() {
    let mut rig = Rig::new();
    for n in 0..NetworkFile::MAX_NETWORKS {
        rig.add(USB, &alloc::format!("lp-net-{n}"), PASSWORD);
    }
    let before = rig.raw();
    let error = rig.error(USB, add("lp-net-9", PASSWORD));
    assert!(error.contains("tooManyNetworks"), "{error}");
    assert!(!error.contains(PASSWORD), "{error}");
    assert_eq!(rig.raw(), before);
    // With eight saved, a saved one's password still changes.
    let status = rig.add(USB, "lp-net-3", "another-pass-99");
    assert_eq!(status.networks.len(), 8);
}

#[test]
fn every_broken_rule_is_refused_with_its_code_and_nothing_is_written() {
    // The device's own reply is the rule's bare code (cheap on the
    // device); Studio turns it into words before a request is ever
    // sent, so this is the rare fallback path (`lp-cli`'s add, which does
    // no early check of its own) — see `NetworkFileError::words`.
    let mut rig = Rig::new();
    rig.add(USB, SSID, PASSWORD);
    let before = rig.raw();
    let long_ssid = "a".repeat(33);
    let long_password = "x".repeat(64);
    for (request, code) in [
        (add("", PASSWORD), "ssidEmpty"),
        (add(&long_ssid, PASSWORD), "ssidTooLong"),
        (add(SSID, "short"), "passwordTooShort"),
        (add(SSID, &long_password), "passwordNotHexKey"),
        (add(SSID, "pässwörd-long"), "passwordNotPrintable"),
    ] {
        let error = rig.error(USB, request);
        assert!(error.contains(code), "{error}");
        assert_eq!(rig.raw(), before, "{error}");
    }
}

#[test]
fn the_switches_change_only_what_is_given() {
    let mut rig = Rig::new();
    rig.add(USB, SSID, PASSWORD);
    let status = rig.answered(USB, switches(Some(false), None));
    assert!(!status.wifi);
    assert!(status.cloud_relay, "an absent switch is left as it was");
    let status = rig.answered(USB, switches(None, Some(false)));
    assert!(!status.wifi);
    assert!(!status.cloud_relay);
    assert_eq!(rig.stored().networks[0].password, PASSWORD);
    // The switches need no saved network.
    let mut empty = Rig::new();
    let status = empty.answered(USB, switches(None, Some(false)));
    assert!(!status.cloud_relay);
    assert!(status.networks.is_empty());
}

#[test]
fn forget_drops_one_network_and_keeps_the_rest_and_the_switches() {
    let mut rig = Rig::new();
    rig.add(USB, SSID, PASSWORD);
    rig.add(USB, SECOND_SSID, SECOND_PASSWORD);
    rig.answered(USB, switches(None, Some(false)));
    let status = rig.forget(USB, SSID);
    let names: Vec<&str> = status.networks.iter().map(|n| n.ssid.as_str()).collect();
    assert_eq!(names, [SECOND_SSID]);
    assert!(!status.cloud_relay, "the relay switch outlives the network");
    let raw = rig.raw();
    assert!(!raw.contains(PASSWORD), "{raw}");
    assert!(!raw.contains(SSID), "{raw}");
    assert!(raw.contains(SECOND_PASSWORD), "the other one stays");
    // Forgetting one that is not saved is not an error.
    assert_eq!(rig.forget(USB, SSID), status);
}

#[test]
fn a_damaged_file_reads_as_no_network_until_the_next_add_replaces_it() {
    let mut rig = Rig::new();
    rig.write_raw("{\"version\":1,\"networks\":");
    assert!(rig.status(USB).networks.is_empty());
    assert_eq!(
        rig.raw(),
        "{\"version\":1,\"networks\":",
        "a read never rewrites"
    );
    rig.add(USB, SSID, PASSWORD);
    assert_eq!(rig.stored().networks[0].ssid, SSID);

    // A newer format is no network too, and is left alone by a status.
    rig.write_raw("{\"version\":2}");
    assert!(rig.status(USB).networks.is_empty());
    assert_eq!(rig.raw(), "{\"version\":2}");
}

#[test]
fn a_held_board_refuses_changes_and_answers_the_status() {
    let mut rig = Rig::new();
    rig.server.set_fs_boot_state(FsBootState::LegacyHeld);
    for request in [
        add(SSID, PASSWORD),
        forget(SSID),
        switches(Some(false), None),
    ] {
        assert_eq!(rig.error(USB, request), HELD_BOARD_REFUSAL);
    }
    assert!(!rig.file_exists());
    assert!(rig.status(USB).networks.is_empty());
}

#[test]
fn the_station_probe_is_reported_verbatim() {
    fn connected() -> StationState {
        StationState::Connected {
            ssid: String::from(SSID),
            ip: String::from("192.168.1.40"),
            rssi: -61,
            host: String::from("lp-8e30.local"),
        }
    }
    let mut rig = Rig::new();
    assert_eq!(rig.status(USB).station, StationState::Unsupported);
    rig.server.set_station_probe(Some(connected));
    assert_eq!(rig.status(USB).station, connected());
    assert_eq!(rig.add(USB, SSID, PASSWORD).station, connected());
    rig.server.set_station_probe(None);
    assert_eq!(rig.status(USB).station, StationState::Unsupported);
}

/// The station's last attempt at each saved network comes from its probe,
/// by name; without one `last` stays absent.
#[test]
fn each_networks_last_attempt_comes_from_the_probe() {
    fn last(ssid: &str) -> Option<LastAttempt> {
        (ssid == SSID).then_some(LastAttempt::WrongPassword)
    }
    let mut rig = Rig::new();
    rig.add(USB, SSID, PASSWORD);
    rig.add(USB, SECOND_SSID, SECOND_PASSWORD);
    assert!(rig.status(USB).networks.iter().all(|n| n.last.is_none()));
    rig.server.set_last_attempt_probe(Some(last));
    let status = rig.status(USB);
    assert_eq!(
        status.network(SSID).unwrap().last,
        Some(LastAttempt::WrongPassword)
    );
    assert_eq!(status.network(SECOND_SSID).unwrap().last, None);
}

/// Every change the board answers with a status hands the station the file
/// as it now stands; a read, a scan and a refused change do not.
#[test]
fn a_change_tells_the_station_and_a_read_does_not() {
    use core::sync::atomic::{AtomicU32, Ordering};
    static TOLD: AtomicU32 = AtomicU32::new(0);
    static SAVED: AtomicU32 = AtomicU32::new(0);
    fn notice(file: &NetworkFile) {
        TOLD.fetch_add(1, Ordering::Relaxed);
        SAVED.store(file.networks.len() as u32, Ordering::Relaxed);
    }
    let mut rig = Rig::new();
    rig.server.set_network_changed(Some(notice));
    rig.status(USB);
    rig.request(USB, ClientRequest::NetworkScan);
    assert_eq!(TOLD.load(Ordering::Relaxed), 0, "reads tell nothing");
    rig.add(USB, SSID, PASSWORD);
    assert_eq!(TOLD.load(Ordering::Relaxed), 1);
    assert_eq!(SAVED.load(Ordering::Relaxed), 1, "the file as written");
    rig.request(USB, switches(Some(false), None));
    rig.request(USB, forget(SSID));
    assert_eq!(TOLD.load(Ordering::Relaxed), 3);
    rig.error(USB, add(SSID, "short"));
    assert_eq!(
        TOLD.load(Ordering::Relaxed),
        3,
        "a refused change tells nothing"
    );
}

/// No M5 image scans: the answer says so, never an empty list. A probe's
/// answer passes through.
#[test]
fn a_scan_is_unsupported_without_a_probe() {
    fn heard() -> NetworkScan {
        NetworkScan::Heard(vec![HeardNetwork {
            ssid: String::from(SSID),
            rssi: -48,
            secure: true,
        }])
    }
    let mut rig = Rig::new();
    assert!(matches!(
        rig.request(USB, ClientRequest::NetworkScan),
        WireServerMsgBody::NetworkScan(NetworkScan::Unsupported)
    ));
    rig.server.set_scan_probe(Some(heard));
    match rig.request(USB, ClientRequest::NetworkScan) {
        WireServerMsgBody::NetworkScan(scan) => assert_eq!(scan, heard()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn play_and_no_tier_links_are_refused_and_change_nothing() {
    for open in [OpenTo::Nobody, OpenTo::Play] {
        let mut rig = Rig::new();
        rig.write_device_store(open);
        rig.add(USB, SSID, PASSWORD);
        let before = rig.raw();
        for request in [
            ClientRequest::NetworkStatus,
            ClientRequest::NetworkScan,
            add("intruder-net", "intruder-pass"),
            forget(SSID),
            switches(Some(false), Some(false)),
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
fn an_edit_link_over_bluetooth_may_add_a_network() {
    // A fresh board is open at edit: anyone nearby holds edit (an accepted
    // exposure, as for everything else on an open board).
    let mut rig = Rig::new();
    assert_eq!(rig.server.link_tier(BLE), Some(Tier::Edit));
    let status = rig.add(BLE, SSID, PASSWORD);
    assert_eq!(status.networks[0].ssid, SSID);
}

/// The write-only gate, end to end on the TRUSTED link: a read is refused,
/// a listing may name the file, a changes walk skips it, and a hash over it
/// is refused. No reply carries a byte of any password.
#[test]
fn the_network_file_is_write_only_on_the_trusted_link() {
    let mut rig = Rig::new();
    rig.add(USB, SSID, PASSWORD);
    rig.add(USB, SECOND_SSID, SECOND_PASSWORD);

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
    rig.add(USB, SSID, "another-pass-99");
    assert_eq!(
        root_hash(&mut rig),
        before,
        "the root hash covers the network file"
    );

    for reply in &replies {
        let json = lpc_wire::json::to_string(reply).unwrap();
        assert!(!json.contains(PASSWORD), "{json}");
        assert!(!json.contains(SECOND_PASSWORD), "{json}");
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

    fn add(&mut self, link: Link, ssid: &str, password: &str) -> NetworkStatus {
        self.answered(link, add(ssid, password))
    }

    fn forget(&mut self, link: Link, ssid: &str) -> NetworkStatus {
        self.answered(link, forget(ssid))
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

fn add(ssid: &str, password: &str) -> ClientRequest {
    ClientRequest::NetworkAdd {
        ssid: String::from(ssid),
        password: WifiPassword::new(password),
        hidden: None,
    }
}

fn forget(ssid: &str) -> ClientRequest {
    ClientRequest::NetworkForget {
        ssid: String::from(ssid),
    }
}

fn switches(wifi: Option<bool>, cloud_relay: Option<bool>) -> ClientRequest {
    ClientRequest::NetworkSet { wifi, cloud_relay }
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
