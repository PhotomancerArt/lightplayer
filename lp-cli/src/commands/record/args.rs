use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

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

    /// Print a recording as a timeline: one line per event, time relative
    /// to the session start, wire bytes decoded into messages.
    ///
    /// A stretch of more than two seconds in which nothing happened while a
    /// request waited on its answer is printed as a `… N s silent` line, and
    /// a request that never got an outcome is listed at the end.
    Timeline(TimelineArgs),
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

#[derive(Debug, Args)]
pub struct TimelineArgs {
    /// A recording (`.jsonl`), or a directory of them — its newest one, or
    /// every one with `--all`.
    pub path: PathBuf,

    /// How to show device bytes: `frames` reassembles each stream and
    /// decodes its messages (JSON Pack and `M!` lines), `raw` shows each
    /// chunk's size and first bytes, `off` hides them.
    #[arg(long, value_enum, default_value_t = WireView::Frames)]
    pub wire: WireView,

    /// Skip everything before this many seconds after the session start.
    #[arg(long)]
    pub since: Option<f64>,

    /// Only these record kinds, comma-separated (e.g. `route,error,request`).
    #[arg(long)]
    pub kinds: Option<String>,

    /// With a directory: every recording in it, oldest name first.
    #[arg(long)]
    pub all: bool,
}

/// `record timeline --wire`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum WireView {
    Frames,
    Raw,
    Off,
}
