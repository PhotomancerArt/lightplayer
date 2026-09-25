//! The access gate, end to end through `LpServer::tick_and_send`: the tier
//! table over every request × every link state, the login, and two links
//! sharing one server.
//!
//! "The board enforces tiers, not Studio": nothing here trusts a client.
//! Every refusal is asserted as a REPLY (`NotPermitted`), never as silence.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{HeartbeatStatus, LpGraphics, LpServer, Required};
use lpc_access::{
    CHALLENGE_TTL_MS, DeviceAccessFile, LoginMac, LoginOutcome, ProjectAccessFile, SecretEntry,
    Tier, derive_login_key,
};
use lpc_model::{AsLpPath, AsLpPathBuf, LpValue, NodeAttachSite, NodeId};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::{Incoming, Link, LinkId, LinkTrust, ServerTransport};
use lpc_wire::server::{FsRequest, FsResponse, SampleStats};
use lpc_wire::{
    ClientMessage, ClientRequest, HelloAuth, ProjectReadRequest, TransportError,
    WireCreateNodeRequest, WireNodeCommand, WireOverlayCommitRequest, WireOverlayMutationRequest,
    WireOverlayReadRequest, WirePanelAutoSaveRequest, WirePanelClearRequest, WirePanelWriteRequest,
    WireProjectCommand, WireProjectHandle, WireProjectInventoryReadRequest, WireRemoveNodeRequest,
    WireScopeRef, WireServerMessage, WireServerMsgBody,
};
use lpfs::{LpFs, LpFsMemory};

const PLAY_PASSWORD: &[u8] = b"s'mores";
const EDIT_PASSWORD: &[u8] = b"hunter2";

const USB: Link = Link::PRIMARY;
const BLE_A: Link = Link {
    id: LinkId::new(7),
    trust: LinkTrust::Untrusted,
};
const BLE_B: Link = Link {
    id: LinkId::new(8),
    trust: LinkTrust::Untrusted,
};

/// THE TABLE. Every `ClientRequest` variant — `Filesystem` expanded to every
/// `FsRequest` variant inside and outside the projects directory,
/// `ProjectCommand` to every `WireProjectCommand` variant — against every
/// link state: answered, or refused `NotPermitted` naming the tier it needs.
///
/// The request list is checked against the variant lists serde derives for
/// each enum, so a new wire variant fails this test until it joins the
/// table (and fails `classify` to compile until it is classified).
#[test]
fn every_request_against_every_link_state() {
    let rows = table_rows();
    assert_covers_every_variant(&rows);

    for state in LinkState::ALL {
        let mut rig = Rig::for_state(state);
        let link = state.link();
        for row in &rows {
            let reply = rig.request(link, row.request.clone());
            let should_answer = row.needs.permits(state.tier());
            match (&reply, should_answer) {
                (WireServerMsgBody::NotPermitted { needs }, false) => {
                    assert_eq!(
                        Some(*needs),
                        row.needs.needs(),
                        "{state:?} / {}: refused naming the wrong tier",
                        row.label
                    );
                }
                (WireServerMsgBody::NotPermitted { .. }, true) => {
                    panic!("{state:?} / {}: refused but should be answered", row.label)
                }
                (_, false) => panic!(
                    "{state:?} / {}: answered ({}) but needs {:?}",
                    row.label,
                    body_name(&reply),
                    row.needs
                ),
                (_, true) => {}
            }
            if let WireServerMsgBody::Hello(hello) = &reply {
                assert_eq!(
                    hello.auth,
                    HelloAuth {
                        required: state != LinkState::Trusted,
                        granted: state.tier(),
                    },
                    "{state:?}: the hello's auth"
                );
            }
        }
    }
}

