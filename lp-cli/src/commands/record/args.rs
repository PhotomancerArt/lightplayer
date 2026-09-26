use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "record",
    about = "Receive and read Studio session recordings (`?record=`)."
)]
pub struct RecordCli {
    #[command(subcommand)]
    pub subcommand: RecordSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum RecordSubcommand {
    /// Listen for a Studio page opened with `?record=<url>` and write each
    /// page session to its own JSONL file.
    ///
    /// Prints the sink URL and the exact `?record=` query to add to a Studio
    /// address. Every page load is one session: its lines land in
    /// `<out>/<YYYYMMDD-HHMMSS>-<session>.jsonl`, created on its first batch.
    /// The recording is the whole session, device traffic included,
    /// unredacted.
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Port to listen on; 0 picks a free one (printed).
    #[arg(long, default_value_t = 0)]
    pub port: u16,

    /// Directory the session files are written to (created if missing).
    #[arg(long, default_value = "recordings")]
    pub out: PathBuf,

    /// Listen on every interface (0.0.0.0) instead of loopback, and print
    /// this machine's LAN address — for a page on another device. Studio
    /// only records to loopback or private-LAN hosts.
    #[arg(long)]
    pub lan: bool,
}
