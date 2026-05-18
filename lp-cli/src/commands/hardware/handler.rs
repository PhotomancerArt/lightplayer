use anyhow::Result;

use super::args::{HardwareCli, HardwareSubcommand, ManifestArgs};
use super::calibrate;
use super::manifest;

pub fn handle_hardware(cli: HardwareCli) -> Result<()> {
    match cli.subcommand {
        Some(HardwareSubcommand::Manifest(args)) => manifest::handle_manifest(args),
        Some(HardwareSubcommand::Calibrate(args)) => calibrate::handle_calibrate(args),
        None => manifest::handle_manifest(ManifestArgs {
            repo: None,
            boards_dir: None,
            command: None,
        }),
    }
}