/// The fs gate is not a tier: the trusted USB link — edit, the recovery
/// path — is refused an access file's bytes exactly like everyone else.
#[test]
fn a_read_of_an_access_file_is_refused_on_the_trusted_link_too() {
    let mut rig = Rig::for_state(LinkState::Trusted);
    rig.write_sidecar("/projects/demo");
    for path in ["/.lp/access.json", "/projects/demo/.lp/access.json"] {
        let reply = rig.request(
            USB,
            ClientRequest::Filesystem(FsRequest::Read {
                path: path.as_path_buf(),
            }),
        );
        match reply {
            WireServerMsgBody::Filesystem(FsResponse::Read { data, error, .. }) => {
                assert_eq!(data, None, "{path}: bytes left the device");
                assert!(error.is_some(), "{path}");
            }
            other => panic!("{path}: {other:?}"),
        }
    }
}

#[test]
fn a_good_password_grants_its_tier_and_names_its_label() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    assert_eq!(
        rig.login(BLE_A, PLAY_PASSWORD),
        LoginOutcome::Granted {
            tier: Tier::Play,
            label: "camp".to_string()
        }
    );
    assert_eq!(rig.server.link_tier(BLE_A), Some(Tier::Play));
}

#[test]
fn with_a_play_and_an_edit_secret_the_edit_password_grants_edit() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    assert_eq!(
        rig.login(BLE_A, EDIT_PASSWORD),
        LoginOutcome::Granted {
            tier: Tier::Edit,
            label: "mine".to_string()
        }
    );
}

/// The device store and a loaded project's sidecar are both installed: a
/// password that only the sidecar holds logs in.
#[test]
fn the_loaded_projects_sidecar_is_installed_too() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    rig.write_sidecar("/projects/demo");
    rig.load_demo_project();
    assert_eq!(
        rig.login(BLE_A, b"campfire"),
        LoginOutcome::Granted {
            tier: Tier::Play,
            label: "project camp".to_string()
        }
    );
}

#[test]
fn a_bad_password_is_refused_with_backoff() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    for attempt in 1..=3 {
        assert_eq!(
            rig.login(BLE_A, b"wrong"),
            LoginOutcome::Refused { retry_after_ms: 0 },
            "attempt {attempt} is free"
        );
    }
    assert_eq!(
        rig.login(BLE_A, b"wrong"),
        LoginOutcome::Refused {
            retry_after_ms: 2_000
        }
    );
    // In backoff, even the right password cannot begin.
    assert!(matches!(
        rig.request(BLE_A, ClientRequest::LoginBegin),
        WireServerMsgBody::LoginResult(LoginOutcome::Refused {
            retry_after_ms: 1_984
        })
    ));
    assert_eq!(rig.server.link_tier(BLE_A), None);
}

#[test]
fn a_replayed_answer_is_refused() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    let (nonce, offers) = rig.begin(BLE_A);
    let macs = answer(PLAY_PASSWORD, &nonce, &offers);
    assert!(matches!(
        rig.answer(BLE_A, macs.clone()),
        LoginOutcome::Granted { .. }
    ));
    // Same MACs, same link: the challenge was single-use.
    let mut replay = Rig::for_state(LinkState::UntrustedNone);
    assert_eq!(
        replay.answer(BLE_A, macs.clone()),
        LoginOutcome::Refused { retry_after_ms: 0 }
    );
    assert_eq!(
        rig.answer(BLE_A, macs),
        LoginOutcome::Refused { retry_after_ms: 0 }
    );
}

#[test]
fn an_expired_challenge_is_refused() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    let (nonce, offers) = rig.begin(BLE_A);
    rig.idle(CHALLENGE_TTL_MS as u32);
    assert_eq!(
        rig.answer(BLE_A, answer(EDIT_PASSWORD, &nonce, &offers)),
        LoginOutcome::Refused { retry_after_ms: 0 }
    );
    assert_eq!(rig.server.link_tier(BLE_A), None);
}

