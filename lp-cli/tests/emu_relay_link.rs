//! The cloud relay with an EMULATED C6 (Wi-Fi relay plan P9, the CI cell):
//! the shipped image under `lp-cli emu run --lan`, on a virtual LAN whose
//! fixture names an uplink (`[[uplink]] name = "lightplayer.app"`), carried
//! to an in-process `lp-cloud-server` on the host. The board runs the image a
//! board is flashed with and dials `lightplayer.app` like a real one; the
//! LAN's gateway resolves the name and carries the connection.
//!
//! One board, two boots over one flash file, in order:
//!
//! 1. a first run: joined, no account key — `relay: noAccount`, and no
//!    board at the hub; then the account key installed over USB
//!    (`AccessAdd`, as Studio does) — the board registers by itself, and
//!    the run ends there with its flash kept;
//! 2. a second boot over that flash, as a fielded board boots: it registers
//!    by itself (`relay: connected`), and the hub lists it with its LAN
//!    address; **P1** (relay protocol 2, plan
//!    `2026-10-08-2050-pictures-through-the-cloud`): at `relayProto` 2, with
//!    the firmware its wire hello names, and the hub holds its first picture
//!    while its heartbeat says `pictures N idle`;
//! 3. `relay:<mac>@<origin>` reaches it at the account's tier, and an
//!    upload of `projects/test/basic` through the relay passes the board's
//!    gates; **P2**: the hub lists the project's name, and Bob and a guest
//!    read no picture of the board; **P3**: a member watches it
//!    (`BoardPictures { watch: true }` every 2 s): its heartbeat says
//!    `watched`, pictures come fast and carry the project's colours, **P4**
//!    the heap is read through a relay session while it is watched, and once
//!    nobody asks, the board says `idle` again by itself;
//! 4. **takeover**: with Alice's relay session open, a `lan:` client holding
//!    Alice's key takes the board's one network session; the relay session
//!    is gone, and Bob through the relay is told busy;
//! 5. a deploy (the server going away and back on its port): the board
//!    comes back by itself (the time printed, never gated: it is wall time);
//!    **P5**: the new hub holds no picture until the board is back with one;
//! 6. Cloud relay off over USB: the board leaves the hub and says `off`; on
//!    again, it comes back; **P6**: while it is off the hub keeps its
//!    picture (`online: false`); back on, the picture is online and newer;
//! 7. the account's key reset on the server, then a deploy: the board is
//!    refused, `refused: unknownAccount`.
//!
//! The board's console rides its USB link, and with no link open its log
//! lines are dropped; so a step that waits for the board's own words holds
//! a USB link while it waits ([`UsbConsole`]), and reads what that link
//! heard.
//!
//! Every wait is a **safety net with a wall clock**, never an input: the
//! forward and the uplink are host sockets. Times printed are wall seconds
//! on this host, on `lp-emu:esp32c6:t1+net=lan` — never a gate.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`, or
//! `LP_CI_IMAGES`); `just test-emu-serve` runs it beside the LAN cells.
//!
//! The second test, `a_protocol_1_core_at_the_new_hub_is_answered_as_before`,
//! is the protocol 1 lane's (`just walk-wifi-emu relay-p1`): an image built
//! at the last relay protocol 1 commit (`LP_RELAY_P1_ELF`) at this hub. It
//! returns at once without that variable, so CI's `--include-ignored` run of
//! this file passes it in no time.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{Scope, ScopedJoinHandle};
use std::time::{Duration, Instant};

use lp_cli::client::cli_connect::{CliConnection, cli_connect_with_password, connect_relay};
use lp_cli::commands::upload::{UploadArgs, handle_upload};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::transport_lan::{LanError, LanOptions, LanTarget, connect_lan_transport};
use lpa_client::{ClientEvent, HostSpecifier, LpClient, TokioClientIo, WIRE_ENCODING_ENV};
use lpa_link::{DeviceEvent, DeviceLineOrigin};
use lpc_access::{SecretEntry, Tier, link_psk};
use lpc_cloud_api::{BoardPicture, KnownPicture};
use lpc_relay::RelayBoardId;
use lpc_wire::lp_link::secure_channel::{KeyId, Psk};
use lpc_wire::server::{NetworkStatus, RelayRefusal, RelayState, StationState};
use lpc_wire::{ClientRequest, WifiPassword};

#[path = "support/relay_cloud.rs"]
mod relay_cloud;

use relay_cloud::Cloud;

const SSID: &str = "lp-walk-net";
const PASSWORD: &str = "correct-horse-42";

