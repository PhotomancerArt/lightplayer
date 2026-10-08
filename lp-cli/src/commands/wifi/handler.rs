//! `lp-cli wifi status|scan|add|forget|set <host>`: a board's Wi-Fi
//! networks over its link, at the edit tier.
//!
//! A password never rides argv: it comes from `LP_WIFI_PASSWORD` or one
//! line of stdin (`--password-stdin`), and nothing here prints it — the
//! board's reply has none to print.

use std::io::BufRead;

use anyhow::{Context, Result, bail};
use lpa_client::{ClientError, HostSpecifier, LpClient};
use lpc_access::NetworkFileError;
use lpc_wire::WifiPassword;
use lpc_wire::server::{
    ConnectStep, LastAttempt, NetworkScan, NetworkStatus, RelayRefusal, RelayState,
    SavedNetworkInfo, StationFailure, StationState,
};

use lpa_client::transport_lan::LanError;

use crate::client::board_password::{BOARD_PASSWORD_ENV, BoardPasswordArgs};
use crate::client::cli_connect::{cli_connect_with_password, stderr_device_events};

use super::args::{AddArgs, WifiCli, WifiCommand};

/// The environment variable a password may come from.
pub const PASSWORD_ENV: &str = "LP_WIFI_PASSWORD";

/// How many times `scan` asks again while the board says `scanning` (its
/// radio is listening; a scan takes about two seconds), one second apart.
const SCAN_ASKS: u32 = 6;

pub fn handle_wifi(cli: WifiCli) -> Result<()> {
    // Device connections are single-actor (`!Send`), as in `upload`.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(handle_wifi_async(cli)))
}

async fn handle_wifi_async(cli: WifiCli) -> Result<()> {
    match cli.command {
        WifiCommand::Status(args) => {
            run(&args.host, args.board_password, args.json, Op::Status).await
        }
        WifiCommand::Scan(args) => run(&args.host, args.board_password, args.json, Op::Scan).await,
        WifiCommand::Forget(args) => {
            let op = Op::Forget(args.ssid);
            run(&args.host, args.board_password, args.json, op).await
        }
        WifiCommand::Set(args) => {
            if args.wifi.is_none() && args.cloud_relay.is_none() {
                bail!("nothing to set: give --wifi on|off, --cloud-relay on|off, or both");
            }
            let op = Op::Set {
                wifi: args.wifi.map(|on| on.is_on()),
                cloud_relay: args.cloud_relay.map(|on| on.is_on()),
            };
            run(&args.host, args.board_password, args.json, op).await
        }
        WifiCommand::Add(args) => {
            let password = password_for(&args)?;
            let op = Op::Add {
                ssid: args.ssid,
                password,
                hidden: args.hidden.then_some(true),
            };
            // stdin is the network's password here: the board's comes from
            // LP_PASSWORD alone.
            run(&args.host, BoardPasswordArgs::default(), args.json, op).await
        }
    }
}

enum Op {
    Status,
    Scan,
    Add {
        ssid: String,
        password: WifiPassword,
        hidden: Option<bool>,
    },
    Forget(String),
    Set {
        wifi: Option<bool>,
        cloud_relay: Option<bool>,
    },
}

/// What the board answered: its status, or a scan.
enum Reply {
    Status(NetworkStatus),
    Scan(NetworkScan),
}

async fn run(host: &str, board_password: BoardPasswordArgs, json: bool, op: Op) -> Result<()> {
    let host_spec = HostSpecifier::parse(host).with_context(|| {
        format!(
            "Failed to parse host specifier: {host}. Examples: serial:auto, \
             serial:/dev/cu.usbmodem2101, lan:192.168.1.40"
        )
    })?;
    let stdin_is_the_networks = matches!(op, Op::Add { .. });
    let password = board_password.resolve(&host_spec)?;
    let connection = cli_connect_with_password(host_spec, password, stderr_device_events(false))
        .await
        .map_err(|error| locked_words(error, stdin_is_the_networks))
        .context("Failed to connect to the device")?;
    let mut client = LpClient::new(connection.client_io());
    let result = request(&mut client, op).await;
    drop(client);
    connection.close().await;
    let lines = match result? {
        Reply::Status(status) if json => vec![lpc_wire::json::to_string(&status)?],
        Reply::Scan(scan) if json => vec![lpc_wire::json::to_string(&scan)?],
        Reply::Status(status) => status_lines(&status),
        Reply::Scan(scan) => scan_lines(&scan),
    };
    for line in lines {
        println!("{line}");
    }
    Ok(())
}

