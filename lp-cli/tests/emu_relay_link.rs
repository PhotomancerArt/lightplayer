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
//!    address;
//! 3. `relay:<mac>@<origin>` reaches it at the account's tier, and an
//!    upload of `projects/test/basic` through the relay passes the board's
//!    gates;
//! 4. **takeover**: with Alice's relay session open, a `lan:` client holding
//!    Alice's key takes the board's one network session; the relay session
//!    is gone, and Bob through the relay is told busy;
//! 5. a deploy (the server going away and back on its port): the board
//!    comes back by itself (the time printed, never gated: it is wall time);
//! 6. Cloud relay off over USB: the board leaves the hub and says `off`; on
//!    again, it comes back;
//! 7. the account's key reset on the server, then a deploy: the board is
//!    refused, `refused: unknownAccount`.
//!
//! Every wait is a **safety net with a wall clock**, never an input: the
//! forward and the uplink are host sockets. Times printed are wall seconds
//! on this host, on `lp-emu:esp32c6:t1+net=lan` — never a gate.
//!
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`, or
//! `LP_CI_IMAGES`); `just test-emu-serve` runs it beside the LAN cells.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lp_cli::client::cli_connect::{CliConnection, cli_connect_with_password, connect_relay};
use lp_cli::commands::upload::{UploadArgs, handle_upload};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::transport_lan::{LanError, LanOptions, LanTarget, connect_lan_transport};
use lpa_client::{ClientEvent, HostSpecifier, LpClient};
use lpc_access::{Tier, link_psk};
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

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-serve` runs it"]
fn an_emulated_c6_reaches_lightplayer_app_through_the_lans_uplink() {
    let Some(elf) = image() else { return };
    let mut cloud = Cloud::start(None);
    let alice = cloud.account("alice");
    let bob = cloud.account("bob");
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("virtual_lan.toml");
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
    run(async {
        let connection = connect(&usb).await;
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
    });
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

    // The gate case (plan A3): projects/test/basic loaded, the relay
    // registered and a session open through it.
    run(async {
        let relayed = connect_relay(cloud.target(board_id), Some(alice.session.clone()), None)
            .await
            .expect("Alice through the relay");
        let connection = connect(&usb).await;
        let mut client = LpClient::new(connection.client_io());
        heap(
            &mut client,
            "projects/test/basic loaded, relay registered, a relay session open",
        )
        .await;
        drop(client);
        connection.close().await;
        let mut through = LpClient::new(relayed.client_io());
        request_rtt(&mut through).await;
        drop(through);
        relayed.close().await;
    });

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
        let connection = connect(&usb).await;
        let mut client = LpClient::new(connection.client_io());
        heap(
            &mut client,
            "projects/test/basic loaded, relay registered, a LAN session open",
        )
        .await;
        drop(client);
        connection.close().await;
        drop(local);
    });

    // 5. A deploy: the board comes back by itself. The wall clock is only
    // the net here (AGENTS.md: never gate on host wall-clock from an
    // emulated run); the 15 s bound is the relay client's, pinned in
    // `lpc-relay`'s rules and on the host in `relay_link.rs`.
    let restarted = Instant::now();
    cloud.restart();
    assert!(
        cloud.boards_within(1, RELAY_NET),
        "the board did not come back after the deploy\n{}",
        board.output()
    );
    eprintln!(
        "emu_relay_link: back {:.1} s wall after the deploy (lp-emu:esp32c6:t1+net=lan; not a gate)",
        restarted.elapsed().as_secs_f64()
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

/// The board's own heap figures from its next heartbeat (over USB), printed
/// with `what`: free bytes and the largest free block. Never a gate.
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
    let at =|p: usize| ms[(ms.len() * p / 100).min(ms.len() - 1)];
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

/// `lp-cli emu run --lan`, killed when the test is done with it, with its
/// output kept for the failure messages and read for the forward's line.
struct EmulatedBoard {
    child: Child,
    output: Arc<Mutex<String>>,
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
