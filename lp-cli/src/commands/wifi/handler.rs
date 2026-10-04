//! `lp-cli wifi status|set|forget <host>`: a board's Wi-Fi settings over
//! its link, at the edit tier.
//!
//! The password never rides argv: it comes from `LP_WIFI_PASSWORD` (when
//! `--ssid` names a network) or one line of stdin (`--password-stdin`), and
//! nothing here prints it — the board's reply has none to print.

use std::io::BufRead;

use anyhow::{Context, Result, bail};
use lpa_client::{HostSpecifier, LpClient};
use lpa_server::network_store::NEW_NETWORK_NEEDS_PASSWORD;
use lpc_wire::WifiPassword;
use lpc_wire::server::{NetworkStatus, StationState};

use crate::client::cli_connect::{cli_connect, stderr_device_events};

use super::args::{SetArgs, WifiCli, WifiCommand};

/// The environment variable a password may come from.
pub const PASSWORD_ENV: &str = "LP_WIFI_PASSWORD";

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
        WifiCommand::Status(args) => run(&args.host, args.json, Op::Status).await,
        WifiCommand::Forget(args) => run(&args.host, args.json, Op::Forget).await,
        WifiCommand::Set(args) => {
            let password = password_for(&args)?;
            let host = args.host.clone();
            let json = args.json;
            run(&host, json, Op::Set { args, password }).await
        }
    }
}

enum Op {
    Status,
    Set {
        args: SetArgs,
        password: Option<WifiPassword>,
    },
    Forget,
}

async fn run(host: &str, json: bool, op: Op) -> Result<()> {
    let host_spec = HostSpecifier::parse(host).with_context(|| {
        format!(
            "Failed to parse host specifier: {host}. Examples: serial:auto, serial:/dev/cu.usbmodem2101"
        )
    })?;
    let connection = cli_connect(host_spec, stderr_device_events(false))
        .await
        .context("Failed to connect to the device")?;
    let mut client = LpClient::new(connection.client_io());
    let result = request(&mut client, op).await;
    drop(client);
    connection.close().await;
    let status = result?;
    if json {
        println!("{}", lpc_wire::json::to_string(&status)?);
    } else {
        for line in status_lines(&status) {
            println!("{line}");
        }
    }
    Ok(())
}

async fn request<Io: lpa_client::ClientIo>(
    client: &mut LpClient<Io>,
    op: Op,
) -> Result<NetworkStatus> {
    let outcome = match op {
        Op::Status => client.network_status().await,
        Op::Forget => client.network_forget().await,
        Op::Set { args, password } => {
            // A new name with no password is refused here, in the board's
            // words, before anything is sent: the saved name may be this
            // one (then the password stays), so the board is asked first.
            if let (Some(ssid), None) = (&args.ssid, &password) {
                let saved = client
                    .network_status()
                    .await
                    .map_err(|error| anyhow::anyhow!("{error}"))?
                    .value;
                if saved.wifi.as_ref().is_none_or(|wifi| &wifi.ssid != ssid) {
                    bail!(
                        "{NEW_NETWORK_NEEDS_PASSWORD}: give it in {PASSWORD_ENV}, \
                         with --password-stdin, or pass --open"
                    );
                }
            }
            client
                .network_set(
                    args.ssid,
                    password,
                    args.enabled.map(|on| on.is_on()),
                    args.lan_only.map(|on| on.is_on()),
                )
                .await
        }
    };
    outcome
        .map(|outcome| outcome.value)
        .map_err(|error| anyhow::anyhow!("{error}"))
}

/// Where the password comes from: `--open` (none), `--password-stdin` (one
/// line), or `LP_WIFI_PASSWORD` when `--ssid` names a network — never argv.
fn password_for(args: &SetArgs) -> Result<Option<WifiPassword>> {
    if args.open {
        return Ok(Some(WifiPassword::new("")));
    }
    if args.password_stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .context("reading the password from stdin")?;
        let password = line.trim_end_matches(['\n', '\r']);
        return Ok(Some(WifiPassword::new(password)));
    }
    if args.ssid.is_some()
        && let Ok(password) = std::env::var(PASSWORD_ENV)
    {
        return Ok(Some(WifiPassword::new(password)));
    }
    Ok(None)
}

/// The status in plain words, one setting per line.
pub fn status_lines(status: &NetworkStatus) -> Vec<String> {
    let network = match &status.wifi {
        None => String::from("network: not set"),
        Some(wifi) => format!(
            "network: {} ({}, {})",
            wifi.ssid,
            if wifi.has_password {
                "password set"
            } else {
                "open"
            },
            if wifi.enabled { "on" } else { "off" }
        ),
    };
    let lan_only = format!("lan only: {}", if status.lan_only { "on" } else { "off" });
    let station = format!("station: {}", station_words(&status.station));
    vec![network, lan_only, station]
}

fn station_words(station: &StationState) -> String {
    match station {
        StationState::Unsupported => String::from("this firmware doesn't join Wi-Fi yet"),
        StationState::Off => String::from("off"),
        StationState::Joining => String::from("joining"),
        StationState::Joined { ip, rssi } => format!("joined · {ip} · {rssi} dBm"),
        StationState::Failed { reason } => format!("couldn't join: {reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::WifiInfo;

    #[test]
    fn a_saved_network_reads_in_plain_words() {
        let status = NetworkStatus {
            wifi: Some(WifiInfo {
                ssid: String::from("lp-walk-net"),
                has_password: true,
                enabled: true,
            }),
            lan_only: false,
            station: StationState::Unsupported,
        };
        assert_eq!(
            status_lines(&status),
            [
                "network: lp-walk-net (password set, on)",
                "lan only: off",
                "station: this firmware doesn't join Wi-Fi yet",
            ]
        );
    }

    #[test]
    fn no_network_and_every_station_state() {
        let mut status = NetworkStatus {
            wifi: None,
            lan_only: true,
            station: StationState::Off,
        };
        assert_eq!(status_lines(&status)[0], "network: not set");
        assert_eq!(status_lines(&status)[1], "lan only: on");
        status.station = StationState::Joined {
            ip: String::from("10.0.0.7"),
            rssi: -48,
        };
        assert_eq!(status_lines(&status)[2], "station: joined · 10.0.0.7 · -48 dBm");
        status.station = StationState::Failed {
            reason: String::from("wrong password"),
        };
        assert_eq!(
            status_lines(&status)[2],
            "station: couldn't join: wrong password"
        );
    }
}
