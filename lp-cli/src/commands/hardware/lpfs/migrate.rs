//! `lp-cli hardware lpfs migrate`: move a board's files to a firmware
//! package's layout, in ONE bootloader session — inspect, back up, plan,
//! write, read back — then wait for the board's hello and check that its
//! files mounted and its identity came back.
//!
//! The backup is stored and read back before anything is written: between
//! retiring the old filesystem and writing the new one's superblock, it is
//! the only copy (plan MQ9).
//!
//! `--to tree` (every C6 image since the cutover): the move from littlefs into
//! the tree store — staged in the blocks the old filesystem does not use
//! when it can be, so the old board stays whole until the firmware switch
//! (`lpa_link::layout_migration::tree_migration_plan`). `--dry-run` reads
//! the board, prints every step, and writes nothing.

use anyhow::{Context, Result, bail};
use lpa_link::layout_migration::device_backup_archive::BackupPurpose;
use lpa_link::layout_migration::{
    FlashPlan, FlashStep, LayoutDecision, LayoutProbe, LpfsGeometry, MigrationSummary, Refusal,
    WriteOrder, decide, describe_layout, firmware_reads_tree_store,
};
use lpa_link::providers::host_serial_esp32::{LayoutSessionOutcome, layout_session};
use lpa_link::{LinkError, LinkLayoutInspection, PartitionTable, normalize_base_mac};

use super::super::args::{LpfsMigrateArgs, LpfsTargetFs};
use super::lpfs_target::{
    BackupFacts, default_backup_dir, kb, package_manifest, stderr_events, store_backup,
};

