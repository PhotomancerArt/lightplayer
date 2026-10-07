use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "lan",
    about = "Boards on this network: `lan list` finds LightPlayer boards on the LAN.",
    long_about = "Boards on the local network (Wi-Fi). A board that has joined a network \
        announces itself over mDNS / DNS-SD as `_lightplayer._tcp`; `lan list` asks \
        for those announcements and prints what answers."
)]
pub struct LanCli {
    #[command(subcommand)]
    pub command: LanCommand,
}

#[derive(Debug, Subcommand)]
pub enum LanCommand {
    // About and examples live on `ListArgs`, so `--help` shows them whole.
    List(ListArgs),
}

#[derive(Debug, Args)]
#[command(
    about = "List the LightPlayer boards on the LAN.",
    long_about = "Ask the local network which LightPlayer boards are there (DNS-SD \
        `_lightplayer._tcp.local`, one question, repeated halfway through the wait) \
        and print one line per board: its name, address, port, MAC, wire version and \
        a ready-to-use `lan:<ip>` specifier. Nothing is sent to a board itself.\n\n\
        No board answering is not an error: it says so on stderr and exits 0 with an \
        empty list.",
    after_help = "Examples:\n  \
        lp-cli lan list\n  \
        lp-cli lan list --wait 5\n  \
        lp-cli lan list --json\n\n\
        The printed `lan:<ip>` works wherever a device is named, e.g.:\n  \
        lp-cli upload projects/test/basic lan:192.168.4.100"
)]
pub struct ListArgs {
    /// How many seconds to listen for answers.
    #[arg(long, value_name = "SECONDS", default_value_t = 2.0)]
    pub wait: f64,
    /// Print a JSON array (name, instance, ip, port, mac, proto, path, spec) for scripts.
    #[arg(long)]
    pub json: bool,
}
