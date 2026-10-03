//! The Flash activity's layout step, decided on the app side (C6
//! repartition, plan P06): a provider's raw layout reads in, a verdict for
//! the model and a staged plan (plus the backup to store) out.
//!
//! Pure: the classification, the plan and the archive are
//! `lpa_link::layout_migration`'s; the store and the transport are the
//! effects layer's (`device_effects.rs`). The model only ever sees the
//! [`LayoutVerdict`] — never a byte of the board's files.

use lpa_devices::LayoutVerdict;
use lpa_link::layout_migration::device_backup_archive::{
    BACKUP_FORMAT_VERSION, BackupManifest, BackupPurpose, backup_file_name, write_archive,
};
use lpa_link::layout_migration::lpfs_geometry::LPFS_BLOCK_SIZE;
use lpa_link::layout_migration::lpfs_tree::LpfsTree;
use lpa_link::layout_migration::{
    FREE_BLOCK_FLOOR, LayoutDecision, LayoutProbe, LpfsGeometry, Refusal, decide,
};
use lpa_link::{FlashPlan, LinkLayoutInspection, PartitionTable, normalize_base_mac};

use super::device_backup_store::{BackupEntry, BackupStatus};

/// A stored backup the resume rule may offer: its index entry and files.
pub struct PendingBackup<'a> {
    pub entry: &'a BackupEntry,
    pub tree: &'a LpfsTree,
}

/// What the effects layer holds for a device between the inspection and
/// the write.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutStaging {
    /// What the model is told.
    pub verdict: LayoutVerdict,
    /// The plan a carried flash runs (`None`: plain flash or refusal).
    pub plan: Option<FlashPlan>,
    /// A backup to store before anything is written (a migration), or to
    /// offer as a download (a refusal).
    pub archive: Option<StagedArchive>,
    /// For a restore: the stored backup being put back.
    pub restoring: Option<String>,
    /// The user downloaded the backup (Continue's other key).
    pub downloaded: bool,
}

/// A backup archive and its index entry.
#[derive(Clone, Debug, PartialEq)]
pub struct StagedArchive {
    pub entry: BackupEntry,
    pub bytes: Vec<u8>,
}

impl LayoutStaging {
    /// The plan as an executor may run it: confirmed once the backup is
    /// stored (the verdict says so) or downloaded, or when it restores a
    /// backup already stored.
    pub fn confirmed_plan(&self) -> Result<FlashPlan, String> {
        let mut plan = self
            .plan
            .clone()
            .ok_or_else(|| "nothing was staged to write".to_string())?;
        let stored = match &self.verdict {
            LayoutVerdict::Migrate { backup_stored, .. } => *backup_stored,
            LayoutVerdict::Restore { .. } => true,
            LayoutVerdict::Plain | LayoutVerdict::Refused { .. } => false,
        };
        if !(stored || self.downloaded) {
            return Err(
                "the backup of this board's files was not stored or downloaded — nothing was \
                 written"
                    .to_string(),
            );
        }
        plan.backup_confirmed = true;
        Ok(plan)
    }
}

