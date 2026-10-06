//! lp-cli's `lan:` link against a board's whole LAN path on the host (Wi-Fi
//! plan P06): `fw_esp32_common::net::host_lan_harness` serves the board's
//! own WebSocket server, LAN link slots and link mux in front of a real
//! `lpa-server`, on `127.0.0.1:0`; these tests drive lp-cli's transport and
//! commands at it, as `lp-cli … lan:<board>` would on a desk.
//!
//! What it proves: the secure link comes up and the board says hello on
//! it; an open board answers at its open tier; a locked board refuses a
//! client with no password in words, takes the right one and refuses a
//! wrong one; `wifi` and `upload` work over the link; a link past the
//! board's slots is told WebSocket close 1013; and the server never takes a request
//! off a link whose secure session is not up — not from a secure client, and
//! not from a plain one that never gets a session at all.
//!
//! Each test runs its own harness; the harness lets one run at a time per
//! process (the board's one static frame buffer), so the tests queue.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fw_esp32_common::net::host_lan_harness::{HarnessAccess, LanHarness, LanHarnessOptions};
use lp_cli::client::cli_connect::{CliConnection, cli_connect_with_password};
use lp_cli::commands::link::args::{CaptureArgs, RttArgs};
use lp_cli::commands::upload::{UploadArgs, handle_upload};
use lp_cli::commands::wifi::WifiCli;
use lp_cli::commands::wifi::args::{HostArgs, WifiCommand};
use lpa_client::transport_lan::{BoardPassword, LOCKED_WORDS, LanError, LanSocket, LanTarget};
use lpa_client::{HostSpecifier, LpClient};
use lpc_access::{OpenTo, SecretEntry, SecretKind, Tier};
use lpc_wire::lp_link::LinkConfig;
use lpc_wire::{ClientMessage, ClientRequest, PortRead, WifiPassword, WireLinkPort};

const PASSWORD: &str = "camp fire";
/// Cheap for a test; a person's password is written with far more.
const PASSWORD_ITERATIONS: u32 = 4;

#[test]
fn an_open_board_says_hello_on_the_secure_link_at_its_open_tier() {
    let harness = start(HarnessAccess::open(OpenTo::Play), None);
    run(async {
        let connection = connect(&harness, None)
            .await
            .expect("an open board lets us in");
        let hello = connection.hello().expect("the session's hello");
        assert!(hello.auth.required, "a LAN link is never trusted");
        assert_eq!(hello.auth.granted, Some(Tier::Play));
        let mut client = LpClient::new(connection.client_io());
        client
            .project_list_loaded()
            .await
            .expect("a play-tier request is answered");
        // Play is not edit: the gate still holds on a keyed link.
        let refused = client.network_status().await.expect_err("edit is refused");
        assert!(refused.to_string().contains("edit"), "{refused}");
        drop(client);
        connection.close().await;
    });
    assert_no_early_requests(&harness);
}

#[test]
fn a_locked_board_refuses_a_link_with_no_password_in_words() {
    let harness = start(locked(), None);
    let error = run(async {
        match connect(&harness, None).await {
            Ok(_) => panic!("a locked board let an anonymous link in"),
            Err(error) => error,
        }
    });
    assert_eq!(error.downcast_ref::<LanError>(), Some(&LanError::Locked));
    assert_eq!(error.to_string(), LOCKED_WORDS);
    assert!(
        LOCKED_WORDS.contains("--password-stdin") && LOCKED_WORDS.contains("LP_PASSWORD"),
        "{LOCKED_WORDS}"
    );
    assert_no_early_requests(&harness);
}

#[test]
fn a_locked_board_takes_the_right_password_at_its_entrys_tier() {
    let harness = start(locked(), None);
    run(async {
        let password = BoardPassword::new(PASSWORD);
        let connection = connect(&harness, Some(password))
            .await
            .expect("the right password gets in");
        assert_eq!(connection.hello().unwrap().auth.granted, Some(Tier::Edit));
        let mut client = LpClient::new(connection.client_io());
        client
            .network_status()
            .await
            .expect("an edit-tier request is answered");
        drop(client);
        connection.close().await;
    });
    let stats = harness.stats();
    // The anonymous session (no tier), then the password's.
    assert_eq!(stats.links_opened, 2, "{stats:?}");
    // The board's own hello on the password session comes after its grant
    // (the mux holds a keyed link's hello for it): it already says edit.
    assert_eq!(
        stats.hello_auths.last().map(|auth| auth.granted),
        Some(Some(Tier::Edit)),
        "the board's unsolicited hellos said {:?}",
        stats.hello_auths
    );
    assert_no_early_requests(&harness);
}

