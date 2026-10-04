//! `lp-cli hardware lpfs restore`: put a backup ZIP's files back onto a
//! board that is already on the package's layout — the last-resort path of
//! the failure matrix (a migration whose browser backup was lost, with only
//! the downloaded ZIP left). Writes the board's filesystem only, never its
//! firmware, and refuses an archive taken from a different board unless told
//! otherwise.

use anyhow::{Context, Result};
use lpa_link::layout_migration::device_backup_archive::read_archive;
use lpa_link::layout_migration::{LayoutProbe, LayoutState, plan_lpfs_restore};
use lpa_link::providers::host_serial_esp32::{LayoutSessionOutcome, layout_session};
use lpa_link::{LinkError, PartitionTable, normalize_base_mac};

use super::super::args::LpfsRestoreArgs;
use super::lpfs_target::{kb, package_manifest, stderr_events};
use super::migrate::refusal_text;

pub fn handle_restore(args: LpfsRestoreArgs) -> Result<()> {
    let bytes =
        std::fs::read(&args.archive).with_context(|| format!("read {}", args.archive.display()))?;
    let (manifest, tree) = read_archive(&bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    println!(
        "Backup: {} files ({}) from board {}, uid {}",
        tree.file_count(),
        kb(tree.total_bytes()),
        manifest.base_mac.as_deref().unwrap_or("unknown"),
        manifest.device_uid.as_deref().unwrap_or("none")
    );
    let package = package_manifest(&args.firmware)?;
    let outcome = layout_session(
        &args.port,
        &package.display().to_string(),
        |inspection| {
            let board = inspection
                .probed_mac
                .as_deref()
                .and_then(normalize_base_mac);
            if !args.other_board && manifest.base_mac.is_some() && board != manifest.base_mac {
                return Err(LinkError::other(format!(
                    "this backup is from board {}, but this board is {} — nothing written \
                     (--other-board restores it anyway, cloning that board's identity)",
                    manifest.base_mac.as_deref().unwrap_or("?"),
                    board.as_deref().unwrap_or("unidentified")
                )));
            }
            let target = PartitionTable::parse(&inspection.target_table)
                .map_err(|e| LinkError::other(e.to_string()))?;
            let chip_is_c6 = inspection
                .chip_name
                .as_deref()
                .and_then(lpa_link::chip_id_from_reported)
                == Some("esp32c6");
            let state = LayoutProbe::replay(target.clone(), chip_is_c6, &inspection.reads).state;
            if !matches!(
                state,
                LayoutState::CurrentMounts | LayoutState::CurrentNothing
            ) {
                return Err(LinkError::other(format!(
                    "the board is not on the package's layout ({state:?}) — nothing written; \
                     flash or migrate it first"
                )));
            }
            let (mut plan, summary) = plan_lpfs_restore(&target, &tree)
                .map_err(|refusal| LinkError::other(refusal_text(&refusal)))?;
            println!(
                "Writing {} files to the filesystem at {:#x} ({} of {} blocks). This replaces \
                 every file on the board.",
                summary.file_count,
                summary.target.offset,
                summary.blocks_used,
                summary.target.block_count
            );
            if !args.yes
                && !dialoguer::Confirm::new()
                    .with_prompt("Replace the board's files with the backup?")
                    .default(false)
                    .interact()
                    .map_err(|e| LinkError::other(e.to_string()))?
            {
                return Ok(None);
            }
            // The archive on disk IS the backup.
            plan.backup_confirmed = true;
            plan.base_mac = board;
            Ok(Some(plan))
        },
        &stderr_events(),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    match outcome {
        LayoutSessionOutcome::Written { .. } => println!("Restored and read back."),
        LayoutSessionOutcome::NotWritten(_) => println!("Nothing written."),
    }
    Ok(())
}
