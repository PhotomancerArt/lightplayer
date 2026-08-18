use anyhow::Result;

use super::args::{HardwareCli, HardwareSubcommand, ManifestArgs};
use super::bench;
use super::calibrate;
use super::list;
use super::manifest;

pub fn handle_hardware(cli: HardwareCli) -> Result<()> {
    match cli.subcommand {
        Some(HardwareSubcommand::List(args)) => list::handle_list(args),
        Some(HardwareSubcommand::Manifest(args)) => manifest::handle_manifest(args),
        Some(HardwareSubcommand::Calibrate(args)) => calibrate::handle_calibrate(args),
        Some(HardwareSubcommand::Bench(args)) => bench::handle_bench(args),
        None => manifest::handle_manifest(ManifestArgs {
            repo: None,
            boards_dir: None,
            command: None,
        }),
    }
}
