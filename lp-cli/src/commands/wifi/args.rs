use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "wifi",
    about = "A board's Wi-Fi settings: status, set, forget (edit access).",
    long_about = "A board's Wi-Fi settings, kept on the board in /.lp/network.json.\n\n\
        The password is write-only: no link reads it back, and this command never \
        takes it on the command line (shell history and `ps` would keep it). Give it \
        in LP_WIFI_PASSWORD or on stdin with --password-stdin; --open saves an open \
        network.\n\n\
        No firmware joins Wi-Fi yet: a board stores the settings and says so."
)]
pub struct WifiCli {
    #[command(subcommand)]
    pub command: WifiCommand,
}

#[derive(Debug, Subcommand)]
pub enum WifiCommand {
    /// The saved network (without its password), LAN only, and the station.
    Status(StatusArgs),
    /// Save or change the network; an absent option is left as it is.
    Set(SetArgs),
    /// Forget the saved network and its password (LAN only stays).
    Forget(StatusArgs),
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    /// The device, e.g. serial:auto, serial:/dev/cu.usbmodem2101 or
    /// serial:tcp://127.0.0.1:5591 (an emulated board).
    pub host: String,
    /// Print the board's reply as JSON (it holds no password).
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct SetArgs {
    /// The device, e.g. serial:auto or serial:tcp://127.0.0.1:5591.
    pub host: String,
    /// The network name (1-32 bytes). A new name needs its password too:
    /// LP_WIFI_PASSWORD, --password-stdin, or --open.
    #[arg(long)]
    pub ssid: Option<String>,
    /// Read the password from one line of stdin (never from argv). Without
    /// --ssid it changes the saved network's password.
    #[arg(long, conflicts_with = "open")]
    pub password_stdin: bool,
    /// The network is open: no password.
    #[arg(long)]
    pub open: bool,
    /// Join the network when on (kept but unused when off).
    #[arg(long, value_enum)]
    pub enabled: Option<OnOff>,
    /// Never use the relay (it is used by default).
    #[arg(long, value_enum)]
    pub lan_only: Option<OnOff>,
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