async fn request<Io: lpa_client::ClientIo>(client: &mut LpClient<Io>, op: Op) -> Result<Reply> {
    let status = match op {
        Op::Scan => {
            // A board answers from what its radio last heard; a stale list
            // answers `scanning` while it listens. Ask again a few times.
            let mut scan = client.network_scan().await.map_err(worded)?.value;
            for _ in 1..SCAN_ASKS {
                if scan != NetworkScan::Scanning {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                scan = client.network_scan().await.map_err(worded)?.value;
            }
            return Ok(Reply::Scan(scan));
        }
        Op::Status => client.network_status().await,
        Op::Add {
            ssid,
            password,
            hidden,
        } => client.network_add(ssid, password, hidden).await,
        Op::Forget(ssid) => client.network_forget(ssid).await,
        Op::Set { wifi, cloud_relay } => client.network_set(wifi, cloud_relay).await,
    };
    status
        .map(|outcome| Reply::Status(outcome.value))
        .map_err(worded)
}

/// The board sends a bare error code (cheap on the device: see
/// `NetworkFileError`'s `Display`); turn it into words here, the same way
/// Studio does, so `lp-cli` never prints a raw code to a user who cannot
/// act on it.
fn worded(error: ClientError) -> anyhow::Error {
    match error {
        ClientError::Server(message) => {
            anyhow::anyhow!(
                "server error: {}",
                NetworkFileError::reword_refusal(&message)
            )
        }
        other => anyhow::anyhow!("{other}"),
    }
}

/// A locked board's refusal, in words that fit this command: on `add`,
/// stdin carries the network's password, so the board's can only come from
/// `LP_PASSWORD`.
fn locked_words(error: anyhow::Error, stdin_is_the_networks: bool) -> anyhow::Error {
    match error.downcast_ref::<LanError>() {
        Some(LanError::Locked) if stdin_is_the_networks => anyhow::anyhow!(
            "this board is locked: give its password in {BOARD_PASSWORD_ENV} (stdin carries the \
             network's password here)"
        ),
        _ => error,
    }
}

/// Where the password comes from: `--open` (none), `--password-stdin` (one
/// line), or `LP_WIFI_PASSWORD` — never argv. With none of them, the add
/// is refused before anything is sent.
fn password_for(args: &AddArgs) -> Result<WifiPassword> {
    if args.open {
        return Ok(WifiPassword::new(""));
    }
    if args.password_stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("reading the password from stdin")?;
        let password = line.trim_end_matches(['\n', '\r']);
        return Ok(WifiPassword::new(password));
    }
    if let Ok(password) = std::env::var(PASSWORD_ENV) {
        return Ok(WifiPassword::new(password));
    }
    bail!(
        "a network needs its password: give it in {PASSWORD_ENV}, with --password-stdin, \
         or pass --open for an open network"
    )
}

/// The status in plain words: the switches, one line per saved network,
/// the station, the cloud relay.
pub fn status_lines(status: &NetworkStatus) -> Vec<String> {
    let mut lines = vec![
        format!("wifi: {}", on_off(status.wifi)),
        format!("cloud relay: {}", on_off(status.cloud_relay)),
    ];
    if status.networks.is_empty() {
        lines.push(String::from("networks: none saved"));
    }
    lines.extend(status.networks.iter().map(network_line));
    lines.push(format!("station: {}", station_words(&status.station)));
    let joined = matches!(status.station, StationState::Connected { .. });
    lines.push(format!("relay: {}", relay_words(status.relay, joined)));
    lines
}

