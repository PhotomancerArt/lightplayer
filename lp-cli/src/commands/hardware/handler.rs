use anyhow::{Result, bail};

use super::args::{HardwareCli, HardwareSubcommand, ManifestArgs};
use super::manifest;

pub fn handle_hardware(cli: HardwareCli) -> Result<()> {
    match cli.subcommand {
        Some(HardwareSubcommand::Manifest(args)) => manifest::handle_manifest(args),
        Some(HardwareSubcommand::Calibrate(args)) => {
            bail!(
                "hardware calibration is planned but not implemented yet (target={}, board={}, port={})",
                args.target,
                args.board,
                args.port.as_deref().unwrap_or("serial:auto"),
            )
        }
        None => manifest::handle_manifest(ManifestArgs {
            repo: None,
            boards_dir: None,
            command: None,
        }),
    }
}
