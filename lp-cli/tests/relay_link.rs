//! The cloud relay end to end on the host: an in-process `lp-cloud-server`
//! (mem store, sessions minted in-process) ↔ lp-cli's host board on the
//! relay (`serve --relay`'s own pieces: the `lpc-relay` client and a secure
//! responder per route, in front of a real `lpa-server`) ↔ lp-cli's
//! `relay:` client.
//!
//! What it proves:
//!
//! - an account the board holds the key of reaches it at that key's tier,
//!   with no password;
//! - anyone else gets in with the board's password, and with no password is
//!   refused by the board — even though the board is open at edit to anyone
//!   nearby (a relayed link never gets that);
//! - no session at all is refused at the hub, in words;
//! - the board's one session: a second client is told busy;
//! - a deploy (the server going away and coming back on the same port)
//!   brings the board back within 15 s;
//! - two boards under one account are both in `ListBoards`, on the same
//!   network as the caller (loopback);
//! - with 100 ms added each way on the device leg, the session still comes
//!   up well inside its login deadline and requests answer (numbers printed);
//! - relay protocol 2: the host board says its firmware and its project's
//!   name, sends its picture (the empty one with no project), its own
//!   accounts read it and nobody else does, and watching makes it fast.

use std::future::Future;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use lp_cli::client::cli_connect::connect_relay;
use lp_cli::server::create_server::create_server_on;
use lp_cli::server::relay_host::relay_host_transport::NoLocalLinks;
use lp_cli::server::relay_host::{host_board_id, start_relay_host};
use lp_cli::server::run_server_loop_with;
use lpa_client::LpClient;
use lpa_client::transport_lan::{BoardPassword, LanError, os_entropy};
use lpc_access::{DeviceAccessFile, OpenTo, SecretEntry, Tier};
use lpc_cloud_api::KnownPicture;
use lpc_model::AsLpPath;
use lpc_relay::RelayBoardId;
use lpfs::{LpFs, LpFsMemory};

#[path = "support/relay_cloud.rs"]
mod relay_cloud;

use relay_cloud::Cloud;

const PASSWORD: &str = "camp fire";
/// Cheap for a test; a person's password is written with far more.
const PASSWORD_ITERATIONS: u32 = 4;

#[test]
fn an_account_the_board_holds_reaches_it_at_its_tier_with_no_password() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);

    run(async {
        let connection = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("the account's key opens the board");
        assert_eq!(
            connection.hello().unwrap().auth.granted,
            Some(Tier::Edit),
            "the account entry's tier"
        );
        let mut client = LpClient::new(connection.client_io());
        client
            .network_status()
            .await
            .expect("an edit-tier request (wifi status) is answered through the relay");
        client
            .project_list_loaded()
            .await
            .expect("and a play-tier one");
        drop(client);
        connection.close().await;
    });
}

#[test]
fn anyone_else_needs_the_boards_password_even_on_a_board_open_to_anyone_nearby() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let bob = cloud.account("bob");
    let guest = cloud.guest();
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[password_entry()]);
    cloud.wait_for_boards(1);

    // No password: the anonymous session holds nothing, so the board is
    // locked to this client — though it is open at edit to anyone nearby.
    for session in [bob.session.clone(), guest.clone()] {
        let refused = run(async {
            match connect_relay(cloud.target(board), Some(session), None).await {
                Ok(_) => panic!("a visitor with no password got in"),
                Err(error) => error,
            }
        });
        assert_eq!(
            refused.downcast_ref::<LanError>(),
            Some(&LanError::Locked),
            "{refused}"
        );
    }

    // The board's password: in, at the password entry's tier.
    for session in [bob.session.clone(), guest] {
        run(async {
            let connection = connect_relay(
                cloud.target(board),
                Some(session),
                Some(BoardPassword::new(PASSWORD)),
            )
            .await
            .expect("the board's password opens it for a visitor");
            assert_eq!(connection.hello().unwrap().auth.granted, Some(Tier::Edit));
            let mut client = LpClient::new(connection.client_io());
            client
                .network_status()
                .await
                .expect("edit through the relay");
            drop(client);
            connection.close().await;
        });
    }

    let wrong = run(async {
        match connect_relay(
            cloud.target(board),
            Some(bob.session.clone()),
            Some(BoardPassword::new("camp-fire")),
        )
        .await
        {
            Ok(_) => panic!("a wrong password got in"),
            Err(error) => error,
        }
    });
    assert_eq!(
        wrong.downcast_ref::<LanError>(),
        Some(&LanError::WrongPassword)
    );
}

