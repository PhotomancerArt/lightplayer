use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "firmware",
    about = "Build, package and inspect firmware variants."
)]
pub struct FirmwareCli {
    #[command(subcommand)]
    pub subcommand: FirmwareSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum FirmwareSubcommand {
    /// Extract and print the manifest core embedded in a firmware artifact
    /// (ELF, espflash merged image, or wasm module). The blob is parsed and
    /// re-serialized, so `show` succeeding also validates its shape.
    Show(ShowArgs),
    /// List the checked-in firmware build definitions (`lp-fw/builds/`).
    List(ListArgs),
    /// Cargo-build one firmware variant from its build definition.
    Build(BuildArgs),
    /// Build, merge and emit a distributable firmware package
    /// (`manifest.json` schemaVersion 2 + merged image). A split build also
    /// gets its OTA files (`ota-manifest.json`, `core.z`, `engine.z`) in its
    /// parts directory, `target/firmware-parts/<id>/`.
    Package(PackageArgs),
    /// Gather packaged firmware into a release staging directory under the
    /// release's asset names (`<target>.<file>`). Compresses nothing.
    ReleaseAssets(ReleaseAssetsArgs),
    /// Re-verify a release staging directory from its files alone.
    ReleaseCheck(ReleaseCheckArgs),
    /// Put a published release's firmware on a board over USB: resolve the
    /// release, download and verify its package against lightplayer.app's
    /// public lookup, lease the board (the desk's board bench, when
    /// present), and write it with the same layout-aware flasher
    /// `lp-cli hardware lpfs migrate` uses.
    Install(InstallArgs),
}

#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Path to the firmware artifact to scan.
    pub artifact: PathBuf,

    /// Print the payload exactly as embedded instead of pretty-printing.
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Emit the build definitions as JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    /// Build definition id (see `lp-cli firmware list`).
    pub id: String,

    /// Build a split def (`"split": true`) as ONE linked image instead:
    /// one link pass, no loader, no boot records, no OTA files — a board
    /// running it updates over USB only. The fast local build
    /// (`lp-fw/builds/README.md`); releases and deploys never pass it.
    #[arg(long)]
    pub single_image: bool,
}

#[derive(Debug, Args)]
pub struct PackageArgs {
    /// Build definition id (see `lp-cli firmware list`).
    pub id: String,

    /// Output directory; defaults to
    /// `target/studio-web-assets/firmware/<id>`.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Package an already-built ELF instead of running cargo first.
    #[arg(long)]
    pub no_build: bool,

    /// Build a split def (`"split": true`) as ONE linked image instead:
    /// one link pass, no loader, no boot records, no OTA files — a board
    /// running it updates over USB only. The fast local build
    /// (`lp-fw/builds/README.md`); releases and deploys never pass it.
    #[arg(long)]
    pub single_image: bool,
}

#[derive(Debug, Args)]
pub struct ReleaseAssetsArgs {
    /// Targets to stage, comma-separated (default: every id in
    /// `lp-fw/builds/served.json`).
    #[arg(long, value_delimiter = ',')]
    pub targets: Option<Vec<String>>,

    /// The staging directory; must be absent or empty.
    #[arg(long)]
    pub out: PathBuf,

    /// Also stage a dev version (the pre-merge dry run). A release never
    /// passes it.
    #[arg(long)]
    pub allow_dev: bool,
}

#[derive(Debug, Args)]
pub struct ReleaseCheckArgs {
    /// The staging directory (or a downloaded release) to verify.
    pub dir: PathBuf,

    /// Accept a dev version (the pre-merge dry run).
    #[arg(long)]
    pub allow_dev: bool,

    /// Targets to check, comma-separated (default: every id in
    /// `lp-fw/builds/served.json` in a checkout, otherwise every
    /// `<target>.package.json` in the directory).
    #[arg(long, value_delimiter = ',')]
    pub targets: Option<Vec<String>>,