pub fn handle_migrate(args: LpfsMigrateArgs) -> Result<()> {
    let manifest = package_manifest(&args.firmware)?;
    let manifest_str = manifest.display().to_string();
    let mut expected_uid: Option<String> = None;
    let mut migrated = false;

    let outcome = layout_session(
        &args.port,
        &manifest_str,
        |inspection| {
            check_target_fs(args.to, inspection)?;
            let decision = decide_for(inspection)?;
            if args.dry_run {
                print_dry_run(&decision);
                return Ok(None);
            }
            match decision {
                LayoutDecision::Plain(_) => {
                    println!(
                        "This board's files do not need to move (its layout matches the \
                         package). Nothing written; use a plain flash."
                    );
                    Ok(None)
                }
                LayoutDecision::Restore { .. } => {
                    println!(
                        "This board has no files of its own to move. Nothing written; to put a \
                         backup back, use `lp-cli hardware lpfs restore`."
                    );
                    Ok(None)
                }
                LayoutDecision::Migrate {
                    mut plan,
                    summary,
                    tree,
                } => {
                    let base_mac = inspection
                        .probed_mac
                        .as_deref()
                        .and_then(normalize_base_mac);
                    let dir = match &args.backup_dir {
                        Some(dir) => dir.clone(),
                        None => default_backup_dir(base_mac.as_deref())
                            .map_err(|e| LinkError::other(e.to_string()))?,
                    };
                    let backup = store_backup(
                        &dir,
                        &tree,
                        &BackupFacts {
                            chip: inspection.chip_name.as_deref(),
                            base_mac: base_mac.as_deref(),
                            source: summary.source.unwrap_or(LpfsGeometry {
                                offset: 0,
                                block_count: 0,
                            }),
                            target: Some(summary.target),
                            purpose: BackupPurpose::LayoutMigration,
                        },
                    )
                    .map_err(|e| LinkError::other(format!("backup not stored: {e}")))?;
                    println!(
                        "Backup stored and read back: {}\n{}",
                        backup.display(),
                        moving(&summary)
                    );
                    if !args.yes && !confirm().map_err(|e| LinkError::other(e.to_string()))? {
                        println!("Nothing written.");
                        return Ok(None);
                    }
                    expected_uid = summary.device_uid.clone();
                    plan.backup_confirmed = true;
                    plan.base_mac = base_mac;
                    migrated = true;
                    Ok(Some(plan))
                }
            }
        },
        &stderr_events(),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    match outcome {
        LayoutSessionOutcome::NotWritten(_) => Ok(()),
        LayoutSessionOutcome::Written { .. } if !migrated => Ok(()),
        LayoutSessionOutcome::Written { .. } => {
            println!("Written and read back. The board is restarting.");
            if args.no_verify {
                return Ok(());
            }
            verify_hello(&args.port, expected_uid.as_deref())
        }
    }
}

/// Classify the session's reads and decide (no pending-backup store on the
/// command line: a restore is its own command).
fn decide_for(inspection: &LinkLayoutInspection) -> Result<LayoutDecision, LinkError> {
    let target = PartitionTable::parse(&inspection.target_table)
        .map_err(|e| LinkError::other(format!("the package's table: {e}")))?;
    let chip_is_c6 = inspection
        .chip_name
        .as_deref()
        .and_then(lpa_link::chip_id_from_reported)
        == Some("esp32c6");
    let classified = LayoutProbe::replay_for(
        target.clone(),
        inspection.target_fs,
        chip_is_c6,
        &inspection.reads,
    );
    if let Some(device) = &classified.device_table {
        eprintln!(
            "  board: {} → package: {}",
            describe_layout(device),
            describe_layout(&target)
        );
    }
    decide(
        &classified,
        &target,
        inspection.target_image_len,
        None,
        false,
        firmware_reads_tree_store(inspection.target_manifest.as_deref()),
    )
    .map_err(|refusal| LinkError::other(refusal_text(&refusal)))
}

/// `--to`: the image must mount what the caller asked for.
fn check_target_fs(
    to: Option<LpfsTargetFs>,
    inspection: &LinkLayoutInspection,
) -> Result<(), LinkError> {
    let wanted = match to {
        None => return Ok(()),
        Some(LpfsTargetFs::Tree) => lpa_link::FsKind::Tree,
        Some(LpfsTargetFs::Littlefs) => lpa_link::FsKind::Littlefs,
    };
    if inspection.target_fs != wanted {
        return Err(LinkError::other(format!(
            "this image mounts {}, not {} (an image for the tree store is built with \
             `fs-tree`, every C6 package since the cutover). Nothing was written.",
            inspection.target_fs.word(),
            wanted.word()
        )));
    }
    Ok(())
}

/// What a migration is about to do, in one sentence.
fn moving(summary: &MigrationSummary) -> String {
    let unit = match summary.target_fs {
        lpa_link::FsKind::Tree => "sectors",
        lpa_link::FsKind::Littlefs => "blocks",
    };
    let order = match summary.order {
        Some(WriteOrder::Staged) => {
            " The old filesystem stays whole until the firmware is switched."
        }
        Some(WriteOrder::Retired) => {
            " The old filesystem is erased first: until the new one is written, the backup is \
             the only copy."
        }
        None => "",
    };
    format!(
        "Moving {} files ({}) to the {} at {:#x}: {} of {} {unit} of 4 KB, {} free{}.{order}",
        summary.file_count,
        kb(summary.total_bytes),
        summary.target_fs.word(),
        summary.target.offset,
        summary.blocks_used,
        summary.target.block_count,
        summary.blocks_free,
        if summary.tight { " (tight)" } else { "" }
    )
}

/// `--dry-run`: the decision and every step, nothing written.
fn print_dry_run(decision: &LayoutDecision) {
    let (what, plan, summary): (&str, &FlashPlan, Option<&MigrationSummary>) = match decision {
        LayoutDecision::Plain(plan) => ("No files move: a plain flash", plan, None),
        LayoutDecision::Migrate { plan, summary, .. } => ("Migration", plan, Some(summary)),
        LayoutDecision::Restore { plan, summary } => ("Restore from a backup", plan, Some(summary)),
    };
    println!("Dry run — nothing written, no backup stored.\n{what}:");
    if let Some(summary) = summary {
        println!("  {}", moving(summary));
    }
    for (index, step) in plan.steps.iter().enumerate() {
        let line = match step {
            FlashStep::WriteFirmware => "write the firmware image at 0x0".to_string(),
            FlashStep::Erase { offset, length } => {
                format!("erase  {offset:#08x} + {length:#x} ({} KB)", length / 1024)
            }
            FlashStep::Write { offset, bytes } => {
                format!(
                    "write  {offset:#08x} + {:#x} ({} KB)",
                    bytes.len(),
                    bytes.len() / 1024
                )
            }
            FlashStep::VerifyEquals { offset, bytes } => {
                format!("verify {offset:#08x} + {:#x}", bytes.len())
            }
        };
        println!("  {:>3}. {line}", index + 1);
    }
}

/// A refusal in words, with what to do about it.
pub fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::DoesNotFit { .. } | Refusal::TooTight { .. } => format!(
            "{refusal}. Nothing was written. Remove a project from the board \
             (`lp-cli` or Studio's Remove project) and try again."
        ),
        // Its own words already say nothing was written.
        Refusal::TreeStoreNotRead | Refusal::TreeStoreWouldBeWritten => format!("{refusal}."),
        other => format!("{other}. Nothing was written."),
    }
}

fn confirm() -> Result<bool> {
    dialoguer::Confirm::new()
        .with_prompt("Write the firmware and move the files now?")
        .default(false)
        .interact()
        .context("no answer (pass --yes to write without asking)")
}

/// Connect, read the hello, and require `fs: mounted` and the same uid.
fn verify_hello(port: &str, expected_uid: Option<&str>) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(async move {
        let spec = crate::client::HostSpecifier::parse(&format!("serial:{port}"))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let connection = crate::client::cli_connect::cli_connect(
            spec,
            crate::client::cli_connect::stderr_device_events(false),
        )
        .await
        .context("the board did not come back")?;
        let hello = connection.hello();
        connection.close().await;
        let Some(hello) = hello else {
            bail!("the board came back without a hello");
        };
        if hello.hardware.fs == lpc_wire::FsBootState::Refused {
            bail!("{}", super::refused_store_words(&hello));
        }
        if hello.hardware.fs != lpc_wire::FsBootState::Mounted {
            bail!(
                "the board came back with its filesystem {:?}, not mounted — its files are in the \
                 backup; put them back with `lp-cli hardware lpfs restore`",
                hello.hardware.fs
            );
        }
        if hello.device_uid.as_deref() != expected_uid {
            bail!(
                "the board came back as {:?}, expected {:?}",
                hello.device_uid,
                expected_uid
            );
        }
        println!(
            "Verified: the board's files mounted and it is still {}.",
            expected_uid.unwrap_or("unnamed")
        );
        Ok(())
    }))
}