#[test]
fn a_second_login_while_one_is_outstanding_is_refused() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    let (nonce, offers) = rig.begin(BLE_A);
    // Another link — and the same one — must wait.
    for link in [BLE_B, BLE_A] {
        match rig.request(link, ClientRequest::LoginBegin) {
            WireServerMsgBody::LoginResult(LoginOutcome::Refused { retry_after_ms }) => {
                assert!(retry_after_ms > 0 && retry_after_ms <= CHALLENGE_TTL_MS);
            }
            other => panic!("{link:?}: {other:?}"),
        }
    }
    // B cannot answer A's challenge, even with the right key…
    assert!(matches!(
        rig.answer(BLE_B, answer(EDIT_PASSWORD, &nonce, &offers)),
        LoginOutcome::Refused { .. }
    ));
    // …and that did not burn it: A still can.
    assert!(matches!(
        rig.answer(BLE_A, answer(EDIT_PASSWORD, &nonce, &offers)),
        LoginOutcome::Granted { .. }
    ));
    assert_eq!(rig.server.link_tier(BLE_B), None);
}

/// Backoff belongs to the device: closing the link and connecting again
/// (a new link id, as a multi-link transport mints) waits the same.
#[test]
fn backoff_survives_a_link_closing_and_reopening() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    for _ in 0..4 {
        rig.login(BLE_A, b"wrong");
    }
    rig.close(BLE_A);
    let reconnected = Link {
        id: LinkId::new(9),
        trust: LinkTrust::Untrusted,
    };
    match rig.request(reconnected, ClientRequest::LoginBegin) {
        WireServerMsgBody::LoginResult(LoginOutcome::Refused { retry_after_ms }) => {
            assert!(retry_after_ms > 0, "the backoff was reset by a reconnect");
        }
        other => panic!("{other:?}"),
    }
}

/// A grant belongs to the link: once it closes, a new connection starts
/// with nothing, and a closed link's outstanding challenge is freed.
#[test]
fn a_grant_and_a_challenge_end_with_their_link() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    rig.login(BLE_A, EDIT_PASSWORD);
    let _ = rig.begin(BLE_B);
    rig.close(BLE_A);
    rig.close(BLE_B);
    assert_eq!(rig.server.link_tier(BLE_A), None);
    // B's challenge went with B: a new login can begin at once.
    assert!(matches!(
        rig.request(BLE_A, ClientRequest::LoginBegin),
        WireServerMsgBody::LoginChallenge { .. }
    ));
}

#[test]
fn a_server_without_entropy_refuses_to_log_in() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    rig.server.set_entropy_source(None);
    assert!(matches!(
        rig.request(BLE_A, ClientRequest::LoginBegin),
        WireServerMsgBody::Error { .. }
    ));
}

/// Two radio links on one server: A logs in, B does not, and B is still
/// refused; every reply goes back on the link that asked.
#[test]
fn two_links_one_logged_in() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    rig.login(BLE_A, EDIT_PASSWORD);

    rig.transport.sent.clear();
    let messages = vec![
        incoming(BLE_A, 31, ClientRequest::ListLoadedProjects),
        incoming(BLE_B, 32, ClientRequest::ListLoadedProjects),
        incoming(USB, 33, ClientRequest::ListLoadedProjects),
    ];
    block_on(rig.server.tick_and_send(16, messages, &mut rig.transport)).expect("tick");

    let replies: Vec<(LinkId, u64, &'static str)> = rig
        .transport
        .sent
        .iter()
        .map(|(link, msg)| (*link, msg.id, body_name(&msg.msg)))
        .collect();
    assert_eq!(
        replies,
        vec![
            (BLE_A.id, 31, "ListLoadedProjects"),
            (BLE_B.id, 32, "NotPermitted"),
            (USB.id, 33, "ListLoadedProjects"),
        ]
    );
}