#[test]
fn a_locked_board_refuses_a_wrong_password_in_words() {
    let harness = start(locked(), None);
    let error = run(async {
        match connect(&harness, Some(BoardPassword::new("camp-fire"))).await {
            Ok(_) => panic!("a wrong password got in"),
            Err(error) => error,
        }
    });
    assert_eq!(
        error.downcast_ref::<LanError>(),
        Some(&LanError::WrongPassword),
        "{error}"
    );
    assert_eq!(error.to_string(), "the board refused that password");
    assert_no_early_requests(&harness);
}

#[test]
fn wifi_status_and_add_work_over_the_link() {
    let harness = start(HarnessAccess::open(OpenTo::Edit), None);
    run(async {
        let connection = connect(&harness, None).await.expect("connect");
        let mut client = LpClient::new(connection.client_io());
        let status = client.network_status().await.expect("status").value;
        assert!(status.networks.is_empty());
        let added = client
            .network_add("Camp".into(), WifiPassword::new("s3cret-pass"), None)
            .await
            .expect("add")
            .value;
        assert_eq!(added.networks.len(), 1);
        assert_eq!(added.networks[0].ssid, "Camp");
        drop(client);
        connection.close().await;
    });
    // And the command itself, end to end.
    lp_cli::commands::wifi::handle_wifi(WifiCli {
        command: WifiCommand::Status(HostArgs {
            host: harness.lan_address(),
            board_password: Default::default(),
            json: true,
        }),
    })
    .expect("`lp-cli wifi status lan:…` answers");
    assert_no_early_requests(&harness);
}

#[test]
fn an_upload_over_the_link_loads_and_runs_the_project() {
    let graphics: Arc<dyn lpa_server::LpGraphics> = Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        lpa_server::DEVICE_SHADER_FRONTEND,
    ));
    let harness = start(HarnessAccess::open(OpenTo::Edit), Some(graphics));
    handle_upload(UploadArgs {
        dir: workspace_dir().join("projects/test/basic"),
        host: harness.lan_address(),
        password: Default::default(),
        no_wait: false,
        wait_timeout_secs: 60,
    })
    .expect("`lp-cli upload projects/test/basic lan:…` deploys and the project runs");
    assert_no_early_requests(&harness);
}

#[test]
fn a_link_past_the_boards_slots_is_told_to_try_again_later() {
    let harness = start(HarnessAccess::open(OpenTo::Edit), None);
    run(async {
        let mut open = Vec::new();
        for n in 0..fw_esp32_common::radio_link::LAN_LINK_SLOTS {
            open.push(
                connect(&harness, None)
                    .await
                    .unwrap_or_else(|e| panic!("link {n}: {e}")),
            );
        }
        let error = match connect(&harness, None).await {
            Ok(_) => panic!("a link past the board's slots got one"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error.downcast_ref::<LanError>(),
                Some(LanError::Busy { .. })
            ),
            "{error}"
        );
        assert!(error.to_string().contains("try again later"), "{error}");
        for link in open {
            link.close().await;
        }
    });
    assert_eq!(harness.stats().refused, 1);
    assert_no_early_requests(&harness);
}

/// A plain lp-link client (no Noise) gets no session from the board's
/// secure responder, and nothing it sends reaches the server.
#[test]
fn a_plain_link_never_comes_up_and_the_server_sees_none_of_it() {
    let harness = start(HarnessAccess::open(OpenTo::Edit), None);
    let target = target(&harness);
    let mut socket = LanSocket::connect(&target).expect("the upgrade");
    let mut port = WireLinkPort::new(LinkConfig::ws(), 0x0b1a_1201, false);
    port.send_client(&ClientMessage {
        id: 1,
        msg: ClientRequest::ListLoadedProjects,
    })
    .unwrap();
    let started = Instant::now();
    let mut came_up = false;
    while started.elapsed() < Duration::from_secs(2) {
        let now = started.elapsed().as_micros() as u64;
        while let Some(frame) = port.poll_transmit(now) {
            if socket.send(frame).is_err() {
                break;
            }
        }
        match socket.recv(Duration::from_millis(5)) {
            Ok(Some(frame)) => port.on_datagram(now, &frame),
            Ok(None) => {}
            // The board may close it: a plain peer is no peer.
            Err(_) => break,
        }
        while let Some(read) = port.poll_read() {
            came_up |= matches!(read, PortRead::Up { .. } | PortRead::Message(_));
        }
    }
    assert!(!came_up, "a plain link came up on the board's LAN endpoint");
    let stats = harness.stats();
    assert_eq!(stats.requests, 0, "{stats:?}");
    assert!(stats.hello_auths.is_empty(), "{stats:?}");
}