#[test]
fn no_session_is_refused_at_the_hub_in_words() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);
    let refused = run(async {
        match connect_relay(
            cloud.target(board),
            None,
            Some(BoardPassword::new(PASSWORD)),
        )
        .await
        {
            Ok(_) => panic!("a client with no session got through the hub"),
            Err(error) => error,
        }
    });
    assert!(
        refused.to_string().contains("Sign in to lightplayer.app"),
        "{refused}"
    );
}

#[test]
fn the_boards_one_session_tells_a_second_client_busy() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);
    run(async {
        let first = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("the first session");
        let second = connect_relay(cloud.target(board), Some(alice.session.clone()), None).await;
        let error = match second {
            Ok(_) => panic!("a second session got in to a board that holds one"),
            Err(error) => error,
        };
        assert!(
            matches!(
                error.downcast_ref::<LanError>(),
                Some(LanError::Busy { .. })
            ),
            "{error}"
        );
        assert!(error.to_string().contains("busy"), "{error}");
        first.close().await;
    });
}

#[test]
fn after_a_deploy_the_board_is_back_within_fifteen_seconds() {
    let mut cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);

    let restarted = Instant::now();
    cloud.restart();
    let session = cloud.session_for(alice.uid);
    let deadline = restarted + Duration::from_secs(15);
    loop {
        if cloud.list_boards(&session).boards.len() == 1 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the board did not come back within 15 s"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    println!(
        "relay e2e: board back {:.1} s after the deploy",
        restarted.elapsed().as_secs_f64()
    );
    run(async {
        let connection = connect_relay(cloud.target(board), Some(session.clone()), None)
            .await
            .expect("a client reconnects after the deploy");
        connection.close().await;
    });
}

#[test]
fn two_boards_under_one_account_are_both_listed_on_the_same_network() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let one = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    let two = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(2);

    let list = cloud.list_boards(&alice.session);
    let mut ids: Vec<String> = list.boards.iter().map(|board| board.id.clone()).collect();
    ids.sort();
    let mut expected = vec![one.to_string(), two.to_string()];
    expected.sort();
    assert_eq!(ids, expected);
    for board in &list.boards {
        assert!(board.same_network, "loopback is one network");
        assert_eq!(board.lan, None, "a host board reports no LAN address");
        assert_eq!(board.wire_proto, lpc_wire::WIRE_PROTO_VERSION);
    }
    let bob = cloud.account("bob");
    assert!(cloud.list_boards(&bob.session).boards.is_empty());
}

/// R2: the relay adds round trip. With 100 ms added each way on the device
/// leg (so ≥ 200 ms per lp-link round trip), the session still comes up
/// inside the 10 s login deadline and requests answer, with no lp-link
/// timing changed. The numbers go in the phase's record.
#[test]
fn a_session_through_a_slow_device_leg_stays_up() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let slow = cloud.delayed_origin(Duration::from_millis(100));
    let board = HostBoard::start(&slow, vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);

    run(async {
        let opening = Instant::now();
        let connection = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("the session comes up over a slow device leg");
        let opened = opening.elapsed();
        assert!(opened < Duration::from_secs(10), "{opened:?}");
        let mut client = LpClient::new(connection.client_io());
        let mut rtts = Vec::new();
        for _ in 0..5 {
            let asked = Instant::now();
            client.network_status().await.expect("a request answers");
            rtts.push(asked.elapsed().as_millis());
        }
        println!(
            "relay e2e, device leg +100 ms each way: session up in {} ms; request round trips {rtts:?} ms",
            opened.as_millis()
        );
        drop(client);
        connection.close().await;
    });
}