/// The unsolicited frames go to every link, each built for its link: the
/// hello's `auth` differs, and a link that holds no tier gets a heartbeat
/// that says "alive" and nothing the hello does not.
#[test]
fn hello_and_heartbeat_go_to_every_link_built_for_each() {
    let mut rig = Rig::for_state(LinkState::UntrustedNone);
    rig.login(BLE_A, PLAY_PASSWORD);
    rig.load_demo_project();

    assert_eq!(
        rig.server.hello_for_link(BLE_A).auth,
        HelloAuth {
            required: true,
            granted: Some(Tier::Play)
        }
    );
    assert_eq!(
        rig.server.hello_for_link(BLE_B).auth,
        HelloAuth {
            required: true,
            granted: None
        }
    );
    assert_eq!(rig.server.hello_for_link(USB).auth, HelloAuth::TRUSTED);

    let heartbeats = rig.server.heartbeats(&[USB, BLE_A, BLE_B], status());
    let links: Vec<LinkId> = heartbeats.iter().map(|(link, _)| *link).collect();
    assert_eq!(links, vec![USB.id, BLE_A.id, BLE_B.id]);
    for (link, message) in &heartbeats {
        let WireServerMsgBody::Heartbeat {
            uptime_ms,
            loaded_projects,
            memory,
            ..
        } = &message.msg
        else {
            panic!("{link:?}: not a heartbeat");
        };
        if *link == BLE_B.id {
            assert_eq!(*uptime_ms, 0);
            assert!(loaded_projects.is_empty());
            assert!(memory.is_none());
        } else {
            assert_eq!(*uptime_ms, 1234, "{link:?}");
            assert_eq!(loaded_projects.len(), 1, "{link:?}");
        }
    }
}

/// An "open" device gives an untrusted link play without a login — and
/// never edit.
#[test]
fn open_grants_play_and_never_edit() {
    let mut rig = Rig::for_state(LinkState::UntrustedOpen);
    assert!(matches!(
        rig.request(BLE_A, ClientRequest::ListLoadedProjects),
        WireServerMsgBody::ListLoadedProjects { .. }
    ));
    assert!(matches!(
        rig.request(BLE_A, ClientRequest::StopAllProjects),
        WireServerMsgBody::NotPermitted { needs: Tier::Edit }
    ));
}

/// The `open` flag is re-read after an fs write changes the device store.
#[test]
fn locking_the_device_over_usb_takes_open_play_away() {
    let mut rig = Rig::for_state(LinkState::UntrustedOpen);
    assert_eq!(rig.server.link_tier(BLE_A), Some(Tier::Play));
    let locked = DeviceAccessFile {
        open: false,
        ..rig.store.clone()
    };
    let reply = rig.request(
        USB,
        ClientRequest::Filesystem(FsRequest::Write {
            path: DeviceAccessFile::PATH.as_path_buf(),
            data: locked.to_json().unwrap().into_bytes(),
        }),
    );
    assert!(matches!(
        reply,
        WireServerMsgBody::Filesystem(FsResponse::Write { error: None, .. })
    ));
    assert_eq!(rig.server.link_tier(BLE_A), None);
}

// --- the table's rows -------------------------------------------------------

struct Row {
    label: String,
    request: ClientRequest,
    needs: Required,
}