/// The cloud relay's state in the words Studio's Wi-Fi popover uses
/// (`lpa-studio-core`'s `wifi_words::relay`), lower-cased for a line. Not
/// reaching lightplayer.app is "no internet" only once the station is
/// `joined`; before that the board is waiting for its network.
fn relay_words(relay: RelayState, joined: bool) -> &'static str {
    match relay {
        RelayState::Off => "off",
        RelayState::NoAccount => {
            "no account key — sign in to Studio and plug this board in once to use lightplayer.app"
        }
        RelayState::WaitingForInternet if joined => {
            "connected, no internet — lightplayer.app didn't answer"
        }
        RelayState::WaitingForInternet => "waiting for a network",
        RelayState::Connecting => "reaching lightplayer.app…",
        RelayState::Connected => "connected to lightplayer.app",
        RelayState::Refused {
            reason: RelayRefusal::UnknownAccount,
        } => "refused — plug this board into Studio once to refresh its account",
        RelayState::Refused {
            reason: RelayRefusal::UpdateFirmware,
        } => "refused — update this board's firmware to use lightplayer.app",
        RelayState::Refused {
            reason: RelayRefusal::Busy,
        } => "lightplayer.app is busy; the board tries again by itself",
    }
}

/// A scan in plain words, one line per network heard.
pub fn scan_lines(scan: &NetworkScan) -> Vec<String> {
    match scan {
        NetworkScan::Unsupported => {
            vec![String::from("scan: this firmware can't scan for Wi-Fi yet")]
        }
        NetworkScan::Scanning => {
            vec![String::from(
                "scan: the board is still listening; ask again",
            )]
        }
        NetworkScan::Heard(heard) if heard.is_empty() => {
            vec![String::from("scan: no networks heard")]
        }
        NetworkScan::Heard(heard) => heard
            .iter()
            .map(|network| {
                format!(
                    "heard: {} · {} dBm{}",
                    network.ssid,
                    network.rssi,
                    if network.secure { "" } else { " · open" }
                )
            })
            .collect(),
    }
}

fn network_line(network: &SavedNetworkInfo) -> String {
    let mut facts = vec![if network.has_password {
        "password set"
    } else {
        "open"
    }];
    if network.hidden {
        facts.push("hidden");
    }
    if let Some(last) = network.last {
        facts.push(match last {
            LastAttempt::Connected => "last connected",
            LastAttempt::WrongPassword => "wrong password",
            LastAttempt::NotFound => "not in range",
            LastAttempt::NoAddress => "got no address",
        });
    }
    format!("network: {} ({})", network.ssid, facts.join(", "))
}