/// The board's own relay driver (the C6's, on the host harness: its
/// WebSocket client, driver, network slot and mux in front of a real
/// lpa-server) against the real hub: it registers with its account key,
/// the hub lists it with its LAN address, and an account session edits it.
#[test]
fn a_boards_own_relay_driver_registers_with_its_lan_address_and_carries_a_session() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let (harness, board) = HarnessBoard::start(&cloud, vec![alice.entry()]);
    cloud.wait_for_boards(1);

    let list = cloud.list_boards(&alice.session);
    assert_eq!(list.boards.len(), 1);
    assert_eq!(list.boards[0].id, board.to_string());
    assert_eq!(
        list.boards[0].lan.as_deref(),
        Some(harness.addr().to_string().as_str()),
        "the board's LAN address rides its hello"
    );
    run(async {
        let connection = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("the account's key opens the board's relay session");
        assert_eq!(connection.hello().unwrap().auth.granted, Some(Tier::Edit));
        let mut client = LpClient::new(connection.client_io());
        client
            .network_status()
            .await
            .expect("an edit request through the board's own relay driver");
        drop(client);
        connection.close().await;
    });
    harness.stop();
}

/// D2 with the real hub and clients: Alice's relay session moves to the LAN
/// when she opens it with the same key, and Bob, through the relay, is told
/// busy while she holds it.
#[test]
fn the_same_key_moves_a_relay_session_to_the_lan_and_anyone_else_is_busy() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let bob = cloud.account("bob");
    let (harness, board) = HarnessBoard::start(&cloud, vec![alice.entry()]);
    cloud.wait_for_boards(1);
    let alice_key = alice.entry();
    run(async {
        let relayed = connect_relay(cloud.target(board), Some(alice.session.clone()), None)
            .await
            .expect("Alice through the relay");
        let lan_target =
            lpa_client::transport_lan::LanTarget::new("127.0.0.1", harness.addr().port());
        let (lan, hello) = lpa_client::transport_lan::connect_lan_transport(
            lan_target.endpoint(),
            lpa_client::transport_lan::LanOptions {
                password: None,
                want_packed: false,
                held_keys: vec![(
                    lpc_wire::lp_link::secure_channel::KeyId(alice_key.salt),
                    lpc_wire::lp_link::secure_channel::Psk::new(lpc_access::link_psk(&alice_key.k)),
                )],
            },
        )
        .await
        .expect("Alice on the LAN with the same key takes her session over");
        assert_eq!(hello.auth.granted, Some(Tier::Edit));
        let mut relay_client = LpClient::new(relayed.client_io());
        assert!(
            relay_client.network_status().await.is_err(),
            "the relay session is gone once the LAN has it"
        );
        drop(relay_client);
        relayed.close().await;

        let refused =
            match connect_relay(cloud.target(board), Some(bob.session.clone()), None).await {
                Ok(_) => panic!("Bob got the board while Alice holds its one session"),
                Err(error) => error,
            };
        assert!(
            matches!(
                refused.downcast_ref::<LanError>(),
                Some(LanError::Busy { .. })
            ),
            "{refused}"
        );
        drop(lan);
    });
    assert_eq!(harness.stats().takeovers, 1);
    harness.stop();
}

/// Relay protocol 2 on the host board: its hello says lp-cli's version,
/// its project report names `Basic`, and its picture reaches the hub's
/// cache, where the board's own account reads it and nobody else does.
#[test]
fn a_host_board_sends_its_project_and_its_picture_and_members_read_them() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let bob = cloud.account("bob");
    let board = HostBoard::start_playing(&cloud.origin(), vec![alice.entry()], "basic");
    cloud.wait_for_boards(1);

    let listed = wait_until(Duration::from_secs(10), || {
        let list = cloud.list_boards(&alice.session);
        list.boards
            .into_iter()
            .find(|presence| presence.id == board.to_string() && presence.project.is_some())
    })
    .expect("the board's project reaches the hub");
    assert_eq!(listed.relay_proto, 2);
    assert_eq!(listed.firmware.as_deref(), Some(env!("LP_APP_VERSION")));
    assert_eq!(listed.project.as_deref(), Some("Basic"));

    let picture = wait_until(Duration::from_secs(10), || {
        cloud
            .board_pictures(&alice.session, &[known(board, None)], false)
            .pictures
            .pop()
    })
    .expect("the board's picture reaches the hub");
    assert_eq!(picture.id, board.to_string());
    assert!(picture.online);
    let lamps: u64 = picture.outputs.iter().map(|&lamps| u64::from(lamps)).sum();
    assert!(
        lamps > 0,
        "the basic project has lamps: {:?}",
        picture.outputs
    );
    let colors = picture.colors.as_ref().expect("colours on a first read");
    assert_eq!(colors.0.len() % 3, 0);
    let count = (colors.0.len() / 3) as u64;
    assert_eq!(count, lamps.min(256), "min(T, 256) samples");
    assert!(colors.0.iter().any(|&byte| byte != 0), "something is lit");
    println!(
        "[relay-pictures] host board: outputs {:?}, {count} colours, seq {}",
        picture.outputs, picture.seq
    );

    // Idle: the next picture is a minute away, so the caller holds this one.
    let again = cloud.board_pictures(&alice.session, &[known(board, Some(picture.seq))], false);
    assert_eq!(again.pictures.len(), 1);
    assert_eq!(again.pictures[0].seq, picture.seq);
    assert!(again.pictures[0].colors.is_none(), "the caller has these");

    assert!(
        cloud
            .board_pictures(&bob.session, &[known(board, None)], false)
            .pictures
            .is_empty(),
        "another account reads nothing"
    );
    let guest = cloud.guest();
    assert!(
        cloud
            .board_pictures(&guest, &[known(board, None)], true)
            .pictures
            .is_empty(),
        "a guest reads nothing"
    );
}