/// The table, restated independently of `classify`: what each request needs.
fn table_rows() -> Vec<Row> {
    let handle = WireProjectHandle::new(1);
    let mut rows = vec![
        row("hello", ClientRequest::Hello, Required::Public),
        row("loginBegin", ClientRequest::LoginBegin, Required::Public),
        row(
            "loginAnswer",
            ClientRequest::LoginAnswer { macs: vec![] },
            Required::Public,
        ),
        // How this link's replies are written, not what they say (plan
        // `lp-json-pack`): asked before a login, so public.
        row(
            "setEncoding",
            ClientRequest::SetEncoding {
                encoding: lpc_wire::WireEncoding::Packed,
                dictionary: lpc_wire::WIRE_DICTIONARY_FINGERPRINT,
            },
            Required::Public,
        ),
        row(
            "projectRead",
            ClientRequest::ProjectRead {
                handle,
                request: ProjectReadRequest::default_debug(None),
            },
            Required::Play,
        ),
        row(
            "listAvailableProjects",
            ClientRequest::ListAvailableProjects,
            Required::Play,
        ),
        row(
            "listLoadedProjects",
            ClientRequest::ListLoadedProjects,
            Required::Play,
        ),
        row(
            "loadProject",
            ClientRequest::LoadProject {
                path: "/projects/absent".to_string(),
            },
            Required::Edit,
        ),
        row(
            "unloadProject",
            ClientRequest::UnloadProject { handle },
            Required::Edit,
        ),
        row(
            "stopAllProjects",
            ClientRequest::StopAllProjects,
            Required::Edit,
        ),
        row(
            "setLogLevel",
            ClientRequest::SetLogLevel {
                level: log_level_unchanged(),
            },
            Required::Edit,
        ),
        row("reboot", ClientRequest::Reboot, Required::Edit),
        row("clearFaults", ClientRequest::ClearFaults, Required::Edit),
        // Who has access: edit only. Harmless to the rows after them — the
        // add brings a salt no other row uses, the remove names a salt that
        // is not there, and the switches row changes neither switch.
        row("accessList", ClientRequest::AccessList, Required::Edit),
        row(
            "accessAdd",
            ClientRequest::AccessAdd {
                entry: secret("table row", Tier::Play, b"table", 0xa0),
            },
            Required::Edit,
        ),
        row(
            "accessRemove",
            ClientRequest::AccessRemove { salt: [0xa1; 16] },
            Required::Edit,
        ),
        row(
            "accessSetSwitches",
            ClientRequest::AccessSetSwitches {
                ble_enabled: None,
                open: None,
            },
            Required::Edit,
        ),
    ];

    for (command, needs) in project_commands() {
        let name = variant_name(&command);
        rows.push(row(
            &alloc::format!("projectCommand/{name}"),
            ClientRequest::ProjectCommand { handle, command },
            needs,
        ));
    }

    // Reads: play inside the projects directory, edit outside it.
    // Mutations: edit everywhere.
    for (dir, read_needs) in [("/projects/demo", Required::Play), ("/.lp", Required::Edit)] {
        for request in fs_requests(dir) {
            let is_read = matches!(
                request,
                FsRequest::Read { .. }
                    | FsRequest::ListDir { .. }
                    | FsRequest::ChangesSince { .. }
                    | FsRequest::HashPackage { .. }
            );
            let name = variant_name(&request);
            rows.push(row(
                &alloc::format!("filesystem/{name} {dir}"),
                ClientRequest::Filesystem(request),
                if is_read { read_needs } else { Required::Edit },
            ));
        }
    }
    rows
}

fn project_commands() -> Vec<(WireProjectCommand, Required)> {
    let owner = NodeId::new(1);
    vec![
        (
            WireProjectCommand::PanelWrite {
                request: WirePanelWriteRequest {
                    scope: WireScopeRef::Module { owner },
                    channel: "time".to_string(),
                    value: LpValue::F32(0.5),
                    ttl_ms: None,
                },
            },
            Required::Play,
        ),
        (
            WireProjectCommand::PanelClear {
                request: WirePanelClearRequest::All,
            },
            Required::Play,
        ),
        (
            WireProjectCommand::ReadOverlay {
                request: WireOverlayReadRequest,
            },
            Required::Play,
        ),
        (
            WireProjectCommand::MutateOverlay {
                request: WireOverlayMutationRequest::new(
                    lpc_model::project::overlay_mutation::MutationCmdBatch::new(vec![]),
                ),
            },
            Required::Edit,
        ),
        (
            WireProjectCommand::CommitOverlay {
                request: WireOverlayCommitRequest,
            },
            Required::Edit,
        ),
        (
            WireProjectCommand::ReadInventory {
                request: WireProjectInventoryReadRequest,
            },
            Required::Play,
        ),
        (
            WireProjectCommand::CreateNode {
                request: WireCreateNodeRequest::new(
                    "./x.json".as_path_buf(),
                    vec![],
                    vec![],
                    NodeAttachSite::ProjectNodes {
                        key: "x".to_string(),
                    },
                ),
            },
            Required::Edit,
        ),
        (
            WireProjectCommand::RemoveNode {
                request: WireRemoveNodeRequest::new(NodeAttachSite::ProjectNodes {
                    key: "x".to_string(),
                }),
            },
            Required::Edit,
        ),
        (
            WireProjectCommand::NodeCommand {
                node: owner,
                command: WireNodeCommand::PlaylistActivateEntry { entry: 0 },
            },
            Required::Edit,
        ),
        (
            WireProjectCommand::PanelAutoSave {
                request: WirePanelAutoSaveRequest { enabled: true },
            },
            Required::Edit,
        ),
    ]
}

