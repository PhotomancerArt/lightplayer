//! Writing a downloaded package to a board, and reading its target and
//! hello back.
//!
//! Reuses `lpa_link::layout_migration`'s `layout_session` + `decide` — the
//! same path `lp-cli hardware lpfs migrate` uses — but, unlike that
//! command, actually writes a `Plain` decision: `lpfs migrate` exists to
//! test and run layout migrations on purpose and deliberately refuses the
//! ordinary case ("use a plain flash"); `firmware install` IS the plain
//! flash for the ordinary case, where a board's filesystem already matches
//! the package's layout and only the firmware changes.

use std::path::Path;

use anyhow::{Context, Result, bail};
use lpa_link::layout_migration::device_backup_archive::BackupPurpose;
use lpa_link::layout_migration::{
    LayoutDecision, LayoutProbe, LpfsGeometry, decide, describe_layout,
};
use lpa_link::providers::host_serial_esp32::{LayoutSessionOutcome, layout_session};
use lpa_link::{LinkError, LinkLayoutInspection, PartitionTable, normalize_base_mac};

use crate::commands::hardware::lpfs::lpfs_target::{
    BackupFacts, default_backup_dir, kb, stderr_events, store_backup,
};
use crate::commands::hardware::lpfs::migrate::refusal_text;

/// What writing the package did.
pub enum WriteOutcome {
    /// The operator said no; nothing was written.
    Declined,
    /// Written and read back. `migrated` is true when the board's files
    /// moved to the package's layout.
    Written { migrated: bool },
}

/// Install the package at `manifest_path` onto the board on `port`.
pub fn install_package(
    port: &str,
    manifest_path: &Path,
    backup_dir: Option<&Path>,
    yes: bool,
) -> Result<WriteOutcome> {
    let manifest_str = manifest_path.display().to_string();
    let mut migrated = false;

    let outcome = layout_session(
        port,
        &manifest_str,
        |inspection| {
            let decision = decide_for(inspection)?;
            match decision {
                LayoutDecision::Plain(plan) => {
                    println!(
                        "This board's filesystem layout already matches the package; \
                         installing the firmware only, its files untouched."
                    );
                    if !yes && !confirm("Install this firmware now?").map_err(to_link_error)? {
                        return Ok(None);
                    }
                    Ok(Some(plan))
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
                    let dir = match backup_dir {
                        Some(dir) => dir.to_path_buf(),
                        None => default_backup_dir(base_mac.as_deref()).map_err(to_link_error)?,
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
                         This firmware moves {} files ({}) to the filesystem at {:#x}: {} of \
                         {} blocks, {} free{}.",
                        backup.display(),
                        summary.file_count,
                        kb(summary.total_bytes),
                        summary.target.offset,
                        summary.blocks_used,
                        summary.target.block_count,
                        summary.blocks_free,
                        if summary.tight { " (tight)" } else { "" }
                    );
                    if !yes
                        && !confirm("Install this firmware and move the files now?")
                            .map_err(to_link_error)?
                    {
                        return Ok(None);
                    }
                    plan.backup_confirmed = true;
                    plan.base_mac = base_mac;
                    migrated = true;
                    Ok(Some(plan))
                }
                // `decide_for` never passes a pending backup, so `decide`
                // cannot produce this variant from here (see
                // `lpa_link::layout_migration::migration_plan::decide`); kept
                // exhaustive in case that changes.
                LayoutDecision::Restore { .. } => {
                    println!(
                        "This board has a pending backup restore queued; run \
                         `lp-cli hardware lpfs restore` first. Nothing written."
                    );
                    Ok(None)
                }
            }
        },
        &stderr_events(),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    match outcome {
        LayoutSessionOutcome::NotWritten(_) => Ok(WriteOutcome::Declined),
        LayoutSessionOutcome::Written { .. } => {
            println!("Written and read back. The board is restarting.");
            Ok(WriteOutcome::Written { migrated })
        }
    }
}

/// The board's own target, from its hello's board manifest. Only a split
/// image sends one (`lpc_wire::ServerHello::firmware`); anything else
/// (single image, another chip, a blank board) has no opinion, so
/// `--target` is required there.
pub fn read_target(port: &str) -> Result<String> {
    with_hello(port, |hello| {
        hello
            .firmware
            .map(|m| m.target)
            .context("the board's hello does not name a target (not a split image)")
    })
}

/// Connect, read the hello, and print the version, target and filesystem
/// state it reports — `lp-cli hardware lpfs migrate`'s own hello check
/// (`fs: mounted`), with the version and target made explicit: this
/// command's whole point is "is the board now running what was asked for".
pub fn wait_for_hello(port: &str, expected_version: &str, expected_target: &str) -> Result<()> {
    with_hello(port, |hello| {
        if hello.hardware.fs != lpc_wire::FsBootState::Mounted {
            bail!(
                "the board came back with its filesystem {:?}, not mounted — its files are in \
                 the backup; put them back with `lp-cli hardware lpfs restore`",
                hello.hardware.fs
            );
        }
        if hello.build.version.as_ref() != expected_version {
            bail!(
                "the board came back running version {}, expected {expected_version}",
                hello.build.version
            );
        }
        let target = hello
            .firmware
            .as_ref()
            .map_or(expected_target, |m| m.target.as_str());
        if target != expected_target {
            bail!("the board came back as target `{target}`, expected `{expected_target}`");
        }
        println!(
            "Verified: version {}, target {}, fs mounted.",
            hello.build.version, target
        );
        Ok(())
    })
}

/// Classify the session's reads (mirrors
/// `hardware::lpfs::migrate::decide_for`; no pending-backup store on this
/// command line either — a restore is its own command).
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

fn confirm(prompt: &str) -> Result<bool> {
    dialoguer::Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .context("no answer (pass --yes to write without asking)")
}

fn to_link_error(e: anyhow::Error) -> LinkError {
    LinkError::other(e.to_string())
}

/// Connect over `port`, hand the hello to `f` (closing the connection
/// either way), and return what `f` decided.
fn with_hello<T>(port: &str, f: impl FnOnce(lpc_wire::ServerHello) -> Result<T>) -> Result<T> {
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
        .context("the board did not answer")?;
        let hello = connection.hello();
        connection.close().await;
        let Some(hello) = hello else {
            bail!("the board came back without a hello");
        };
        f(hello)
    }))
}
