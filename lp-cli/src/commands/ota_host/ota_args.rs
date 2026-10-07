//! The over-the-air flags, shared by `lp-cli emu run --host-link` and
//! `lp-cli link capture` (flattened into each).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Args;
use lpc_update::PieceKind;

/// Offer a build on the board's update channel (lp-link channel 3) and
/// drive the update with `lpa-update`'s driver.
#[derive(Clone, Debug, Default, Args)]
pub struct OtaArgs {
    /// Offer the build in this OTA directory: a split package's parts
    /// directory as `lp-cli firmware package` leaves it
    /// (`target/firmware-parts/<target>/`: `ota-manifest.json`, `core.bin`,
    /// `engine.bin`, `core.z`, `engine.z`), read through its
    /// `ota-manifest.json` and the files that names. A release staging
    /// directory (files named `<target>.<file>`) works too. The package's
    /// `split` block is never read.
    #[arg(long = "ota-offer", value_name = "DIR")]
    pub ota_offer: Option<PathBuf>,

    /// An engine cache: files named `<sha256>.bin`. Where a heal looks for
    /// the board's engine first (Studio's cache, modelled), and where the
    /// backup read back before an update lands.
    #[arg(long = "ota-cache", value_name = "DIR", requires = "ota_offer")]
    pub ota_cache: Option<PathBuf>,

    /// Chunks kept in flight per request (send-ahead). Default 1 on USB,
    /// serial and tcp; 4 on `blepipe:` (`ServeConfig::BLE`).
    #[arg(long = "ota-ahead", requires = "ota_offer")]
    pub ota_ahead: Option<u8>,

    /// A password for the core-side login (channel 3), whenever the board
    /// asks for one; on `blepipe:` also the running engine's login
    /// (channel 1), before the update starts. Tests and desk only; never
    /// logged.
    #[arg(long = "ota-password", requires = "ota_offer")]
    pub ota_password: Option<String>,

    /// Do only what needs no press: heal an engine-less board, continue an
    /// update this build is pending for, or report. An offered update waits
    /// (and the run ends without one).
    #[arg(long = "ota-heal-only", requires = "ota_offer")]
    pub ota_heal_only: bool,

    /// Never send encoding 1 (`Z`): every chunk raw (`D`).
    #[arg(long = "ota-no-z", requires = "ota_offer")]
    pub ota_no_z: bool,

    /// Test only: corrupt the chunk at `<C|E>:<offset>` once, the first
    /// time it is sent (a `D` gets a flipped byte, a `Z` is cut in half).
    #[arg(long = "ota-corrupt", hide = true, requires = "ota_offer")]
    pub ota_corrupt: Option<String>,

    /// `emu run` only: end the run (a power cut, the flash written back)
    /// right after the Nth update request is answered.
    #[arg(long = "ota-cut-after", requires = "ota_offer")]
    pub ota_cut_after: Option<u32>,
}

/// One chunk to corrupt: its piece and its offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorruptAt {
    pub kind: PieceKind,
    pub off: u32,
}

impl OtaArgs {
    /// `--ota-corrupt`, parsed.
    pub fn corrupt_at(&self) -> Result<Option<CorruptAt>> {
        self.ota_corrupt.as_deref().map(parse_corrupt).transpose()
    }
}

/// `E:0x40000` or `C:4096`.
pub fn parse_corrupt(text: &str) -> Result<CorruptAt> {
    let (kind, off) = text
        .split_once(':')
        .with_context(|| format!("--ota-corrupt `{text}`: expected <C|E>:<offset>"))?;
    let kind = match kind {
        "C" | "c" => PieceKind::Core,
        "E" | "e" => PieceKind::Engine,
        _ => bail!("--ota-corrupt `{text}`: the piece is C or E"),
    };
    let off = match off.strip_prefix("0x") {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => off.parse(),
    }
    .with_context(|| format!("--ota-corrupt `{text}`: the offset is a number"))?;
    if off % lpc_update::CHUNK != 0 {
        bail!("--ota-corrupt `{text}`: a chunk starts on a 4 KiB boundary");
    }
    Ok(CorruptAt { kind, off })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        ota: OtaArgs,
    }

    #[test]
    fn the_flags_parse() {
        let cli = Cli::try_parse_from([
            "x",
            "--ota-offer",
            "pkg",
            "--ota-cache",
            "cache",
            "--ota-ahead",
            "4",
            "--ota-heal-only",
            "--ota-no-z",
            "--ota-corrupt",
            "E:0x40000",
            "--ota-cut-after",
            "12",
        ])
        .unwrap();
        assert_eq!(cli.ota.ota_offer.unwrap(), PathBuf::from("pkg"));
        assert_eq!(cli.ota.ota_ahead, Some(4));
        assert!(cli.ota.ota_heal_only && cli.ota.ota_no_z);
        assert_eq!(cli.ota.ota_cut_after, Some(12));
    }

    #[test]
    fn every_ota_flag_needs_an_offer() {
        assert!(Cli::try_parse_from(["x", "--ota-heal-only"]).is_err());
        assert!(Cli::try_parse_from(["x", "--ota-cut-after", "3"]).is_err());
    }

    #[test]
    fn a_corruption_names_a_piece_and_a_chunk() {
        assert_eq!(
            parse_corrupt("E:0x40000").unwrap(),
            CorruptAt {
                kind: PieceKind::Engine,
                off: 0x40000
            }
        );
        assert_eq!(parse_corrupt("C:4096").unwrap().off, 4096);
        assert!(parse_corrupt("X:0").is_err());
        assert!(parse_corrupt("E:100").is_err(), "not a chunk boundary");
    }
}