fn fs_requests(dir: &str) -> Vec<FsRequest> {
    let file = alloc::format!("{dir}/table.txt").as_path_buf();
    vec![
        FsRequest::Read { path: file.clone() },
        FsRequest::ListDir {
            path: dir.as_path_buf(),
            recursive: true,
        },
        FsRequest::ChangesSince {
            prefix: dir.as_path_buf(),
            since: lpc_model::FsVersion::new(0),
            cursor: None,
        },
        FsRequest::HashPackage {
            prefix: dir.as_path_buf(),
        },
        FsRequest::Write {
            path: file.clone(),
            data: b"x".to_vec(),
        },
        FsRequest::WriteChunk {
            path: file.clone(),
            offset: 0,
            data: b"x".to_vec(),
        },
        FsRequest::DeleteFile { path: file },
        FsRequest::DeleteDir {
            path: alloc::format!("{dir}/scratch").as_path_buf(),
        },
    ]
}

/// The table must name every variant serde knows for the three request
/// enums — derived from the types, not typed by hand.
fn assert_covers_every_variant(rows: &[Row]) {
    let mut client: Vec<String> = rows.iter().map(|row| variant_name(&row.request)).collect();
    client.sort();
    client.dedup();
    assert_eq!(
        client,
        sorted(serde_variants::<ClientRequest>()),
        "ClientRequest"
    );

    let mut commands: Vec<String> = project_commands()
        .iter()
        .map(|(command, _)| variant_name(command))
        .collect();
    commands.sort();
    assert_eq!(
        commands,
        sorted(serde_variants::<WireProjectCommand>()),
        "WireProjectCommand"
    );

    let mut fs: Vec<String> = fs_requests("/projects/demo")
        .iter()
        .map(variant_name)
        .collect();
    fs.sort();
    assert_eq!(fs, sorted(serde_variants::<FsRequest>()), "FsRequest");
}

// --- the rig ----------------------------------------------------------------

/// A link's standing, as the table enumerates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkState {
    Trusted,
    UntrustedNone,
    UntrustedOpen,
    UntrustedPlay,
    UntrustedEdit,
}

impl LinkState {
    const ALL: [LinkState; 5] = [
        LinkState::Trusted,
        LinkState::UntrustedNone,
        LinkState::UntrustedOpen,
        LinkState::UntrustedPlay,
        LinkState::UntrustedEdit,
    ];

    fn link(self) -> Link {
        match self {
            LinkState::Trusted => USB,
            _ => BLE_A,
        }
    }

    fn tier(self) -> Option<Tier> {
        match self {
            LinkState::Trusted | LinkState::UntrustedEdit => Some(Tier::Edit),
            LinkState::UntrustedOpen | LinkState::UntrustedPlay => Some(Tier::Play),
            LinkState::UntrustedNone => None,
        }
    }
}

struct Rig {
    server: LpServer,
    transport: LinkTransport,
    store: DeviceAccessFile,
    next_id: u64,
}

impl Rig {
    /// A server with a device store holding a play secret ("camp") and an
    /// edit secret ("mine"), and `state`'s link brought to its standing.
    fn for_state(state: LinkState) -> Self {
        let store = DeviceAccessFile {
            version: DeviceAccessFile::VERSION,
            secrets: vec![
                secret("camp", Tier::Play, PLAY_PASSWORD, 1),
                secret("mine", Tier::Edit, EDIT_PASSWORD, 2),
            ],
            ble_enabled: true,
            open: state == LinkState::UntrustedOpen,
        };
        let fs = LpFsMemory::new();
        fs.write_file(
            DeviceAccessFile::PATH.as_path(),
            store.to_json().unwrap().as_bytes(),
        )
        .unwrap();
        fs.write_file("/projects/demo/table.txt".as_path(), b"hello")
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

        let mut rig = Self {
            server,
            transport: LinkTransport::default(),
            store,
            next_id: 100,
        };
        match state {
            LinkState::UntrustedPlay => {
                assert!(matches!(
                    rig.login(BLE_A, PLAY_PASSWORD),
                    LoginOutcome::Granted { .. }
                ));
            }
            LinkState::UntrustedEdit => {
                assert!(matches!(
                    rig.login(BLE_A, EDIT_PASSWORD),
                    LoginOutcome::Granted { .. }
                ));
            }
            _ => {}
        }
        rig
    }