/// Decide the layout step for `inspection`.
pub fn stage_layout(
    inspection: &LinkLayoutInspection,
    pending: Option<PendingBackup<'_>>,
    restore_requested: bool,
    now_secs: f64,
) -> Result<LayoutStaging, String> {
    let target = PartitionTable::parse(&inspection.target_table)
        .map_err(|error| format!("the firmware's partition table: {error}"))?;
    let chip_is_c6 = inspection
        .chip_name
        .as_deref()
        .and_then(lpa_link::chip_id_from_reported)
        == Some("esp32c6");
    let classified = LayoutProbe::replay(target.clone(), chip_is_c6, &inspection.reads);
    let base_mac = inspection
        .probed_mac
        .as_deref()
        .and_then(normalize_base_mac);
    let decision = decide(
        &classified,
        &target,
        inspection.target_image_len,
        pending.as_ref().map(|p| p.tree),
        restore_requested,
    );
    let plain = || LayoutStaging {
        verdict: LayoutVerdict::Plain,
        plan: None,
        archive: None,
        restoring: None,
        downloaded: false,
    };
    match decision {
        Ok(LayoutDecision::Plain(_)) => Ok(plain()),
        Ok(LayoutDecision::Migrate {
            mut plan,
            summary,
            tree,
        }) => {
            plan.base_mac = base_mac.clone();
            let source = summary.source.unwrap_or(summary.target);
            let archive = archive_of(
                &tree,
                inspection.chip_name.as_deref(),
                base_mac.as_deref(),
                source,
                Some(summary.target),
                BackupPurpose::LayoutMigration,
                now_secs,
            )?;
            Ok(LayoutStaging {
                verdict: LayoutVerdict::Migrate {
                    files: summary.file_count,
                    bytes: summary.total_bytes,
                    free_blocks: summary.blocks_free,
                    tight: summary.tight,
                    backup_stored: false,
                    device_uid: summary.device_uid,
                },
                plan: Some(plan),
                archive: Some(archive),
                restoring: None,
                downloaded: false,
            })
        }
        Ok(LayoutDecision::Restore { mut plan, summary }) => {
            plan.base_mac = base_mac;
            let entry = pending.map(|p| p.entry);
            Ok(LayoutStaging {
                verdict: LayoutVerdict::Restore {
                    captured_at: entry
                        .map(|e| e.captured_at_epoch_seconds as u64)
                        .unwrap_or(0),
                    files: summary.file_count,
                    bytes: summary.total_bytes,
                    device_uid: summary.device_uid,
                },
                plan: Some(plan),
                archive: None,
                restoring: entry.map(|e| e.archive.clone()),
                downloaded: false,
            })
        }
        Err(refusal @ (Refusal::DoesNotFit { .. } | Refusal::TooTight { .. })) => {
            let target_geometry =
                LpfsGeometry::from_table(&target).ok_or("the firmware has no filesystem")?;
            // A refused board keeps everything; the backup is still offered
            // as a download (the user is about to remove a project).
            let archive = classified.source.as_ref().and_then(|(geometry, image)| {
                let (tree, _) = LpfsTree::from_image(image, *geometry).ok()?;
                archive_of(
                    &tree,
                    inspection.chip_name.as_deref(),
                    base_mac.as_deref(),
                    *geometry,
                    Some(target_geometry),
                    BackupPurpose::Backup,
                    now_secs,
                )
                .ok()
            });
            // The planner's own measure: blocks, and the reserve it keeps.
            let (files, blocks_needed) = match refusal {
                Refusal::TooTight {
                    files, blocks_used, ..
                } => (files, Some(blocks_used)),
                Refusal::DoesNotFit { files, .. } => (files, None),
                _ => unreachable!("matched above"),
            };
            Ok(LayoutStaging {
                verdict: LayoutVerdict::Refused {
                    files,
                    blocks_needed,
                    blocks_total: target_geometry.block_count,
                    blocks_reserved: FREE_BLOCK_FLOOR,
                    block_bytes: LPFS_BLOCK_SIZE,
                },
                plan: None,
                archive,
                restoring: None,
                downloaded: false,
            })
        }
        Err(refusal) => Err(refusal.to_string()),
    }
}