fn station_words(station: &StationState) -> String {
    match station {
        StationState::Unsupported => String::from("this firmware can't connect to Wi-Fi yet"),
        StationState::Off => String::from("off"),
        StationState::NotConnected => String::from("not connected"),
        StationState::Connecting { ssid, step } => format!(
            "connecting to {ssid}: {}",
            match step {
                ConnectStep::Looking => "looking for it",
                ConnectStep::CheckingPassword => "checking the password",
                ConnectStep::GettingAddress => "getting an address",
            }
        ),
        StationState::Connected {
            ssid,
            ip,
            rssi,
            host,
        } => {
            format!("connected to {ssid} · {ip} ({host}) · {rssi} dBm")
        }
        StationState::Failed { ssid, reason } => format!(
            "couldn't connect to {ssid}: {}",
            match reason {
                StationFailure::WrongPassword => "wrong password",
                StationFailure::NotFound => "not in range",
                StationFailure::NoAddress => "got no address",
            }
        ),
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::HeardNetwork;

    fn network(ssid: &str, has_password: bool) -> SavedNetworkInfo {
        SavedNetworkInfo {
            ssid: String::from(ssid),
            has_password,
            hidden: false,
            last: None,
        }
    }

    #[test]
    fn saved_networks_read_in_plain_words() {
        let status = NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks: vec![
                network("lp-walk-net", true),
                SavedNetworkInfo {
                    hidden: true,
                    last: Some(LastAttempt::WrongPassword),
                    ..network("lp-back-office", false)
                },
            ],
            station: StationState::Unsupported,
            relay: RelayState::Off,
        };
        assert_eq!(
            status_lines(&status),
            [
                "wifi: on",
                "cloud relay: on",
                "network: lp-walk-net (password set)",
                "network: lp-back-office (open, hidden, wrong password)",
                "station: this firmware can't connect to Wi-Fi yet",
                "relay: off",
            ]
        );
    }

    #[test]
    fn every_relay_state_reads_in_plain_words() {
        let mut status = NetworkStatus {
            wifi: true,
            cloud_relay: true,
            networks: Vec::new(),
            station: StationState::NotConnected,
            relay: RelayState::WaitingForInternet,
        };
        assert_eq!(
            status_lines(&status).last().map(String::as_str),
            Some("relay: waiting for a network"),
            "not reaching lightplayer.app is no internet only once joined"
        );
        status.station = StationState::Connected {
            ssid: String::from("lp-walk-net"),
            ip: String::from("10.0.0.7"),
            rssi: -48,
            host: String::from("lp-8e30.local"),
        };
        let cases = [
            (RelayState::Connecting, "relay: reaching lightplayer.app…"),
            (RelayState::Connected, "relay: connected to lightplayer.app"),
            (
                RelayState::WaitingForInternet,
                "relay: connected, no internet — lightplayer.app didn't answer",
            ),
            (
                RelayState::NoAccount,
                "relay: no account key — sign in to Studio and plug this board in once to use \
                 lightplayer.app",
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UnknownAccount,
                },
                "relay: refused — plug this board into Studio once to refresh its account",
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::UpdateFirmware,
                },
                "relay: refused — update this board's firmware to use lightplayer.app",
            ),
            (
                RelayState::Refused {
                    reason: RelayRefusal::Busy,
                },
                "relay: lightplayer.app is busy; the board tries again by itself",
            ),
        ];
        for (relay, line) in cases {
            status.relay = relay;
            assert_eq!(status_lines(&status).last().map(String::as_str), Some(line));
        }
    }

    #[test]
    fn no_network_and_every_station_state() {
        let mut status = NetworkStatus {
            wifi: false,
            cloud_relay: false,
            networks: Vec::new(),
            station: StationState::Off,
            relay: RelayState::Off,
        };
        assert_eq!(
            status_lines(&status),
            [
                "wifi: off",
                "cloud relay: off",
                "networks: none saved",
                "station: off",
                "relay: off"
            ]
        );
        let cases = [
            (StationState::NotConnected, "not connected"),
            (
                StationState::Connecting {
                    ssid: String::from("lp-walk-net"),
                    step: ConnectStep::Looking,
                },
                "connecting to lp-walk-net: looking for it",
            ),
            (
                StationState::Connecting {
                    ssid: String::from("lp-walk-net"),
                    step: ConnectStep::GettingAddress,
                },
                "connecting to lp-walk-net: getting an address",
            ),
            (
                StationState::Connected {
                    ssid: String::from("lp-walk-net"),
                    ip: String::from("10.0.0.7"),
                    rssi: -48,
                    host: "lp-8e30.local".to_string(),
                },
                "connected to lp-walk-net · 10.0.0.7 (lp-8e30.local) · -48 dBm",
            ),
            (
                StationState::Failed {
                    ssid: String::from("lp-walk-net"),
                    reason: StationFailure::WrongPassword,
                },
                "couldn't connect to lp-walk-net: wrong password",
            ),
        ];
        for (station, words) in cases {
            status.station = station;
            assert_eq!(status_lines(&status)[3], format!("station: {words}"));
        }
    }

    #[test]
    fn a_scan_reads_in_plain_words() {
        assert_eq!(
            scan_lines(&NetworkScan::Unsupported),
            ["scan: this firmware can't scan for Wi-Fi yet"]
        );
        assert_eq!(
            scan_lines(&NetworkScan::Scanning),
            ["scan: the board is still listening; ask again"]
        );
        assert_eq!(
            scan_lines(&NetworkScan::Heard(vec![HeardNetwork {
                ssid: String::from("lp-cafe"),
                rssi: -70,
                secure: false,
            }])),
            ["heard: lp-cafe · -70 dBm · open"]
        );
    }
}