/// With nothing loaded the host board still answers: the empty picture,
/// and no project.
#[test]
fn a_host_board_with_no_project_sends_an_empty_picture() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start(&cloud.origin(), vec![alice.entry()], &[]);
    cloud.wait_for_boards(1);

    let picture = wait_until(Duration::from_secs(10), || {
        cloud
            .board_pictures(&alice.session, &[known(board, None)], false)
            .pictures
            .pop()
    })
    .expect("the empty picture reaches the hub");
    assert!(picture.outputs.is_empty(), "no lamps");
    assert_eq!(
        picture.colors.map(|colors| colors.0.len()),
        Some(0),
        "count 0"
    );
    let list = cloud.list_boards(&alice.session);
    let presence = list
        .boards
        .iter()
        .find(|presence| presence.id == board.to_string())
        .expect("listed");
    assert_eq!(presence.project, None, "no project");
    assert_eq!(presence.relay_proto, 2);
}

/// A member watching (`watch: true`, once a second) makes the host board
/// send pictures fast. A host test on real time: the net is generous and
/// the number is printed.
#[test]
fn watching_makes_a_host_board_fast() {
    let cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let board = HostBoard::start_playing(&cloud.origin(), vec![alice.entry()], "basic");
    cloud.wait_for_boards(1);
    let first = wait_until(Duration::from_secs(10), || {
        cloud
            .board_pictures(&alice.session, &[known(board, None)], false)
            .pictures
            .pop()
    })
    .expect("a first picture");

    let started = Instant::now();
    let mut seqs = vec![first.seq];
    while started.elapsed() < Duration::from_secs(20) {
        let held = seqs.last().copied();
        let read = cloud.board_pictures(&alice.session, &[known(board, held)], true);
        if let Some(picture) = read.pictures.first()
            && Some(picture.seq) != held
        {
            seqs.push(picture.seq);
        }
        if seqs.len() > 3 && started.elapsed() >= Duration::from_secs(5) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    println!(
        "[relay-pictures] watched for {:.1} s: seq moved {} times ({seqs:?})",
        started.elapsed().as_secs_f64(),
        seqs.len() - 1
    );
    assert!(
        seqs.len() > 3,
        "watching moved the picture at least 3 times: {seqs:?}"
    );
}

// ---- helpers ---------------------------------------------------------

/// A `BoardPictures` entry for `board`, holding `seq`.
fn known(board: RelayBoardId, seq: Option<u64>) -> KnownPicture {
    KnownPicture {
        id: board.to_string(),
        seq,
    }
}

/// Poll `probe` every 100 ms until it answers, up to `wait`.
fn wait_until<T>(wait: Duration, mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + wait;
    loop {
        if let Some(found) = probe() {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The C6's board-side relay on the host harness: its network slot, mux
/// and server, locked (no "Anyone"), holding `accounts`, on `cloud`'s relay.
struct HarnessBoard;

impl HarnessBoard {
    fn start(
        cloud: &Cloud,
        accounts: Vec<SecretEntry>,
    ) -> (
        fw_esp32_common::net::host_lan_harness::LanHarness,
        RelayBoardId,
    ) {
        use fw_esp32_common::net::host_lan_harness::{
            HarnessAccess, HarnessRelay, LanHarness, LanHarnessOptions,
        };
        let mut mac = [0u8; 6];
        os_entropy(&mut mac);
        mac[0] = (mac[0] | 0x02) & 0xfe;
        let harness = LanHarness::start(LanHarnessOptions {
            access: HarnessAccess::locked(accounts),
            graphics: None,
            relay: Some(HarnessRelay {
                host: "127.0.0.1".to_string(),
                port: cloud.port,
                board_mac: mac,
                label: "harness board".to_string(),
            }),
        })
        .expect("the harness starts");
        (harness, RelayBoardId(mac))
    }
}

/// The board's own password, at edit (stretched, as a person's is).
fn password_entry() -> SecretEntry {
    SecretEntry::from_password(
        "desk",
        Tier::Edit,
        PASSWORD.as_bytes(),
        [5; 16],
        PASSWORD_ITERATIONS,
    )
}

/// lp-cli's host board on the relay, on its own thread (the server is
/// single-threaded): a memory filesystem whose access store holds
/// `accounts` and `passwords` and is open at EDIT to anyone nearby — the
/// worst case for the relay's second lock.
struct HostBoard;

impl HostBoard {
    fn start(origin: &str, accounts: Vec<SecretEntry>, passwords: &[SecretEntry]) -> RelayBoardId {
        Self::spawn(origin, accounts, passwords, None)
    }

    /// The same board playing `projects/test/<project>`: its files written
    /// into the memory filesystem under `projects/<project>/`, loaded and
    /// run two frames before the relay starts (no upload, and no process
    /// environment touched: these tests run in parallel).
    fn start_playing(
        origin: &str,
        accounts: Vec<SecretEntry>,
        project: &'static str,
    ) -> RelayBoardId {
        Self::spawn(origin, accounts, &[], Some(project))
    }

    fn spawn(
        origin: &str,
        accounts: Vec<SecretEntry>,
        passwords: &[SecretEntry],
        project: Option<&'static str>,
    ) -> RelayBoardId {
        let mut seed = [0u8; 16];
        os_entropy(&mut seed);
        let board = host_board_id(&URL_SAFE_NO_PAD.encode(seed));
        let origin = origin.to_string();
        let mut secrets = accounts;
        secrets.extend_from_slice(passwords);
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let fs = LpFsMemory::new();
            let store = DeviceAccessFile {
                version: DeviceAccessFile::VERSION,
                secrets,
                ble_enabled: true,
                open: OpenTo::Edit,
            };
            fs.write_file(
                DeviceAccessFile::PATH.as_path(),
                store.to_json().unwrap().as_bytes(),
            )
            .unwrap();
            if let Some(project) = project {
                write_test_project(&fs, project);
            }
            let accounts = lp_cli::server::relay_host::relay_accounts(&fs);
            let mut server = create_server_on(Box::new(fs), None, true, None).unwrap();
            server.set_entropy_source(Some(os_entropy));
            if let Some(project) = project {
                server
                    .load_project(format!("/projects/{project}").as_path())
                    .expect("the test project loads");
                for _ in 0..2 {
                    server.advance_frame(16).expect("a frame");
                }
            }
            runtime.block_on(async move {
                let mut transport =
                    start_relay_host(NoLocalLinks, &origin, board, "test board".into(), accounts)
                        .unwrap();
                transport.report_project(&server);
                let _ = run_server_loop_with(server, transport, |server, transport| {
                    transport.after_tick(server);
                })
                .await;
            });
        });
        board
    }
}

/// `projects/test/<project>`'s files, into `fs` under `/projects/<project>/`.
fn write_test_project(fs: &LpFsMemory, project: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../projects/test")
        .join(project);
    for entry in std::fs::read_dir(&dir).expect("the test project's directory") {
        let path = entry.expect("an entry").path();
        if !path.is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        fs.write_file(
            format!("/projects/{project}/{name}").as_path(),
            &std::fs::read(&path).expect("a project file"),
        )
        .expect("written");
    }
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