fn archive_of(
    tree: &LpfsTree,
    chip: Option<&str>,
    base_mac: Option<&str>,
    source: LpfsGeometry,
    target: Option<LpfsGeometry>,
    purpose: BackupPurpose,
    now_secs: f64,
) -> Result<StagedArchive, String> {
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        captured_at_epoch_seconds: now_secs,
        device_uid: tree.device_uid(),
        chip: chip.map(str::to_string),
        base_mac: base_mac.map(str::to_string),
        partition_offset: source.offset,
        partition_length: source.len(),
        target_partition_offset: target.map(|t| t.offset),
        target_partition_length: target.map(|t| t.len()),
        block_size: 4096,
        file_count: tree.file_count(),
        total_bytes: tree.total_bytes(),
        purpose,
    };
    let bytes = write_archive(tree, &manifest).map_err(|error| error.to_string())?;
    let entry = BackupEntry {
        base_mac: base_mac.unwrap_or("unknown").to_string(),
        archive: format!(
            "{}-{}",
            base_mac.unwrap_or("unknown").replace(':', ""),
            backup_file_name(None, now_secs).trim_start_matches("lightplayer-backup-device-")
        ),
        captured_at_epoch_seconds: now_secs,
        purpose: match purpose {
            BackupPurpose::Backup => "backup",
            BackupPurpose::LayoutMigration => "layout-migration",
        }
        .to_string(),
        status: BackupStatus::Pending,
        file_count: tree.file_count(),
        total_bytes: tree.total_bytes(),
    };
    Ok(StagedArchive { entry, bytes })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lpa_link::layout_migration::{LEGACY_C6_V1_LPFS, build_image, legacy_c6_v1_table};

    pub(crate) fn d1() -> PartitionTable {
        let mut entries = legacy_c6_v1_table().entries().to_vec();
        entries[3].size = 0x34_0000;
        entries[4].offset = 0x35_0000;
        entries[4].size = 0xB_0000;
        PartitionTable::new(entries)
    }

    pub(crate) fn tree(files: usize) -> LpfsTree {
        let mut all = vec![
            ("/hardware.json".to_string(), b"{}".to_vec()),
            (
                "/.lp/device.json".to_string(),
                br#"{"uid":"dev0000000000000042"}"#.to_vec(),
            ),
        ];
        all.extend((0..files).map(|i| (format!("/projects/p/f{i:03}"), vec![1u8; 3000])));
        LpfsTree::from_files(all)
    }

    pub(crate) fn inspection_of(flash: &[u8]) -> LinkLayoutInspection {
        let target = d1();
        let mut probe = LayoutProbe::new(target.clone(), true);
        let mut reads = Vec::new();
        while let Some(read) = probe.next_read() {
            let at = read.offset as usize;
            let bytes = flash[at..at + read.length as usize].to_vec();
            probe.record(read, bytes.clone());
            reads.push((read, bytes));
        }
        LinkLayoutInspection {
            chip_name: Some("esp32c6".to_string()),
            probed_mac: Some("10:BD:A3:B0:8E:30".to_string()),
            target_table: target.to_bytes(),
            target_image_len: 0x2D_0000,
            reads,
            logs: Vec::new(),
            progress: Vec::new(),
        }
    }

    pub(crate) fn legacy_chip(tree: &LpfsTree) -> Vec<u8> {
        let mut flash = vec![0xFFu8; 0x40_0000];
        let table = legacy_c6_v1_table().to_bytes();
        flash[0x8000..0x8000 + table.len()].copy_from_slice(&table);
        let image = build_image(tree, LEGACY_C6_V1_LPFS).unwrap();
        flash[0x31_0000..].copy_from_slice(&image);
        flash
    }

    #[test]
    fn a_legacy_board_stages_a_migration_with_its_backup_and_its_mac() {
        let staging = stage_layout(
            &inspection_of(&legacy_chip(&tree(3))),
            None,
            false,
            1_800_000_000.0,
        )
        .unwrap();
        let LayoutVerdict::Migrate {
            files,
            backup_stored,
            device_uid,
            ..
        } = &staging.verdict
        else {
            panic!("{:?}", staging.verdict);
        };
        assert_eq!(*files, 5);
        assert!(!backup_stored, "not stored until the store says so");
        assert_eq!(device_uid.as_deref(), Some("dev0000000000000042"));
        let plan = staging.plan.as_ref().unwrap();
        assert_eq!(plan.base_mac.as_deref(), Some("10:bd:a3:b0:8e:30"));
        assert!(!plan.backup_confirmed);
        let archive = staging.archive.as_ref().unwrap();
        assert_eq!(archive.entry.status, BackupStatus::Pending);
        let (manifest, back) =
            lpa_link::layout_migration::device_backup_archive::read_archive(&archive.bytes)
                .unwrap();
        assert_eq!(back, tree(3));
        assert_eq!(manifest.base_mac.as_deref(), Some("10:bd:a3:b0:8e:30"));
        // Not confirmed until stored or downloaded.
        assert!(staging.confirmed_plan().is_err());
        let mut downloaded = staging.clone();
        downloaded.downloaded = true;
        assert!(downloaded.confirmed_plan().unwrap().backup_confirmed);
    }

    #[test]
    fn a_crowded_board_is_refused_with_its_backup_offered() {
        let staging =
            stage_layout(&inspection_of(&legacy_chip(&tree(190))), None, false, 1.0).unwrap();
        assert!(matches!(staging.verdict, LayoutVerdict::Refused { .. }));
        assert!(staging.plan.is_none());
        assert!(staging.archive.is_some());
    }

    #[test]
    fn an_empty_current_board_with_a_pending_backup_stages_a_restore() {
        let mut flash = vec![0xFFu8; 0x40_0000];
        let table = d1().to_bytes();
        flash[0x8000..0x8000 + table.len()].copy_from_slice(&table);
        let files = tree(2);
        let entry = BackupEntry {
            base_mac: "10:bd:a3:b0:8e:30".to_string(),
            archive: "x.zip".to_string(),
            captured_at_epoch_seconds: 5.0,
            purpose: "layout-migration".to_string(),
            status: BackupStatus::Pending,
            file_count: 4,
            total_bytes: 1,
        };
        let staging = stage_layout(
            &inspection_of(&flash),
            Some(PendingBackup {
                entry: &entry,
                tree: &files,
            }),
            false,
            9.0,
        )
        .unwrap();
        assert!(matches!(
            staging.verdict,
            LayoutVerdict::Restore { captured_at: 5, .. }
        ));
        assert_eq!(staging.restoring.as_deref(), Some("x.zip"));
        assert!(
            staging.confirmed_plan().is_ok(),
            "a restore's backup is stored"
        );
    }
}
