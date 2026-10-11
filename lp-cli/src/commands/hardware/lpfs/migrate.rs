//! `lp-cli hardware lpfs migrate`: move a board's files to a firmware
//! package's layout, in ONE bootloader session — inspect, back up, plan,
//! write, read back — then wait for the board's hello and check that its
//! files mounted and its identity came back.
//!
//! The backup is stored and read back before anything is written: between
//! retiring the old filesystem and writing the new one's superblock, it is
//! the only copy (plan MQ9).

use anyhow::{Context, Result, bail};
use lpa_link::layout_migration::device_backup_archive::BackupPurpose;
use lpa_link::layout_migration::{
    LayoutDecision, LayoutProbe, LpfsGeometry, Refusal, decide, describe_layout,
};
use lpa_link::providers::host_serial_esp32::{LayoutSessionOutcome, layout_session};
use lpa_link::{LinkError, LinkLayoutInspection, PartitionTable, normalize_base_mac};

use super::super::args::LpfsMigrateArgs;
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
            let decision = decide_for(inspection)?;
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
                        "Backup stored and read back: {}\n\
                         Moving {} files ({}) to the filesystem at {:#x}: {} of {} blocks, {} \
                         free{}.",
                        backup.display(),
                        summary.file_count,
                        kb(summary.total_bytes),
                        summary.target.offset,
                        summary.blocks_used,
                        summary.target.block_count,
                        summary.blocks_free,
                        if summary.tight { " (tight)" } else { "" }
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
    let classified = LayoutProbe::replay(target.clone(), chip_is_c6, &inspection.reads);
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
    )
    .map_err(|refusal| LinkError::other(refusal_text(&refusal)))
}

/// A refusal in words, with what to do about it.
pub fn refusal_text(refusal: &Refusal) -> String {
    match refusal {
        Refusal::DoesNotFit { .. } | Refusal::TooTight { .. } => format!(
            "{refusal}. Nothing was written. Remove a project from the board \
             (`lp-cli` or Studio's Remove project) and try again."
        ),
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
            bail!("{}", super::REFUSED_STORE);
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
