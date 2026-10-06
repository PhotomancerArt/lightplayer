//! lp-cli's `lan:` link against an EMULATED board on a virtual LAN (Wi-Fi
//! plan P13, the CI cell): the shipped C6 image under `lp-cli emu run --lan`,
//! its network seam answered by a virtual LAN with one access point, and
//! the board reached through its port forward exactly as `lp-cli … lan:<ip>`
//! reaches a board on a desk.
//!
//! `lan_link.rs` proves the same path against the firmware's LAN code built
//! for the host. This one proves it against the image a board is flashed
//! with, joined to a network by its own station:
//!
//! - over the board's USB link, the fixture's network is added; the board's
//!   own status then says `connected`, with an address and its `.local`
//!   name;
//! - over the forward (`lan:127.0.0.1:<port>`), the secure link comes up and
//!   the board says hello on it, never trusted (a LAN link is keyed);
//! - `wifi status` over the LAN answers with the address the USB link
//!   reported (the same board, reached the other way);
//! - an upload over the LAN loads and runs `projects/test/basic`.
//!
//! Test values only (`lp-walk-net` / `correct-horse-42`), never a real
//! network's. Every wait here is a **safety net with a wall clock**, never
//! an input: the forward is a host socket (`lp-emu/esp/README.md`
//! §Determinism), so the test waits for an outcome and gives up after a
//! generous while. Nothing asserts how long anything took.
//!
//! In lp-cli because the link host is a product crate (the MIT fence).
//! `#[ignore]`d: it needs a built `fw-esp32c6` ELF (`LP_EMU_BUILD_FW=1`, or
//! `LP_CI_IMAGES`); `just test-emu-c6-cli` runs it. Figures it prints are
//! `lp-emu:esp32c6:t1+net=lan`.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lp_cli::client::cli_connect::{CliConnection, cli_connect_with_password};
use lp_cli::commands::upload::{UploadArgs, handle_upload};
use lp_cli::commands::wifi::WifiCli;
use lp_cli::commands::wifi::args::{HostArgs, WifiCommand};
use lp_emu_esp32c6::test_support::{FwImage, fw_esp32c6_image};
use lpa_client::{HostSpecifier, LpClient};
use lpc_wire::WifiPassword;
use lpc_wire::server::{NetworkStatus, StationState};

const SSID: &str = "lp-walk-net";
const PASSWORD: &str = "correct-horse-42";

/// The virtual LAN the board joins: one access point, test values only.
///
/// `emu run --lan <fixture>` reads P11's fixture format
/// (`lp-emu/esp/lp-emu-esp-common/testdata/virtual_lan.toml`: one
/// `[[access_point]]` per network, `name`, `password`, `signal_dbm`;
/// `lp-cli/src/commands/emu/lan_fixture.rs`).
const FIXTURE: &str = r#"# emu_lan_link.rs: made-up test values only.
[[access_point]]
name = "lp-walk-net"
password = "correct-horse-42"
signal_dbm = -50
"#;

/// The wall-clock net on the board joining (an emulated boot, a scan, the
/// join and DHCP, all at emulated speed on a loaded box).
const JOIN_NET: Duration = Duration::from_secs(420);

/// The wall-clock net on the forward's line appearing in `emu run`'s output.
const FORWARD_NET: Duration = Duration::from_secs(120);

/// How often to ask the board's station again while it joins.
const STATUS_POLL: Duration = Duration::from_secs(2);