/// The wall-clock net on the board joining (an emulated boot, a scan, the
/// join and DHCP, at emulated speed on a loaded box).
const JOIN_NET: Duration = Duration::from_secs(420);
/// The wall-clock net on the relay reaching a state.
const RELAY_NET: Duration = Duration::from_secs(180);
/// The wall-clock net on the forward's line appearing.
const FORWARD_NET: Duration = Duration::from_secs(120);
const STATUS_POLL: Duration = Duration::from_secs(2);
/// How often a member watching a board asks for its picture (a page
/// showing it would ask about this often; the hub's lease is 15 s).
const WATCH_EVERY: Duration = Duration::from_secs(2);
/// The protocol 1 lane's window: a member watches the board and a relay
/// session comes and goes, for at least this long of wall time.
const PROTOCOL_1_WINDOW: Duration = Duration::from_secs(60);
/// The project `projects/test/basic` names itself (`project.json`).
const BASIC_NAME: &str = "Basic";

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-serve` runs it"]
fn an_emulated_c6_reaches_lightplayer_app_through_the_lans_uplink() {
    let Some(elf) = image() else { return };
    let mut cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let bob = cloud.account("bob");
    let dir = tempfile::tempdir().unwrap();
    let fixture = lan_fixture(dir.path(), &cloud);
    let flash = dir.path().join("chip.bin");

    // 1. A first run: the network over USB; no account key, so nothing to
    //    register for. Then the account key, as Studio installs it: the
    //    board registers by itself, and the run ends there, its flash kept.
    let usb_addr = free_addr();
    let mut first = EmulatedBoard::start(
        &elf,
        &usb_addr,
        &fixture,
        &flash,
        Some("[relay] state=connected"),
    );
    wait_listening(&usb_addr);
    let usb = format!("serial:tcp://{usb_addr}");
    usb_replies_stay_json(true);
    run(async {
        let connection = connect(&usb).await;
        let mut client = LpClient::new(connection.client_io());
        client
            .network_add(String::from(SSID), WifiPassword::new(PASSWORD), None)
            .await
            .expect("`wifi add` over USB");
        let joined = Instant::now();
        let status = wait_status(&mut client, &first, JOIN_NET, "the board joins", |s| {
            matches!(s.station, StationState::Connected { .. })
        })
        .await;
        eprintln!(
            "emu_relay_link: joined in {:.0} s wall; relay {}",
            joined.elapsed().as_secs_f64(),
            status.relay.kind()
        );
        wait_status(&mut client, &first, RELAY_NET, "no account", |s| {
            s.relay == RelayState::NoAccount
        })
        .await;
        heap(&mut client, "joined, no account key (no relay buffers)").await;
        assert_eq!(
            cloud.board_count(),
            0,
            "no account key: no board at the hub"
        );
        client
            .send_request(ClientRequest::AccessAdd {
                entry: alice.entry(),
            })
            .await
            .expect("`AccessAdd` over USB");
        // Keep the link up (the board's log lines, the `--exit-on` line
        // among them, ride it) until the run ends and takes it down.
        let started = Instant::now();
        while client.network_status().await.is_ok() && started.elapsed() < RELAY_NET {
            tokio::time::sleep(STATUS_POLL).await;
        }
        drop(client);
    });
    usb_replies_stay_json(false);
    first.wait_exit(RELAY_NET);
    drop(first);

    // 2. The board boots with its network and key saved, as a fielded one
    //    does (its LAN and relay buffers made at boot): it registers by
    //    itself, and the hub lists it with its LAN address.
    let booted = Instant::now();
    let usb_addr = free_addr();
    let board = EmulatedBoard::start(&elf, &usb_addr, &fixture, &flash, None);
    wait_listening(&usb_addr);
    let usb = format!("serial:tcp://{usb_addr}");
    usb_replies_stay_json(true);
    // The app version the board's wire hello names: the firmware its relay
    // hello carries too (P1).
    let firmware = run(async {
        let connection = connect(&usb).await;
        let firmware = connection
            .hello()
            .map(|hello| hello.build.version.into_owned());
        let mut client = LpClient::new(connection.client_io());
        wait_status(
            &mut client,
            &board,
            JOIN_NET,
            "relay connected after boot",
            |s| s.relay == RelayState::Connected,
        )
        .await;
        heap(
            &mut client,
            "booted with network and key saved, relay registered, no session",
        )
        .await;
        drop(client);
        connection.close().await;
        firmware
    });
    usb_replies_stay_json(false);
    assert!(cloud.boards_within(1, Duration::from_secs(10)));
    eprintln!(
        "emu_relay_link: boot → registered in {:.1} s wall (lp-emu:esp32c6:t1+net=lan)",
        booted.elapsed().as_secs_f64()
    );
    let list = cloud.list_boards(&alice.session);
    assert_eq!(list.boards.len(), 1);
    let presence = &list.boards[0];
    assert!(
        presence
            .lan
            .as_deref()
            .is_some_and(|lan| lan.ends_with(":80")),
        "the board's LAN address rides its hello: {:?}",
        presence.lan
    );
    let board_id: RelayBoardId = presence.id.parse().expect("a relay id");

    // P1. Relay protocol 2: the hub lists what the board's hello said, and
    // holds the picture it sent as soon as the hub set its pace — a fresh
    // chip has loaded nothing, so a picture of no lamps.
    assert_eq!(presence.relay_proto, 2, "this build's hello is protocol 2");
    let firmware = firmware.expect("the board's wire hello");
    assert_eq!(
        presence.firmware.as_deref(),
        Some(firmware.as_str()),
        "the relay hello's firmware is the wire hello's app version"
    );
    match &presence.project {
        None => {
            eprintln!("emu_relay_link: listed at relayProto 2, firmware {firmware}, no project")
        }
        // Not asserted away (plan P6): say it, for the walk record.
        Some(name) => eprintln!(
            "emu_relay_link: listed at relayProto 2, firmware {firmware}, and a project \
             before any upload: {name:?}"
        ),
    }
    let first = wait_picture(
        &cloud,
        &alice.session,
        board_id,
        &board,
        "the first picture",
        |_| true,
    );
    assert!(first.online, "the board is on the relay: {first:?}");
    {
        let mark = 0;
        let console = UsbConsole::open(&usb, &board, None);
        wait_heartbeat(&board, mark, RELAY_NET, "a picture sent, idle", |beat| {
            beat.state == "connected" && beat.pictures >= 1 && beat.mode == "idle"
        });
        console.close();
    }
    eprintln!(
        "emu_relay_link: the hub holds the board's picture (seq {}, {} lamps, {} samples)",
        first.seq,
        lamps(&first),
        colours(&first).len() / 3
    );

    // 3. Through the relay, at the account's tier, and an upload.
    run(async {
        let connection = connect_relay(cloud.target(board_id), Some(alice.session.clone()), None)
            .await
            .expect("Alice reaches her board through the relay");
        assert_eq!(connection.hello().unwrap().auth.granted, Some(Tier::Edit));
        let mut client = LpClient::new(connection.client_io());
        let asked = Instant::now();
        client.network_status().await.expect("a request answers");
        eprintln!(
            "emu_relay_link: a status request through the relay answered in {} ms wall",
            asked.elapsed().as_millis()
        );
        drop(client);
        connection.close().await;
    });
    // SAFETY: this test binary's one firmware test; nothing else reads the
    // environment while it runs.
    unsafe { std::env::set_var("LP_CLOUD_SESSION", &alice.session) };
    let relay_spec = format!("relay:{board_id}@{}", cloud.origin());
    handle_upload(UploadArgs {
        dir: workspace_dir().join("projects/test/basic"),
        host: relay_spec.clone(),
        password: Default::default(),
        no_wait: false,
        wait_timeout_secs: 300,
    })
    .expect("`lp-cli upload projects/test/basic relay:…` deploys and the project runs");
    eprintln!("emu_relay_link: an upload over {relay_spec}");

    // P2. The board reports the project it now runs (its name in the clear;
    // its uid only as a keyed tag, never shown here); the hub lists it. Only
    // the accounts the board proved read its picture.
    let named = Instant::now();
    loop {
        let list = cloud.list_boards(&alice.session);
        let project = list.boards.first().and_then(|b| b.project.clone());
        if project.as_deref() == Some(BASIC_NAME) {
            break;
        }
        assert!(
            named.elapsed() < RELAY_NET,
            "the hub never listed the project {BASIC_NAME:?} (last {project:?})\n{}",
            board.output()
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    let known = [KnownPicture {
        id: board_id.to_string(),
        seq: None,
    }];
    let bob_reads = cloud.board_pictures(&bob.session, &known, true);
    assert!(bob_reads.pictures.is_empty(), "Bob reads {bob_reads:?}");
    let guest = cloud.guest();
    let guest_reads = cloud.board_pictures(&guest, &known, true);
    assert!(
        guest_reads.pictures.is_empty(),
        "a guest reads {guest_reads:?}"
    );
    eprintln!(
        "emu_relay_link: the hub lists the project {BASIC_NAME:?}; Bob and a guest read no picture"
    );

    // The gate case (plan A3): projects/test/basic loaded, the relay
    // registered and a session open through it. The heap is read through
    // that session, with no USB link open: a USB link that asked for packed
    // replies costs the board a ~6.9 KB learned table and ~0.5 KB of session
    // state a relay client never costs.
    run(async {
        let relayed = connect_relay(cloud.target(board_id), Some(alice.session.clone()), None)
            .await
            .expect("Alice through the relay");
        let mut through = LpClient::new(relayed.client_io());
        heap(
            &mut through,
            "projects/test/basic loaded, relay registered, a relay session open (read through it)",
        )
        .await;
        request_rtt(&mut through).await;
        drop(through);
        relayed.close().await;
    });

    // P3. A member watches the board: fast pictures, the board says
    // `watched`, and the pictures carry the project's colours (P2). P4: the
    // heap while it is watched and a relay session is open. Then nobody
    // asks, and the board falls back to `idle` on its own clock (the lease,
    // 15 s, plus a heartbeat).
    let console = UsbConsole::open(&usb, &board, None);
    let mark = board.said_len();
    let last_watch = std::thread::scope(|scope| {
        let watcher = Watcher::start(scope, &cloud, &alice.session, board_id);
        wait_heartbeat(&board, mark, RELAY_NET, "watched", |beat| {
            beat.state == "connected" && beat.mode == "watched"
        });
        let lit = watcher.wait(&board, "a picture of the project's lamps", |p| lamps(p) > 0);
        let samples = colours(&lit).len() / 3;
        assert_eq!(
            colours(&lit).len(),
            3 * samples,
            "three bytes a sample: {lit:?}"
        );
        assert!(
            (1..=256).contains(&samples) && samples as u64 <= lamps(&lit),
            "1 ≤ count ≤ min(256, lamps): {samples} samples of {} lamps",
            lamps(&lit)
        );
        assert!(
            colours(&lit).iter().any(|&b| b != 0),
            "the project renders: its picture is not all dark: {lit:?}"
        );
        eprintln!(
            "emu_relay_link: the picture carries the project's colours (seq {}, outputs {:?}, \
             {samples} samples, {} lit)",
            lit.seq,
            lit.outputs,
            colours(&lit)
                .chunks(3)
                .filter(|rgb| rgb.iter().any(|&b| b != 0))
                .count()
        );
        let first_seq = watcher.first_seq();
        let moved = watcher.wait(&board, "three more pictures while watched", |p| {
            p.seq >= first_seq + 3
        });
        eprintln!(
            "emu_relay_link: watched: seq {first_seq} → {} in {:.1} s wall \
             (lp-emu:esp32c6:t1+net=lan; not a gate)",
            moved.seq,
            watcher.elapsed().as_secs_f64()
        );
        console.close();
        run(async {
            let relayed = connect_relay(cloud.target(board_id), Some(alice.session.clone()), None)
                .await
                .expect("Alice through the relay");
            let mut through = LpClient::new(relayed.client_io());
            heap(
                &mut through,
                "projects/test/basic loaded, relay registered, pictures watched, a relay session \
                 open (read through it)",
            )
            .await;
            drop(through);
            relayed.close().await;
        });
        watcher.stop().last
    });
    let console = UsbConsole::open(&usb, &board, None);
    let mark = board.said_len();
    wait_heartbeat(&board, mark, RELAY_NET, "idle again", |beat| {
        beat.state == "connected" && beat.mode == "idle"
    });
    console.close();
    eprintln!(
        "emu_relay_link: idle again by itself {:.1} s wall after the last watch \
         (lp-emu:esp32c6:t1+net=lan; not a gate)",
        last_watch.elapsed().as_secs_f64()
    );

    // 4. The same key moves the session to the LAN; anyone else is busy.
    let lan = board.forward();
    let port: u16 = lan.rsplit(':').next().unwrap().parse().unwrap();
    run(async {
        let relayed = connect_relay(cloud.target(board_id), Some(alice.session.clone()), None)
            .await
            .expect("Alice through the relay");
        let alice_key = alice.entry();
        let (local, hello) = connect_lan_transport(
            LanTarget::new("127.0.0.1", port).endpoint(),
            LanOptions {
                password: None,
                want_packed: false,
                held_keys: vec![(KeyId(alice_key.salt), Psk::new(link_psk(&alice_key.k)))],
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
            match connect_relay(cloud.target(board_id), Some(bob.session.clone()), None).await {
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
        // The heap is read over the LAN session itself (no USB link open).
        let mut over_lan = LpClient::new(TokioClientIo::new(Box::new(local)));
        heap(
            &mut over_lan,
            "projects/test/basic loaded, relay registered, a LAN session open (read through it)",
        )
        .await;
        drop(over_lan);
    });

    // 5. A deploy: the board comes back by itself. The wall clock is only
    // the net here (AGENTS.md: never gate on host wall-clock from an
    // emulated run); the 15 s bound is the relay client's, pinned in
    // `lpc-relay`'s rules and on the host in `relay_link.rs`.
    let restarted = Instant::now();
    cloud.restart();
    // P5. A new hub has no cache: no picture until the board is back. A
    // session does not survive the deploy (the store keeps the accounts,
    // not their sessions), so Alice signs in again.
    let alice_session = cloud.session_for(alice.uid);
    let away = cloud.board_count() == 0;
    let fresh = cloud.board_pictures(&alice_session, &known, false);
    if away && cloud.board_count() == 0 {
        assert!(
            fresh.pictures.is_empty(),
            "a new hub holds no picture: {fresh:?}"
        );
        eprintln!("emu_relay_link: right after the deploy the new hub holds no picture");
    } else {
        eprintln!(
            "emu_relay_link: the board was back before the new hub could be asked; its empty \
             cache was not checked"
        );
    }
    assert!(
        cloud.boards_within(1, RELAY_NET),
        "the board did not come back after the deploy\n{}",
        board.output()
    );
    eprintln!(
        "emu_relay_link: back {:.1} s wall after the deploy (lp-emu:esp32c6:t1+net=lan; not a gate)",
        restarted.elapsed().as_secs_f64()
    );
    let back = wait_picture(
        &cloud,
        &alice_session,
        board_id,
        &board,
        "a picture after the deploy",
        |p| p.online,
    );
    eprintln!(
        "emu_relay_link: a picture is back with the board after the deploy (seq {}, {} lamps)",
        back.seq,
        lamps(&back)
    );

    // 6. Cloud relay off: the board leaves and does not dial; on: back.
    run(async {
        let connection = connect(&usb).await;
        let mut client = LpClient::new(connection.client_io());
        client
            .network_set(None, Some(false))
            .await
            .expect("`wifi set --cloud-relay off` over USB");
        wait_status(&mut client, &board, RELAY_NET, "relay off", |s| {
            s.relay == RelayState::Off
        })
        .await;
        drop(client);
        connection.close().await;
    });
    assert!(cloud.boards_within(0, Duration::from_secs(10)));
    // P6. The board has left: the hub keeps its last picture, offline.
    let kept = cloud.board_pictures(&alice_session, &known, false);
    let offline = kept
        .pictures
        .first()
        .unwrap_or_else(|| panic!("the hub dropped the picture of a board gone offline: {kept:?}"));
    assert!(!offline.online, "the board is off the relay: {offline:?}");
    eprintln!(
        "emu_relay_link: offline, the hub keeps the board's picture (seq {}, online false)",
        offline.seq
    );
    std::thread::sleep(Duration::from_secs(5));
    assert_eq!(cloud.board_count(), 0, "off: no dial");
    run(async {
        let connection = connect(&usb).await;
        let mut client = LpClient::new(connection.client_io());
        client
            .network_set(None, Some(true))
            .await
            .expect("`wifi set --cloud-relay on` over USB");
        wait_status(
            &mut client,
            &board,
            RELAY_NET,
            "relay connected again",
            |s| s.relay == RelayState::Connected,
        )
        .await;
        drop(client);
        connection.close().await;
    });
    let offline_seq = offline.seq;
    let online = wait_picture(
        &cloud,
        &alice_session,
        board_id,
        &board,
        "the picture online again, and newer",
        |p| p.online && p.seq > offline_seq,
    );
    eprintln!(
        "emu_relay_link: back online, the picture is online again and newer (seq {offline_seq} → {})",
        online.seq
    );

    // 7. Alice's key reset on the server, then a deploy: refused by name.
    cloud.reset_account_key(alice.uid);
    cloud.restart();
    run(async {
        let connection = connect(&usb).await;
        let mut client = LpClient::new(connection.client_io());
        wait_status(&mut client, &board, RELAY_NET, "refused", |s| {
            s.relay
                == RelayState::Refused {
                    reason: RelayRefusal::UnknownAccount,
                }
        })
        .await;
        drop(client);
        connection.close().await;
    });
    assert_eq!(cloud.board_count(), 0);
}

/// The protocol 1 lane (plan `2026-10-08-2050-pictures-through-the-cloud`,
/// Q13): a core built at the last relay protocol 1 commit, at this hub. It
/// registers, is routed and is listed as it was, at `relayProto` 1, and is
/// never sent a protocol 2 frame. The board's own evidence of that last
/// part: a protocol 1 client closes its leg on any frame it does not
/// expect and dials again, which prints a second `[relay] leg open`; one
/// USB link is held from its first boot to the end, so every such line
/// reaches this test. (The in-process hub is a debug build, whose send
/// guard also fails outright on such a frame.)
///
/// `LP_RELAY_P1_ELF` names the image (`just walk-wifi-emu relay-p1` finds
/// one); without it this returns at once.
#[test]
#[ignore = "the protocol 1 lane's (`just walk-wifi-emu relay-p1`); needs LP_RELAY_P1_ELF"]
fn a_protocol_1_core_at_the_new_hub_is_answered_as_before() {
    let Some(elf) = std::env::var_os("LP_RELAY_P1_ELF").map(PathBuf::from) else {
        eprintln!("emu_relay_link (protocol 1): not asked — LP_RELAY_P1_ELF unset");
        return;
    };
    assert!(
        elf.is_file(),
        "LP_RELAY_P1_ELF={} is not a file",
        elf.display()
    );
    let mut cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let dir = tempfile::tempdir().unwrap();
    let fixture = lan_fixture(dir.path(), &cloud);
    let flash = dir.path().join("chip.bin");
    let usb_addr = free_addr();
    let board = EmulatedBoard::start(&elf, &usb_addr, &fixture, &flash, None);
    wait_listening(&usb_addr);
    let usb = format!("serial:tcp://{usb_addr}");

    // The network and Alice's key over the one USB link this test holds:
    // the board registers by itself.
    let console = UsbConsole::open(&usb, &board, Some(alice.entry()));
    console.wait_relay(&board, JOIN_NET + RELAY_NET, RelayState::Connected);
    assert!(cloud.boards_within(1, Duration::from_secs(10)));
    eprintln!(
        "emu_relay_link (protocol 1): registered by itself (the board answers: relay connected)"
    );
    let list = cloud.list_boards(&alice.session);
    assert_eq!(list.boards.len(), 1);
    let presence = &list.boards[0];
    assert_eq!(presence.relay_proto, 1, "{presence:?}");
    assert_eq!(presence.firmware, None, "{presence:?}");
    assert_eq!(presence.project, None, "{presence:?}");
    eprintln!("emu_relay_link (protocol 1): listed at relayProto 1, no firmware, no project");
    let board_id: RelayBoardId = presence.id.parse().expect("a relay id");

    // The window: a member watching, a relay session opened, used and
    // closed — and the hub never holds a picture for it.
    let window = Instant::now();
    let answered = console.answers();
    let reads = std::thread::scope(|scope| {
        let watcher = Watcher::start(scope, &cloud, &alice.session, board_id);
        run(async {
            let relayed = connect_relay(cloud.target(board_id), Some(alice.session.clone()), None)
                .await
                .expect("Alice reaches a protocol 1 board through the relay");
            assert_eq!(relayed.hello().unwrap().auth.granted, Some(Tier::Edit));
            let mut client = LpClient::new(relayed.client_io());
            for _ in 0..5 {
                client
                    .network_status()
                    .await
                    .expect("a request through the relay");
            }
            drop(client);
            relayed.close().await;
        });
        eprintln!(
            "emu_relay_link (protocol 1): a relay session opened at the edit tier, five status \
             requests answered, closed"
        );
        while window.elapsed() < PROTOCOL_1_WINDOW {
            std::thread::sleep(Duration::from_millis(500));
        }
        watcher.stop().reads
    });
    assert!(
        reads.iter().all(Option::is_none),
        "the hub held a picture for a protocol 1 board: {reads:?}"
    );
    eprintln!(
        "emu_relay_link (protocol 1): watched for {:.0} s wall ({} reads): the hub never held a \
         picture for it",
        window.elapsed().as_secs_f64(),
        reads.len()
    );
    let during = console.states_since(answered);
    assert!(
        during.iter().all(|state| *state == RelayState::Connected),
        "the board's relay state left `connected` in the window: {during:?}"
    );
    let legs = board.count_lines("[relay] leg open to lightplayer.app");
    let beats = relay_heartbeats(&board.said_since(0))
        .into_iter()
        .filter(|beat| beat.state == "connected")
        .count();
    assert_eq!(
        legs,
        1,
        "a second leg means the board dropped the first (an unexpected frame is a protocol \
         error to a protocol 1 client)\n{}",
        board.heard_tail()
    );
    assert!(
        beats >= 1,
        "no `[relay] state=connected` heartbeat\n{}",
        board.heard_tail()
    );
    eprintln!(
        "emu_relay_link (protocol 1): one leg the whole window (1 `[relay] leg open` line); its \
         status answered `connected` {} times and its heartbeat said `state=connected` {beats} times",
        during.len()
    );

    // A deploy: back by itself, still protocol 1, on one new leg.
    cloud.restart();
    assert!(
        cloud.boards_within(1, RELAY_NET),
        "the protocol 1 board did not come back after the deploy\n{}",
        board.output()
    );
    let session = cloud.session_for(alice.uid);
    let list = cloud.list_boards(&session);
    assert_eq!(list.boards[0].relay_proto, 1, "{:?}", list.boards[0]);
    // Two more status answers through the console, so a leg that would
    // drop on the new hub's first frames has had time to say so.
    console.wait_answers(2);
    let states = console.close();
    assert_eq!(
        board.count_lines("[relay] leg open to lightplayer.app"),
        2,
        "one new leg after the deploy, and only one\n{}",
        board.heard_tail()
    );
    assert_eq!(states.last(), Some(&RelayState::Connected), "{states:?}");
    eprintln!(
        "emu_relay_link (protocol 1): back after a deploy, still relayProto 1, on one new leg"
    );
}

/// The board's own heap figures from its next heartbeat, over whichever
/// session `client` holds, printed with `what`: free bytes and the largest
/// free block. Never a gate.
///
/// Read a figure through the session it describes. A USB link that asked for
/// packed replies makes the board allocate a learned table (~6.9 KB) and
/// ~0.5 KB of session state that a relay or LAN client does not cost; the
/// rows with no network session to read through use a USB link that never
/// asks (`usb_replies_stay_json`), which leaves only the ~0.5 KB.
async fn heap(client: &mut LpClient<impl lpa_client::ClientIo>, what: &str) {
    let until = Instant::now() + Duration::from_secs(30);
    // A heartbeat after the state settled: skip what was queued before.
    tokio::time::sleep(Duration::from_secs(3)).await;
    while Instant::now() < until {
        let outcome = client.network_status().await.expect("`wifi status`");
        for event in outcome.events {
            if let ClientEvent::Heartbeat {
                memory: Some(memory),
                uptime_ms,
                ..
            } = event
            {
                eprintln!(
                    "emu_relay_link: heap — {what}: free {} B, used {} B, largest block {} B \
                     (uptime {uptime_ms} ms, lp-emu:esp32c6:t1+net=lan)",
                    memory.free_bytes,
                    memory.used_bytes,
                    memory
                        .largest_free_block
                        .map_or_else(|| "?".to_string(), |b| b.to_string())
                );
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    eprintln!("emu_relay_link: heap — {what}: no heartbeat within 30 s");
}

/// Whether this test's USB connections ask the board for packed replies
/// (`LP_WIRE_ENCODING`, read when a connection opens). `true` keeps them JSON.
fn usb_replies_stay_json(json: bool) {
    // SAFETY: this test binary's one firmware test; nothing else reads the
    // environment while it runs, and no connection is opening here.
    unsafe {
        if json {
            std::env::set_var(WIRE_ENCODING_ENV, "json");
        } else {
            std::env::remove_var(WIRE_ENCODING_ENV);
        }
    }
}

/// How promptly the board answers through the relay with a project
/// rendering: `RTT_COUNT` status requests one after another, their wall
/// round trips at p50 and p90 — in milliseconds, in frames at the board's
/// own frame rate (its heartbeat's), and in frames it drew per wall second
/// (two heartbeats' frame counts), as `lp-cli link rtt` reports a LAN link.
/// Printed, never a gate: the round trip is wall time on this host.
async fn request_rtt(client: &mut LpClient<impl lpa_client::ClientIo>) {
    const RTT_COUNT: usize = 20;
    let mut ms = Vec::with_capacity(RTT_COUNT);
    let mut fps = None;
    let mut counts: Vec<(Instant, u64)> = Vec::new();
    for _ in 0..RTT_COUNT {
        let asked = Instant::now();
        let outcome = client
            .network_status()
            .await
            .expect("a request through the relay");
        ms.push(asked.elapsed().as_secs_f64() * 1000.0);
        for event in outcome.events {
            if let ClientEvent::Heartbeat {
                fps: rate,
                frame_count,
                ..
            } = event
            {
                fps = Some(f64::from(rate.avg));
                counts.push((Instant::now(), frame_count));
            }
        }
    }
    // The frame rate per wall second needs two heartbeats: keep asking
    // (untimed) until a second one comes, for at most 30 s.
    let until = Instant::now() + Duration::from_secs(30);
    while counts.len() < 2 && Instant::now() < until {
        let outcome = client
            .network_status()
            .await
            .expect("a request through the relay");
        for event in outcome.events {
            if let ClientEvent::Heartbeat { frame_count, .. } = event {
                counts.push((Instant::now(), frame_count));
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    ms.sort_by(f64::total_cmp);
    let at = |p: usize| ms[(ms.len() * p / 100).min(ms.len() - 1)];
    let (p50, p90) = (at(50), at(90));
    let frames = |rate: Option<f64>| match rate {
        Some(rate) if rate > 0.0 => format!(
            "{:.1} / {:.1} frames at {rate:.1} fps",
            p50 * rate / 1000.0,
            p90 * rate / 1000.0
        ),
        _ => String::from("no frame rate"),
    };
    let wall_fps = match (counts.first(), counts.last()) {
        (Some((t0, f0)), Some((t1, f1))) if t1 > t0 && f1 > f0 => {
            Some((f1 - f0) as f64 / t1.duration_since(*t0).as_secs_f64())
        }
        _ => None,
    };
    eprintln!(
        "emu_relay_link: {RTT_COUNT} status requests through the relay, projects/test/basic \
         rendering: p50 {p50:.0} ms / p90 {p90:.0} ms wall; {} (the board's own); {} drawn \
         per wall second (lp-emu:esp32c6:t1+net=lan; not a gate)",
        frames(fps),
        frames(wall_fps)
    );
}

/// Ask the board's status until `done` says yes, or fail after `net`.
async fn wait_status(
    client: &mut LpClient<impl lpa_client::ClientIo>,
    board: &EmulatedBoard,
    net: Duration,
    what: &str,
    done: impl Fn(&NetworkStatus) -> bool,
) -> NetworkStatus {
    let started = Instant::now();
    loop {
        let status = client
            .network_status()
            .await
            .expect("`wifi status` over USB")
            .value;
        if done(&status) {
            // The board's own answer, for the walk to read
            // (`scripts/emu/walk-wifi-emu-relay.mjs`).
            let relay = match status.relay {
                RelayState::Refused { reason } => format!("refused: {}", reason.code()),
                other => String::from(other.kind()),
            };
            eprintln!("emu_relay_link: the board answers ({what}): relay {relay}");
            return status;
        }
        assert!(
            started.elapsed() < net,
            "{what}: not within {net:?}; last station {:?}, relay {:?}\n{}",
            status.station,
            status.relay,
            board.output()
        );
        tokio::time::sleep(STATUS_POLL).await;
    }
}

/// The virtual LAN: the walk's made-up network, and an uplink that carries
/// `lightplayer.app` to `cloud`.
fn lan_fixture(dir: &Path, cloud: &Cloud) -> PathBuf {
    let fixture = dir.join("virtual_lan.toml");
    std::fs::write(
        &fixture,
        format!(
            "# emu_relay_link.rs: made-up test values only.\n\
             [[access_point]]\nname = \"{SSID}\"\npassword = \"{PASSWORD}\"\nsignal_dbm = -50\n\n\
             [[uplink]]\nname = \"lightplayer.app\"\nto = \"127.0.0.1:{}\"\n",
            cloud.port
        ),
    )
    .unwrap();
    fixture
}

/// The lamps a picture covers (`T`, its outputs' sum).
fn lamps(picture: &BoardPicture) -> u64 {
    picture.outputs.iter().map(|&n| u64::from(n)).sum()
}

/// A picture's colour bytes, R G B a sample (empty when the answer left
/// them out).
fn colours(picture: &BoardPicture) -> &[u8] {
    picture.colors.as_ref().map_or(&[], |colors| &colors.0)
}

/// Ask the hub for `id`'s picture as `session` (not watching) until one
/// satisfies `done`, or fail after `RELAY_NET`.
fn wait_picture(
    cloud: &Cloud,
    session: &str,
    id: RelayBoardId,
    board: &EmulatedBoard,
    what: &str,
    done: impl Fn(&BoardPicture) -> bool,
) -> BoardPicture {
    let started = Instant::now();
    let known = [KnownPicture {
        id: id.to_string(),
        seq: None,
    }];
    loop {
        let list = cloud.board_pictures(session, &known, false);
        if let Some(picture) = list.pictures.into_iter().find(|p| done(p)) {
            return picture;
        }
        assert!(
            started.elapsed() < RELAY_NET,
            "{what}: not within {RELAY_NET:?}\n{}",
            board.output()
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// One `[relay]` heartbeat line of the board's console: `[relay]
/// state=connected routes=0 rx=… tx=… · takeovers 0 busy 0 · pictures 3
/// idle`. A protocol 1 core's line has no `pictures` part (0 and "").
#[derive(Debug)]
struct RelayHeartbeat {
    line: String,
    state: String,
    pictures: u64,
    mode: String,
}