    /// Send one request on `link` and return the one reply it got.
    fn request(&mut self, link: Link, request: ClientRequest) -> WireServerMsgBody {
        self.next_id += 1;
        let id = self.next_id;
        self.transport.sent.clear();
        block_on(self.server.tick_and_send(
            16,
            vec![incoming(link, id, request)],
            &mut self.transport,
        ))
        .expect("tick");
        let mut replies: Vec<(LinkId, WireServerMessage)> =
            core::mem::take(&mut self.transport.sent);
        // A project read streams; its last frame is the verdict.
        let (reply_link, reply) = replies.pop().expect("every request gets a reply");
        assert_eq!(reply_link, link.id, "the reply went to another link");
        assert_eq!(reply.id, id);
        reply.msg
    }

    fn begin(&mut self, link: Link) -> ([u8; 32], Vec<lpc_access::LoginOffer>) {
        match self.request(link, ClientRequest::LoginBegin) {
            WireServerMsgBody::LoginChallenge { nonce, offers } => (nonce, offers),
            other => panic!("expected a challenge, got {other:?}"),
        }
    }

    fn answer(&mut self, link: Link, macs: Vec<LoginMac>) -> LoginOutcome {
        match self.request(link, ClientRequest::LoginAnswer { macs }) {
            WireServerMsgBody::LoginResult(outcome) => outcome,
            other => panic!("expected a login result, got {other:?}"),
        }
    }

    /// A whole client-side login: begin, derive, answer.
    fn login(&mut self, link: Link, password: &[u8]) -> LoginOutcome {
        let (nonce, offers) = self.begin(link);
        self.answer(link, answer(password, &nonce, &offers))
    }

    fn close(&mut self, link: Link) {
        self.transport.closed.push(link.id);
        self.idle(16);
    }

    fn idle(&mut self, delta_ms: u32) {
        block_on(
            self.server
                .tick_and_send(delta_ms, Vec::new(), &mut self.transport),
        )
        .expect("tick");
    }

    fn write_sidecar(&mut self, project_dir: &str) {
        let sidecar =
            ProjectAccessFile::new(vec![secret("project camp", Tier::Play, b"campfire", 1)]);
        self.server
            .base_fs_mut()
            .write_file(
                alloc::format!("{project_dir}/.lp/access.json").as_path(),
                sidecar.to_json().unwrap().as_bytes(),
            )
            .unwrap();
    }

