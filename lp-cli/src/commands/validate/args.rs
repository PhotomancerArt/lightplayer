use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ValidateCli {
    #[command(subcommand)]
    pub command: ValidateCommand,

    /// Repository root. Defaults to the workspace root found by walking up
    /// from the current directory.
    #[arg(long, global = true)]
    pub repo_root: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum ValidateCommand {
    /// List payloads, sets, configurations and committed transcripts.
    List,
    /// Replay one transcript against another, or against a configuration.
    Replay(ReplayArgs),
    /// Run a set of payloads on a configuration.
    Run(RunArgs),
    /// Run a set and write each capture into its committed location.
    Record(RecordArgs),
}

#[derive(Debug, Args)]
pub struct ReplayArgs {
    /// The transcript under test. Its ratios are computed as this side
    /// divided by the `--against` side.
    pub transcript: PathBuf,
    /// A transcript path, or a configuration name whose committed transcript
    /// of the same payload and chip should be used.
    #[arg(long)]
    pub against: String,
    /// Refuse any field class either side grades below `measured`.
    #[arg(long)]
    pub strict: bool,
    /// Treat timing divergence as a failure. Off by default: no host gate runs
    /// on emulated microseconds (plan PD9).
    #[arg(long)]
    pub strict_timing: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Set name, for example `compile-parity`.
    pub set: String,
    /// Configuration name, for example `silicon:esp32c6`.
    #[arg(long = "config")]
    pub configuration: String,
    /// Serial port, for silicon configurations. Never defaulted: resolve it
    /// with `lp-cli fwcheck port --chip esp32c6`.
    #[arg(long)]
    pub port: Option<String>,
    /// An already-built image to run, for the emulated configurations:
    /// `--image <payload>=<path>`, or a bare path for every payload in the
    /// set. Repeatable.
    ///
    /// The committed C6 transcripts are at a pinned firmware commit with the
    /// `spike_uart0_link` feature applied as a dirty tree, which is not what a
    /// checkout builds; `scripts/emu/build-reference-image.sh` reproduces that
    /// tree and this is how the runner is pointed at what it produced.
    #[arg(long)]
    pub image: Vec<String>,
    /// Force the payload's effective link on an `lp-emu:*` configuration:
    /// `real` (the shipped USB-Serial-JTAG link, no `spike_uart0_link`) or
    /// `spike` (the UART0 workaround). Unset leaves the payload's own `link`
    /// (`validate list`'s `link=` column) in charge, as it always has been.
    /// Refused on any configuration that is not `lp-emu:*`.
    #[arg(long, value_parser = ["real", "spike"])]
    pub link: Option<String>,
    /// Seconds to wait for the payload's sentinel. EMULATED seconds on an
    /// emulated configuration.
    #[arg(long, default_value_t = 120)]
    pub timeout_secs: u64,
    /// Print the exact commands and stop.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct RecordArgs {
    /// Set name.
    pub set: String,
    /// Configuration name.
    #[arg(long = "config")]
    pub configuration: String,
    /// Serial port, for silicon configurations.
    #[arg(long)]
    pub port: Option<String>,
    /// Capture date, `YYYY-MM-DD`. Part of the committed filename.
    #[arg(long)]
    pub date: Option<String>,
    /// Short git commit of the IMAGE under test. Stated, not sniffed: the
    /// image on a board is often older than the checkout, and a filename
    /// derived from HEAD would then be quietly wrong.
    #[arg(long = "commit")]
    pub firmware_commit: String,
    /// The image was built from a dirty tree. Stated for the same reason the
    /// commit is: the reference images are a commit plus a staged cherry-pick,
    /// and their hello frame says `"dirty":true`.
    #[arg(long = "dirty")]
    pub firmware_dirty: bool,
    /// An already-built image, for the emulated configurations. See
    /// `validate run --image`.
    #[arg(long)]
    pub image: Vec<String>,
    /// Force the payload's effective link on an `lp-emu:*` configuration. See
    /// `validate run --link`.
    #[arg(long, value_parser = ["real", "spike"])]
    pub link: Option<String>,
    #[arg(long, default_value_t = 120)]
    pub timeout_secs: u64,
    /// Print the exact commands and the destination paths, and stop.
    #[arg(long)]
    pub dry_run: bool,
}
