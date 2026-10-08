use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::client::board_password::BoardPasswordArgs;

#[derive(Debug, Parser)]
#[command(
    name = "wifi",
    about = "A board's Wi-Fi networks: status, scan, add, forget, set (edit access).",
    long_about = "A board's Wi-Fi settings, kept on the board in /.lp/network.json: up to \
        eight saved networks, a Wi-Fi switch and a cloud relay switch.\n\n\
        A password is write-only: no link reads it back, and this command never \
        takes it on the command line (shell history and `ps` would keep it). Give it \
        in LP_WIFI_PASSWORD or on stdin with --password-stdin; --open saves an open \
        network. Adding a saved network again changes its password.\n\n\
        A locked board at a lan: address needs its own password: LP_PASSWORD, \
        or --password-stdin on status, scan, forget and set.\n\n\
        No firmware connects to Wi-Fi yet: a board stores the settings and says so, \
        and a scan answers that this firmware cannot scan."
)]
pub struct WifiCli {
    #[command(subcommand)]
    pub command: WifiCommand,
}

#[derive(Debug, Subcommand)]
pub enum WifiCommand {
    /// The switches, the saved networks (without their passwords), and the
    /// station.
    Status(HostArgs),
    /// What the board's radio hears (2.4 GHz only).
    Scan(HostArgs),
    /// Save a network, or change a saved network's password.
    Add(AddArgs),
    /// Forget a saved network and its password.
    Forget(ForgetArgs),
    /// Turn Wi-Fi or the cloud relay on or off; an absent option is left as
    /// it is.
    Set(SetArgs),
}

#[derive(Debug, Args)]
pub struct HostArgs {
    /// The device, e.g. serial:auto, serial:/dev/cu.usbmodem2101,
    /// serial:tcp://127.0.0.1:5591 (an emulated board) or lan:192.168.1.40
    /// (a board on the network).
    pub host: String,
    #[command(flatten)]
    pub board_password: BoardPasswordArgs,
    /// Print the board's reply as JSON (it holds no password).
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct AddArgs {
    /// The device, e.g. serial:auto or serial:tcp://127.0.0.1:5591. A
    /// locked board at a lan: address takes its own password from
    /// LP_PASSWORD (stdin here is the network's).
    pub host: String,
    /// The network name (1-32 bytes). Its password comes from
    /// LP_WIFI_PASSWORD or --password-stdin; --open for none.
    pub ssid: String,
    /// Read the password from one line of stdin (never from argv).
    #[arg(long, conflicts_with = "open")]
    pub password_stdin: bool,
    /// The network is open: no password.
    #[arg(long)]
    pub open: bool,
    /// The network does not broadcast its name: the board asks for it by
    /// name. Without it, a saved network keeps what it had.
    #[arg(long)]
    pub hidden: bool,
    /// Print the board's reply as JSON (it holds no password).
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ForgetArgs {
    /// The device, e.g. serial:auto, serial:tcp://127.0.0.1:5591 or
    /// lan:192.168.1.40.
    pub host: String,
    #[command(flatten)]
    pub board_password: BoardPasswordArgs,
    /// The saved network to forget.
    pub ssid: String,
    /// Print the board's reply as JSON (it holds no password).
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    /// The device, e.g. serial:auto, serial:tcp://127.0.0.1:5591 or
    /// lan:192.168.1.40.
    pub host: String,
    #[command(flatten)]
    pub board_password: BoardPasswordArgs,
    /// The board's Wi-Fi switch.
    #[arg(long, value_enum)]
    pub wifi: Option<OnOff>,
    /// Let lightplayer.app reach this board through the cloud relay (on by
    /// default). On, a joined board that holds an account key dials
    /// lightplayer.app by itself and `wifi status` says how far it got; off,
    /// it never dials.
    #[arg(long, value_enum)]
    pub cloud_relay: Option<OnOff>,
    /// Print the board's reply as JSON (it holds no password).
    #[arg(long)]
    pub json: bool,
}

/// A switch on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OnOff {
    On,
    Off,
}

impl OnOff {
    pub fn is_on(self) -> bool {
        self == Self::On
    }
}