/// The `[relay]` heartbeats in `text`, oldest first.
fn relay_heartbeats(text: &str) -> Vec<RelayHeartbeat> {
    const STATE: &str = "[relay] state=";
    const PICTURES: &str = "· pictures ";
    text.lines()
        .filter_map(|line| {
            let line = &line[line.find(STATE)?..];
            let state = line[STATE.len()..].split(" routes=").next()?.to_string();
            let mut picture = line
                .rfind(PICTURES)
                .map(|at| line[at + PICTURES.len()..].split_whitespace())
                .into_iter()
                .flatten();
            let pictures = picture.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let mode = picture.next().unwrap_or("").to_string();
            Some(RelayHeartbeat {
                line: line.to_string(),
                state,
                pictures,
                mode,
            })
        })
        .collect()
}

/// Wait until a `[relay]` heartbeat the board said after `mark` (a
/// [`EmulatedBoard::said_len`]) satisfies `done`, and print it for the walk;
/// fail after `net`. The board's console reaches this test only through a
/// [`UsbConsole`]: hold one while waiting.
fn wait_heartbeat(
    board: &EmulatedBoard,
    mark: usize,
    net: Duration,
    what: &str,
    done: impl Fn(&RelayHeartbeat) -> bool,
) {
    let started = Instant::now();
    loop {
        if let Some(beat) = relay_heartbeats(&board.said_since(mark))
            .into_iter()
            .find(|beat| done(beat))
        {
            eprintln!("emu_relay_link: the board says ({what}): {}", beat.line);
            return;
        }
        assert!(
            started.elapsed() < net,
            "{what}: the board never said it within {net:?}; its console:\n{}\n{}",
            board.heard_tail(),
            board.output()
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// A USB link held open on its own thread, asking the board's status every
/// `STATUS_POLL`, so the board's console reaches this test (with no link
/// open, its log lines are dropped) while the test does other things. Its
/// replies stay JSON, as the heap rows' links do. Every status answer's
/// relay state is kept.
struct UsbConsole {
    stop: Arc<AtomicBool>,
    relay: Arc<Mutex<Vec<RelayState>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl UsbConsole {
    /// Open the link to `board`, whose console it hears; with `setup`,
    /// first add the walk's network over it and install that account key,
    /// as step 1 does.
    fn open(usb: &str, board: &EmulatedBoard, setup: Option<SecretEntry>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let relay = Arc::new(Mutex::new(Vec::new()));
        let (opened, is_open) = mpsc::channel();
        let usb = usb.to_string();
        let heard = Arc::clone(&board.console);
        // The connection reads `LP_WIRE_ENCODING` as it opens; it is set back
        // only once it has (and before any other thread of this test runs).
        usb_replies_stay_json(true);
        let thread = {
            let (stop, relay) = (Arc::clone(&stop), Arc::clone(&relay));
            std::thread::Builder::new()
                .name("usb-console".to_string())
                .spawn(move || {
                    run(async move {
                        let connection = connect_heard(&usb, heard).await;
                        let _ = opened.send(());
                        let mut client = LpClient::new(connection.client_io());
                        if let Some(entry) = setup {
                            client
                                .network_add(String::from(SSID), WifiPassword::new(PASSWORD), None)
                                .await
                                .expect("`wifi add` over USB");
                            client
                                .send_request(ClientRequest::AccessAdd { entry })
                                .await
                                .expect("`AccessAdd` over USB");
                        }
                        while !stop.load(Ordering::Relaxed) {
                            let status = client
                                .network_status()
                                .await
                                .expect("`wifi status` over the USB console link")
                                .value;
                            relay.lock().expect("relay states").push(status.relay);
                            tokio::time::sleep(STATUS_POLL).await;
                        }
                        drop(client);
                        connection.close().await;
                    });
                })
                .expect("the USB console thread")
        };
        is_open
            .recv_timeout(Duration::from_secs(120))
            .expect("the USB console link opens");
        usb_replies_stay_json(false);
        Self {
            stop,
            relay,
            thread: Some(thread),
        }
    }

    /// How many status answers have come so far.
    fn answers(&self) -> usize {
        self.relay.lock().expect("relay states").len()
    }

    /// The relay states answered from answer `from` on.
    fn states_since(&self, from: usize) -> Vec<RelayState> {
        self.relay.lock().expect("relay states")[from..].to_vec()
    }

    /// Wait for the board's status to answer `want`, or fail after `net`.
    fn wait_relay(&self, board: &EmulatedBoard, net: Duration, want: RelayState) {
        let started = Instant::now();
        while self.relay.lock().expect("relay states").last() != Some(&want) {
            assert!(
                started.elapsed() < net,
                "the board never answered relay {want:?} within {net:?}\n{}",
                board.output()
            );
            self.check_alive();
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Wait for `more` status answers after the ones already in.
    fn wait_answers(&self, more: usize) {
        let want = self.answers() + more;
        let started = Instant::now();
        while self.answers() < want {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "the USB console stopped answering"
            );
            self.check_alive();
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// Fail now if the link's thread has died (its panic says why).
    fn check_alive(&self) {
        let finished = self.thread.as_ref().is_none_or(|t| t.is_finished());
        assert!(!finished, "the USB console link ended early");
    }

    /// Close the link; every relay state it was answered.
    fn close(mut self) -> Vec<RelayState> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take()
            && let Err(panic) = thread.join()
        {
            std::panic::resume_unwind(panic);
        }
        std::mem::take(&mut *self.relay.lock().expect("relay states"))
    }
}

impl Drop for UsbConsole {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// A member watching a board, as a page showing it would:
/// `BoardPictures { watch: true }` every `WATCH_EVERY` on its own thread,
/// every answer kept.
struct Watcher<'scope> {
    started: Instant,
    stop: Arc<AtomicBool>,
    reads: Arc<Mutex<Vec<(Instant, Option<BoardPicture>)>>>,
    thread: ScopedJoinHandle<'scope, ()>,
}

/// What a [`Watcher`] saw: when it last asked, and each answer.
struct Watched {
    last: Instant,
    reads: Vec<Option<BoardPicture>>,
}

impl<'scope> Watcher<'scope> {
    fn start<'env>(
        scope: &'scope Scope<'scope, 'env>,
        cloud: &'env Cloud,
        session: &'env str,
        id: RelayBoardId,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let reads = Arc::new(Mutex::new(Vec::new()));
        let thread = {
            let (stop, reads) = (Arc::clone(&stop), Arc::clone(&reads));
            let known = [KnownPicture {
                id: id.to_string(),
                seq: None,
            }];
            scope.spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let list = cloud.board_pictures(session, &known, true);
                    let picture = list.pictures.into_iter().next();
                    reads
                        .lock()
                        .expect("watch reads")
                        .push((Instant::now(), picture));
                    let asked = Instant::now();
                    while asked.elapsed() < WATCH_EVERY && !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
            })
        };
        Self {
            started: Instant::now(),
            stop,
            reads,
            thread,
        }
    }

    /// The first picture a read returned that satisfies `done`; fail after
    /// `RELAY_NET`.
    fn wait(
        &self,
        board: &EmulatedBoard,
        what: &str,
        done: impl Fn(&BoardPicture) -> bool,
    ) -> BoardPicture {
        loop {
            if let Some(picture) = self
                .reads
                .lock()
                .expect("watch reads")
                .iter()
                .find_map(|(_, picture)| picture.as_ref().filter(|p| done(p)))
            {
                return picture.clone();
            }
            assert!(
                self.started.elapsed() < RELAY_NET,
                "{what}: not within {RELAY_NET:?} of watching\n{}",
                board.output()
            );
            assert!(
                !self.thread.is_finished(),
                "the watching thread ended early"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The `seq` of the first picture the watch read: where it began.
    fn first_seq(&self) -> u64 {
        self.reads
            .lock()
            .expect("watch reads")
            .iter()
            .find_map(|(_, picture)| picture.as_ref().map(|p| p.seq))
            .expect("the watch has read a picture")
    }

    /// Wall time since the watch began.
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Stop watching; what the watch saw.
    fn stop(self) -> Watched {
        self.stop.store(true, Ordering::Relaxed);
        if let Err(panic) = self.thread.join() {
            std::panic::resume_unwind(panic);
        }
        let reads = std::mem::take(&mut *self.reads.lock().expect("watch reads"));
        Watched {
            last: reads.last().map_or(self.started, |(at, _)| *at),
            reads: reads.into_iter().map(|(_, picture)| picture).collect(),
        }
    }
}

/// `lp-cli emu run --lan`, killed when the test is done with it, with its
/// output kept for the failure messages and read for the forward's line.
struct EmulatedBoard {
    child: Child,
    output: Arc<Mutex<String>>,
    /// The board's console as a [`UsbConsole`] heard it, every line.
    console: Arc<Mutex<String>>,
    forward: mpsc::Receiver<String>,
}

impl EmulatedBoard {
    /// `emu run --lan <fixture>` over the flash file `flash` (read at start,
    /// written back when the run ends), ending at the first console line
    /// holding `exit_on` when given.
    fn start(elf: &Path, usb: &str, fixture: &Path, flash: &Path, exit_on: Option<&str>) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lp-cli"));
        command
            .args(["emu", "run", "--elf"])
            .arg(elf)
            .args(["--link", usb, "--lan"])
            .arg(fixture)
            .arg("--flash")
            .arg(flash)
            .args([
                "--reboot-on-reset",
                "--timeout",
                "3600s",
                "--wall-timeout",
                "3600",
            ]);
        if let Some(line) = exit_on {
            command.args(["--exit-on", line]);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawning lp-cli emu run --lan");
        let output = Arc::new(Mutex::new(String::new()));
        let (tx, forward) = mpsc::channel();
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        drain(stdout, Arc::clone(&output), tx.clone());
        drain(stderr, Arc::clone(&output), tx);
        Self {
            child,
            output,
            console: Arc::new(Mutex::new(String::new())),
            forward,
        }
    }

    /// Wait for the run to end by itself (its `--exit-on` line), its flash
    /// written back.
    fn wait_exit(&mut self, net: Duration) {
        let started = Instant::now();
        while started.elapsed() < net {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("`emu run` never ended by itself:\n{}", self.output());
    }

    /// The board's forward, `lan:127.0.0.1:<port>`, as `emu run` printed it.
    fn forward(&self) -> String {
        self.forward.recv_timeout(FORWARD_NET).unwrap_or_else(|_| {
            panic!(
                "`emu run --lan` never printed a forward:\n{}",
                self.output()
            )
        })
    }

    /// How much of the board's console has been heard so far: a mark for
    /// [`Self::said_since`].
    fn said_len(&self) -> usize {
        self.console.lock().expect("console poisoned").len()
    }

    /// Every console line heard after `mark`.
    fn said_since(&self, mark: usize) -> String {
        let text = self.console.lock().expect("console poisoned");
        text.get(mark..).unwrap_or_default().to_string()
    }

    /// How many console lines heard hold `needle`.
    fn count_lines(&self, needle: &str) -> usize {
        let text = self.console.lock().expect("console poisoned");
        text.lines().filter(|line| line.contains(needle)).count()
    }

    /// The last of the board's console heard, for a failure message.
    fn heard_tail(&self) -> String {
        let text = self.console.lock().expect("console poisoned");
        let start = text.len().saturating_sub(4_000);
        let start = (start..text.len())
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(text.len());
        text[start..].to_string()
    }

    /// The last of what `emu run` said, for a failure message.
    fn output(&self) -> String {
        let text = self.output.lock().expect("output poisoned");
        let start = text.len().saturating_sub(6_000);
        let start = (start..text.len())
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(text.len());
        text[start..].to_string()
    }
}

impl Drop for EmulatedBoard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn drain(pipe: impl Read + Send + 'static, output: Arc<Mutex<String>>, tx: mpsc::Sender<String>) {
    std::thread::Builder::new()
        .name("emu-run-output".to_string())
        .spawn(move || {
            for line in BufReader::new(pipe).lines() {
                let Ok(line) = line else { break };
                if let Some(forward) = forward_in(&line) {
                    let _ = tx.send(forward);
                }
                let mut text = output.lock().expect("output poisoned");
                text.push_str(&line);
                text.push('\n');
            }
        })
        .expect("the output drain");
}

/// `lan:127.0.0.1:<port>` out of a line that names one.
fn forward_in(line: &str) -> Option<String> {
    const PREFIX: &str = "lan:127.0.0.1:";
    let at = line.find(PREFIX)?;
    let digits: String = line[at + PREFIX.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then(|| format!("{PREFIX}{digits}"))
}

async fn connect(address: &str) -> CliConnection {
    let spec = HostSpecifier::parse(address).expect("an address lp-cli parses");
    cli_connect_with_password(spec, None, |_| {})
        .await
        .unwrap_or_else(|error| panic!("connecting {address}: {error:#}"))
}

/// [`connect`], appending each console line the board says over this link
/// to `heard`.
async fn connect_heard(address: &str, heard: Arc<Mutex<String>>) -> CliConnection {
    let spec = HostSpecifier::parse(address).expect("an address lp-cli parses");
    cli_connect_with_password(spec, None, move |event| {
        if let DeviceEvent::LogLine {
            line,
            origin: DeviceLineOrigin::Device,
        } = event
        {
            let mut heard = heard.lock().expect("console poisoned");
            heard.push_str(&line);
            heard.push('\n');
        }
    })
    .await
    .unwrap_or_else(|error| panic!("connecting {address}: {error:#}"))
}

fn image() -> Option<PathBuf> {
    match fw_esp32c6_image(&FwImage::SHIPPED) {
        Ok(path) => Some(path),
        Err(reason) => {
            eprintln!("emu_relay_link: skipped — {reason}");
            None
        }
    }
}

fn free_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

/// Wait for the board's USB socket without connecting to it.
fn wait_listening(addr: &str) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        if TcpListener::bind(addr).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("the emulated board never listened on {addr}");
}

fn run<F: std::future::Future>(future: F) -> F::Output {
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