    fn load_demo_project(&mut self) {
        let fs = self.server.base_fs_mut();
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

/// A multi-link transport double: records which link every frame went to,
/// and reports closed links when told to.
#[derive(Default)]
struct LinkTransport {
    sent: Vec<(LinkId, WireServerMessage)>,
    closed: Vec<LinkId>,
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
        vec![USB, BLE_A, BLE_B]
    }

    fn take_closed_links(&mut self) -> Vec<LinkId> {
        core::mem::take(&mut self.closed)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

// --- helpers ------------------------------------------------------------------

fn row(label: &str, request: ClientRequest, needs: Required) -> Row {
    Row {
        label: label.to_string(),
        request,
        needs,
    }
}

fn incoming(link: Link, id: u64, msg: ClientRequest) -> Incoming {
    Incoming::on(link, ClientMessage { id, msg })
}

fn secret(label: &str, tier: Tier, password: &[u8], salt_byte: u8) -> SecretEntry {
    SecretEntry::from_password(label, tier, password, [salt_byte; 16], 3)
}

/// What a client holding `password` answers: one MAC per offer, in order.
fn answer(password: &[u8], nonce: &[u8; 32], offers: &[lpc_access::LoginOffer]) -> Vec<LoginMac> {
    offers
        .iter()
        .map(|offer| {
            let k = derive_login_key(password, &offer.salt, offer.iterations);
            LoginMac::compute(&k, nonce)
        })
        .collect()
}

/// Test entropy: a different fill every call, never a real RNG.
fn counting_entropy(buf: &mut [u8]) {
    static COUNTER: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
    let next = COUNTER
        .fetch_add(1, core::sync::atomic::Ordering::Relaxed)
        .wrapping_add(1);
    buf.fill(next);
}

fn status() -> HeartbeatStatus {
    HeartbeatStatus {
        fps: SampleStats {
            avg: 60.0,
            sdev: 0.0,
            min: 60.0,
            max: 60.0,
        },
        frame_count: 99,
        uptime_ms: 1234,
        memory: None,
        recovery: None,
        outputs: None,
        link: None,
    }
}

/// Setting the level the process already runs at, so the edit-tier rows do
/// not change what later tests log.
fn log_level_unchanged() -> lpc_wire::server::api::LogLevel {
    use lpc_wire::server::api::LogLevel;
    match log::max_level() {
        log::LevelFilter::Trace => LogLevel::Trace,
        log::LevelFilter::Debug => LogLevel::Debug,
        log::LevelFilter::Warn => LogLevel::Warn,
        log::LevelFilter::Error | log::LevelFilter::Off => LogLevel::Error,
        log::LevelFilter::Info => LogLevel::Info,
    }
}

/// A value's serde variant name: the bare string of a unit variant, or the
/// single key of an externally tagged one.
fn variant_name<T: serde::Serialize>(value: &T) -> String {
    let json = lpc_wire::json::to_string(value).expect("serializes");
    let rest = json.strip_prefix('{').unwrap_or(&json);
    let rest = rest.strip_prefix('"').expect("a string or a tagged object");
    rest[..rest.find('"').expect("closing quote")].to_string()
}

fn body_name(body: &WireServerMsgBody) -> &'static str {
    match body {
        WireServerMsgBody::NotPermitted { .. } => "NotPermitted",
        WireServerMsgBody::ListLoadedProjects { .. } => "ListLoadedProjects",
        WireServerMsgBody::Hello(_) => "Hello",
        WireServerMsgBody::Error { .. } => "Error",
        WireServerMsgBody::Filesystem(_) => "Filesystem",
        WireServerMsgBody::LoginChallenge { .. } => "LoginChallenge",
        WireServerMsgBody::LoginResult(_) => "LoginResult",
        WireServerMsgBody::AccessList { .. } => "AccessList",
        _ => "Other",
    }
}

fn sorted(names: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
    names.sort();
    names
}

/// The variant names serde's derive declares for enum `T`, captured from
/// the `deserialize_enum` call its `Deserialize` impl makes.
fn serde_variants<T: serde::de::DeserializeOwned>() -> &'static [&'static str] {
    use serde::de::{self, Visitor};

    struct Capture<'a>(&'a Cell<Option<&'static [&'static str]>>);

    impl<'de> de::Deserializer<'de> for Capture<'_> {
        type Error = de::value::Error;

        fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Self::Error> {
            Err(de::Error::custom("not an enum"))
        }

        fn deserialize_enum<V: Visitor<'de>>(
            self,
            _name: &'static str,
            variants: &'static [&'static str],
            _visitor: V,
        ) -> Result<V::Value, Self::Error> {
            self.0.set(Some(variants));
            Err(de::Error::custom("captured"))
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf option unit unit_struct newtype_struct seq tuple
            tuple_struct map struct identifier ignored_any
        }
    }

    let captured = Cell::new(None);
    let _ = T::deserialize(Capture(&captured));
    captured.get().expect("T deserializes as an enum")
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