    /// The version every target must carry: the release's (its tag without
    /// the `v`). Without it, the targets must still agree on one.
    #[arg(long)]
    pub version: Option<String>,
}

#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("board_select").args(["mac", "port"]).required(true)
))]
pub struct InstallArgs {
    /// Which release to install: a version (`2026.10.05-3`), `latest`, or
    /// `previous` (the newest published release older than latest that
    /// carries firmware for the target).
    #[arg(long)]
    pub release: String,

    /// Select the board by MAC (its USB serial number on Espressif native
    /// USB). Passive: opens and resets nothing to find the port.
    #[arg(long)]
    pub mac: Option<String>,

    /// Select the board by its serial port.
    #[arg(long)]
    pub port: Option<String>,

    /// The firmware target (e.g. `esp32c6-4mb`). Defaults to the board's
    /// own target, read from its hello, for a board already running a
    /// split image; required for anything else (a blank board, a single
    /// image, another chip's firmware).
    #[arg(long)]
    pub target: Option<String>,

    /// Who is leasing the board on the desk's board bench, when one is
    /// present (`board take --as <who>`). Falls back to `$BOARD_HOLDER`
    /// when not given; `board` refuses a lease with neither.
    #[arg(long = "as")]
    pub holder: Option<String>,

    /// Do not ask before writing.
    #[arg(long)]
    pub yes: bool,

    /// Resolve, download and verify the release, print what would be
    /// written, and stop — no board is touched.
    #[arg(long)]
    pub dry_run: bool,

    /// Where the mandatory backup is stored, when the board's filesystem
    /// must move to the package's layout (default
    /// ~/.lightplayer/backups/<mac>/). Unused for the ordinary case where
    /// the layout already matches.
    #[arg(long)]
    pub backup_dir: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        install: InstallArgs,
    }

    fn parse(args: &[&str]) -> Result<InstallArgs, clap::Error> {
        let mut full = vec!["x"];
        full.extend_from_slice(args);
        Cli::try_parse_from(full).map(|cli| cli.install)
    }

    #[test]
    fn parses_with_a_port_and_defaults() {
        let args = parse(&["--release", "latest", "--port", "/dev/cu.usbmodem1"]).unwrap();
        assert_eq!(args.release, "latest");
        assert_eq!(args.port.as_deref(), Some("/dev/cu.usbmodem1"));
        assert_eq!(args.mac, None);
        assert_eq!(args.target, None);
        assert_eq!(args.holder, None);
        assert!(!args.yes);
        assert!(!args.dry_run);
        assert_eq!(args.backup_dir, None);
    }

    #[test]
    fn parses_every_flag() {
        let args = parse(&[
            "--release",
            "2026.10.05-3",
            "--mac",
            "A0:F2:62:87:B4:8C",
            "--target",
            "esp32c6-4mb",
            "--as",
            "yona",
            "--yes",
            "--dry-run",
            "--backup-dir",
            "/tmp/backups",
        ])
        .unwrap();
        assert_eq!(args.release, "2026.10.05-3");
        assert_eq!(args.mac.as_deref(), Some("A0:F2:62:87:B4:8C"));
        assert_eq!(args.target.as_deref(), Some("esp32c6-4mb"));
        assert_eq!(args.holder.as_deref(), Some("yona"));
        assert!(args.yes);
        assert!(args.dry_run);
        assert_eq!(args.backup_dir, Some(PathBuf::from("/tmp/backups")));
    }

    #[test]
    fn release_is_required() {
        assert!(parse(&["--port", "/dev/cu.usbmodem1"]).is_err());
    }

    #[test]
    fn exactly_one_of_mac_or_port_is_required() {
        assert!(parse(&["--release", "latest"]).is_err(), "neither given");
        assert!(
            parse(&[
                "--release",
                "latest",
                "--mac",
                "A0:F2:62:87:B4:8C",
                "--port",
                "/dev/cu.usbmodem1"
            ])
            .is_err(),
            "both given"
        );
    }
}
