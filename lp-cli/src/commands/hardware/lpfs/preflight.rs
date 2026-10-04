//! `lp-cli hardware lpfs preflight`: before a plain flash, would writing the
//! image's partition table strand (or destroy) the board's files? Both
//! directions refuse (plan MQ7): a pre-2026-10 image onto a migrated board
//! formats over the migrated files.
//!
//! Exit codes, for scripts: `0` proceed; `3` refused (layouts differ); `4`
//! refused with `--migrate` — run `lp-cli hardware lpfs migrate` instead.
//! `--discard-lpfs` erases the board's own filesystem region and proceeds.

use anyhow::Result;
use lpa_link::layout_migration::{LayoutPreflight, preflight};
use lpa_link::providers::host_serial_esp32::{erase_region, read_partition_table};
use lpa_link::{LinkFlashRegion, PartitionTable};

use super::super::args::LpfsPreflightArgs;
use super::lpfs_target::{stderr_events, target_table};

pub fn handle_preflight(args: LpfsPreflightArgs) -> Result<()> {
    let image_table = target_table(Some(&args.table))?;
    let (_, device_table) =
        read_partition_table(&args.port, &stderr_events()).map_err(|e| anyhow::anyhow!("{e}"))?;
    let verdict = preflight(&device_table, &image_table);
    match &verdict {
        LayoutPreflight::Same => println!("layout preflight: same layout — the board's files stay"),
        LayoutPreflight::DeviceBlank => println!("layout preflight: blank board"),
        LayoutPreflight::DeviceForeign => {
            println!("layout preflight: not a LightPlayer layout — nothing of ours to keep")
        }
        LayoutPreflight::Differs { from, to } => {
            if args.discard_lpfs {
                let region = PartitionTable::parse(&device_table)
                    .ok()
                    .as_ref()
                    .and_then(LinkFlashRegion::lpfs_in);
                if let Some(region) = region {
                    erase_region(&args.port, region.offset, region.length, &stderr_events())
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    println!(
                        "layout preflight: the board's filesystem at {:#x} was ERASED \
                         (--discard-lpfs); flashing {to}",
                        region.offset
                    );
                }
                return Ok(());
            }
            eprintln!(
                "layout preflight: REFUSED. The board holds the {from}; this image writes the \
                 {to}.\nFlashing it would strand the board's files (or, onto a newer layout, \
                 destroy them). Either:\n  \
                 - move the files: lp-cli hardware lpfs migrate --port {port} --merged <image> \
                 (just flash-fw-esp32c6 … migrate=1), or\n  \
                 - throw them away (test boards): --discard-lpfs (just … discard=1).",
                port = args.port
            );
            std::process::exit(if args.migrate { 4 } else { 3 });
        }
    }
    Ok(())
}