#[test]
#[ignore = "needs a built fw-esp32c6 ELF; `just test-emu-c6-cli` runs it"]
fn lan_transport_reaches_an_emulated_board_through_its_forward() {
    let Some(elf) = image() else { return };
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("virtual_lan.toml");
    std::fs::write(&fixture, FIXTURE).unwrap();
    let usb = free_addr();
    let board = EmulatedBoard::start(&elf, &usb, &fixture);
    wait_listening(&usb);

    // 1. Over USB: add the network, then wait for the board's own status
    //    to say it joined.
    let usb_status = run(async {
        let connection = connect(&format!("serial:tcp://{usb}")).await;
        let mut client = LpClient::new(connection.client_io());
        let added = client
            .network_add(String::from(SSID), WifiPassword::new(PASSWORD), None)
            .await
            .expect("`wifi add` over USB")
            .value;
        assert_eq!(added.networks.len(), 1, "{added:?}");
        assert_eq!(added.networks[0].ssid, SSID);
        assert!(added.networks[0].has_password);
        let started = Instant::now();
        let status = loop {
            let status = client
                .network_status()
                .await
                .expect("`wifi status` over USB")
                .value;
            if matches!(status.station, StationState::Connected { .. }) {
                break status;
            }
            assert!(
                !matches!(status.station, StationState::Failed { .. }),
                "the board failed to join the fixture's network: {:?}\n{}",
                status.station,
                board.output()
            );
            assert!(
                started.elapsed() < JOIN_NET,
                "the board never joined within {JOIN_NET:?}; last {:?}\n{}",
                status.station,
                board.output()
            );
            tokio::time::sleep(STATUS_POLL).await;
        };
        drop(client);
        connection.close().await;
        status
    });
    let (ip, host) = connected(&usb_status);
    assert!(
        host.starts_with("lp-") && host.ends_with(".local"),
        "the board's LAN name: {host}"
    );
    eprintln!("emu_lan_link: joined {SSID} as {ip} ({host}) (lp-emu:esp32c6:t1+net=lan)");

    // 2. Over the forward: the secure link and the board's hello on it.
    let lan = board.forward();
    run(async {
        let connection = connect(&lan).await;
        let hello = connection
            .hello()
            .expect("the board's hello on the LAN link");
        assert_eq!(hello.proto, lpc_wire::WIRE_PROTO_VERSION);
        assert!(hello.auth.required, "a LAN link is never trusted");
        assert!(
            hello.auth.granted.is_some(),
            "a fresh board is open: the anonymous key gets a tier ({:?})",
            hello.auth
        );
        // The same board, reached the other way: the address it reported
        // over USB is the one it reports over the LAN.
        let mut client = LpClient::new(connection.client_io());
        let lan_status = client
            .network_status()
            .await
            .expect("`wifi status` over the LAN (edit, on an open board)")
            .value;
        assert_eq!(connected(&lan_status), (ip.clone(), host.clone()));
        drop(client);
        connection.close().await;
    });

    // 3. The commands themselves, end to end over `lan:`.
    lp_cli::commands::wifi::handle_wifi(WifiCli {
        command: WifiCommand::Status(HostArgs {
            host: lan.clone(),
            board_password: Default::default(),
            json: true,
        }),
    })
    .expect("`lp-cli wifi status lan:…` answers");
    handle_upload(UploadArgs {
        dir: workspace_dir().join("projects/test/basic"),
        host: lan.clone(),
        password: Default::default(),
        no_wait: false,
        wait_timeout_secs: 300,
    })
    .expect("`lp-cli upload projects/test/basic lan:…` deploys and the project runs");
    eprintln!("emu_lan_link: hello, status and an upload over {lan}");
}

/// The forward's line is read whatever words surround it (the words are
/// `emu run`'s to change). This one needs no firmware.
#[test]
fn the_forward_is_read_out_of_whatever_line_names_it() {
    assert_eq!(
        forward_in("emu run: board on lan `fixture` · forward lan:127.0.0.1:28111 → :80"),
        Some("lan:127.0.0.1:28111".to_string())
    );
    assert_eq!(forward_in("lan:127.0.0.1:"), None);
    assert_eq!(forward_in("nothing here"), None);
}

/// The address and `.local` name a `connected` station reports.
fn connected(status: &NetworkStatus) -> (String, String) {
    match &status.station {
        StationState::Connected { ssid, ip, host, .. } => {
            assert_eq!(ssid, SSID, "joined the fixture's network");
            (ip.clone(), host.clone())
        }
        other => panic!("the station is not connected: {other:?}"),
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
    fn start(elf: &Path, usb: &str, fixture: &Path) -> Self {
        // `emu run --lan <fixture.toml>` (P12 §3) puts the board on a
        // virtual LAN with the fixture's access points and prints its
        // forward on stderr, once, right after the machine is built:
        // `emu: board on LAN <fixture> (…) · forward lan:127.0.0.1:<port> →
        // board :80`.
        let mut child = Command::new(env!("CARGO_BIN_EXE_lp-cli"))
            .args(["emu", "run", "--elf"])
            .arg(elf)
            .args(["--link", usb, "--lan"])
            .arg(fixture)
            .args([
                "--reboot-on-reset",
                "--timeout",
                "1800s",
                "--wall-timeout",
                "1800",
            ])
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

    /// The board's forward, `lan:127.0.0.1:<port>`, as `emu run` printed it.
    fn forward(&self) -> String {
        self.forward.recv_timeout(FORWARD_NET).unwrap_or_else(|_| {
            panic!(
                "`emu run --lan` never printed a `lan:127.0.0.1:<port>` forward:\n{}",
                self.output()
            )
        })
    }

    /// The last of what `emu run` said, for a failure message.
    fn output(&self) -> String {
        let text = self.output.lock().expect("output poisoned");
        let start = text.len().saturating_sub(4_000);
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

/// Keep reading `pipe` (or the child blocks on a full pipe), keep the text,
/// and send the first forward a line names.
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
            eprintln!("emu_lan_link: skipped — {reason}");
            None
        }
    }
}

/// A loopback address nothing is listening on right now.
fn free_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

/// Wait for the board's USB socket without connecting to it: a connect IS
/// the port's open, so a probe would be a first client the test is not.
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

/// Run `future` the way lp-cli's commands do: a current-thread runtime and
/// a `LocalSet` (the CLI connection is single-actor).
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