#[test]
fn link_capture_hosts_a_lan_link_and_writes_its_console() {
    let harness = start(HarnessAccess::open(OpenTo::Edit), None);
    let dir = tempfile::tempdir().unwrap();
    let console = dir.path().join("lan.cap");
    lp_cli::commands::link::capture::capture(&CaptureArgs {
        target: harness.lan_address(),
        board_password: Default::default(),
        console: console.clone(),
        exit_on: Some("\"heartbeat\":{".into()),
        seconds: 20,
        json_replies: true,
        request: Vec::new(),
        ota: Default::default(),
    })
    .expect("the capture reached a heartbeat");
    let text = std::fs::read_to_string(&console).unwrap();
    for want in ["[link] up (session", "M!{\"id\":0,\"msg\":{\"hello\":{"] {
        assert!(text.contains(want), "no `{want}` in:\n{text}");
    }
    assert_no_early_requests(&harness);
}

#[test]
fn link_rtt_measures_a_lan_link_in_frames() {
    let harness = start(HarnessAccess::open(OpenTo::Edit), None);
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("lan-rtt.json");
    lp_cli::commands::link::rtt::rtt(&RttArgs {
        target: harness.lan_address(),
        board_password: Default::default(),
        chip: None,
        json: Some(json.clone()),
        console: None,
        count: 5,
        max_gap_ms: 5,
        seed: 7,
        reads: 2,
        read_bytes: 2048,
        writes: 2,
        write_bytes: 1024,
        warmup_s: 0.0,
        idle_s: 2.5,
        transfers_at_s: 0.0,
        requests_at_s: 0.0,
        tail_s: 0.0,
        grade: None,
        project: None,
        label: "lan harness".into(),
    })
    .expect("`lp-cli link rtt lan:…` runs");
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(report["link"], "lan");
    assert_eq!(report["request_rtt_ms"]["n"], 5);
    assert_eq!(report["request_rtt_frames"]["n"], 5, "{report}");
    assert_eq!(report["link_resets"], 0);
    assert_no_early_requests(&harness);
}

// ---- helpers ----

/// A locked board: a password entry at edit, and a browser's key at play
/// (one PBKDF2 iteration, so a password is never tried against it).
fn locked() -> HarnessAccess {
    HarnessAccess::locked(vec![
        SecretEntry::from_password(
            "desk",
            Tier::Edit,
            PASSWORD.as_bytes(),
            [5; 16],
            PASSWORD_ITERATIONS,
        ),
        SecretEntry::from_password("a browser", Tier::Play, &[9; 32], [6; 16], 1)
            .with_kind(SecretKind::Browser),
    ])
}

fn start(access: HarnessAccess, graphics: Option<Arc<dyn lpa_server::LpGraphics>>) -> LanHarness {
    LanHarness::start(LanHarnessOptions { access, graphics }).expect("the harness starts")
}

fn target(harness: &LanHarness) -> LanTarget {
    let spec = HostSpecifier::parse(&harness.lan_address()).unwrap();
    LanTarget::from_specifier(&spec).unwrap()
}

async fn connect(
    harness: &LanHarness,
    password: Option<BoardPassword>,
) -> anyhow::Result<CliConnection> {
    let spec = HostSpecifier::parse(&harness.lan_address())?;
    cli_connect_with_password(spec, password, |_| {}).await
}

/// The server took no request off a link before its secure session was up.
fn assert_no_early_requests(harness: &LanHarness) {
    let stats = harness.stats();
    assert_eq!(stats.early_requests, 0, "{stats:?}");
}

/// Run `future` the way lp-cli's commands do: a current-thread runtime and
/// a `LocalSet` (the CLI connection is single-actor).
fn run<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, future)
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace dir")
        .to_path_buf()
}
