//! The scripted board's flash, as a layout migration sees it.
//!
//! The fake keeps no flash between operations — its truth is the scripted
//! boot state. So a layout operation builds the 4 MB image that state
//! implies (the partition table of its [`FakeFlashLayout`] at `0x8000`, a
//! REAL littlefs image of its files at that layout's `lpfs`), runs the same
//! pure code the real providers run against it
//! ([`crate::layout_migration`]), and — after a plan — boots whatever the
//! image now holds, the way firmware would: files that mount, a legacy
//! filesystem held, or a freshly formatted one.
//!
//! The fake firmware package carries the post-repartition C6 layout
//! ([`fake_target_table`]), independent of `partitions.csv`: the fake exists
//! to exercise migration, not to mirror whichever table ships today.

use crate::layout_migration::lpfs_repack::write_tree_image;
use crate::layout_migration::{
    LayoutProbe, LayoutState, LpfsGeometry, LpfsTree, apply_steps, inspect_flash,
    legacy_c6_v1_table,
};
use crate::provider::flash_plan::FlashPlan;
use crate::{
    LinkError, LinkFlashRegion, LinkLayoutInspection, LinkManagementProgress,
    LinkRawFilesystemReadResult, PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET, PartitionTable,
};

use super::fake_device_core::FakeEsp32Device;
use super::fake_device_script::{
    FAKE_IMAGE_IDENTITY, FakeBootState, FakeFlashLayout, FakeLightPlayerState, fake_provenance,
};

/// What the scripted device answers to a chip probe during a layout
/// operation.
const FAKE_CHIP_NAME: &str = "ESP32-C6 (fake)";

/// The scripted chip's flash size.
const FAKE_FLASH_LEN: usize = 0x40_0000;

/// The fake firmware package's merged image length: below the legacy
/// filesystem, like today's real C6 image.
pub const FAKE_FIRMWARE_LEN: u32 = 0x2D_0000;

/// The partition table the fake firmware package carries: the
/// post-repartition C6 layout (factory `0x340000`, `lpfs` `0x350000` +
/// `0xB0000`).
pub fn fake_target_table() -> PartitionTable {
    let mut entries = legacy_c6_v1_table().entries().to_vec();
    entries[3].size = 0x34_0000;
    entries[4].offset = 0x35_0000;
    entries[4].size = 0xB_0000;
    PartitionTable::new(entries)
}

/// The fake package's merged image: filler with the target table at `0x8000`.
pub fn fake_firmware_image() -> Vec<u8> {
    let mut image = vec![0x5Au8; FAKE_FIRMWARE_LEN as usize];
    let table = fake_target_table().to_bytes();
    let at = PARTITION_TABLE_OFFSET as usize;
    image[at..at + PARTITION_TABLE_LEN].copy_from_slice(&table);
    image
}

impl FakeEsp32Device {
    /// `InspectLayout` on the scripted board.
    pub fn fake_inspect_layout(&self) -> LinkLayoutInspection {
        let flash = flash_image_of(&self.flash_state());
        let mut probe = LayoutProbe::new(fake_target_table(), true);
        let mut reads = Vec::new();
        while let Some(read) = probe.next_read() {
            let at = read.offset as usize;
            let bytes = flash[at..at + read.length as usize].to_vec();
            probe.record(read, bytes.clone());
            reads.push((read, bytes));
        }
        let (efuse_mac, _) = self.board_constants();
        LinkLayoutInspection {
            chip_name: Some(FAKE_CHIP_NAME.to_string()),
            probed_mac: efuse_mac,
            target_table: fake_target_table().to_bytes(),
            target_image_len: FAKE_FIRMWARE_LEN,
            reads,
            logs: vec!["fake layout inspection".to_string()],
            progress: vec![
                LinkManagementProgress::new("Reading the board's layout").with_percent(100),
            ],
        }
    }

    /// `FlashFirmware` with a plan on the scripted board: the steps applied
    /// to its flash image (stopping early when
    /// [`FakeEsp32Device::interrupt_next_plan_after`] says so), then a boot of
    /// whatever the image holds.
    pub fn fake_execute_plan(&self, plan: &FlashPlan) -> Result<(), LinkError> {
        if !plan.may_execute() {
            return Err(LinkError::other(
                crate::PlanError::BackupNotConfirmed.to_string(),
            ));
        }
        let (efuse_mac, link_config) = self.board_constants();
        if let Some(refusal) = plan.refuse_board(efuse_mac.as_deref()) {
            return Err(LinkError::other(refusal));
        }
        let mut flash = flash_image_of(&self.flash_state());
        let firmware = fake_firmware_image();
        let limit = self.take_plan_interrupt();
        let steps = &plan.steps[..limit.unwrap_or(plan.steps.len()).min(plan.steps.len())];
        let result = apply_steps(&mut flash, steps, &firmware);
        let wrote_firmware = steps
            .iter()
            .any(|step| matches!(step, crate::FlashStep::WriteFirmware));
        // A pull before the firmware write leaves the old firmware booting
        // its old filesystem — nothing to replace. (A retire-first plan's
        // first step is the retire; the old firmware then boots and its own
        // mount fails — not modelled: the fake's old firmware is the
        // scripted state.)
        if wrote_firmware || plan.steps.first() != Some(&crate::FlashStep::WriteFirmware) {
            self.replace_boot(FakeBootState::LightPlayer(boot_from_flash(
                flash,
                efuse_mac,
                link_config,
            )));
        }
        if limit.is_some_and(|n| n < plan.steps.len()) {
            return Err(LinkError::other("the board went away mid-write (scripted)"));
        }
        result.map_err(|error| LinkError::other(error.to_string()))
    }

    /// Power-cycle the scripted board: boot again from what its flash holds
    /// now, the way firmware would. A board that formatted after a pulled
    /// cable boots its (empty) filesystem this time as `mounted` — the
    /// later boot the migration walk's W7b covers. A board whose flash was
    /// never written by a plan just replays its scripted boot.
    pub fn fake_power_cycle(&self) {
        let FakeBootState::LightPlayer(lp) = self.flash_state() else {
            self.reset_runtime();
            return;
        };
        let Some(flash) = lp.flash.as_ref() else {
            self.reset_runtime();
            return;
        };
        let (efuse_mac, link_config) = self.board_constants();
        self.replace_boot(FakeBootState::LightPlayer(boot_from_flash(
            flash.as_ref().clone(),
            efuse_mac,
            link_config,
        )));
    }

    /// Every file the scripted board holds right now (absolute paths), and
    /// the filesystem state its next hello reports — what a test asserts
    /// after a plan ran.
    pub fn fake_board_files(&self) -> (Vec<(String, Vec<u8>)>, Option<lpc_wire::FsBootState>) {
        match self.flash_state() {
            FakeBootState::LightPlayer(lp) => (
                tree_of(&lp)
                    .files()
                    .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
                    .collect(),
                Some(lp.fs_boot_state),
            ),
            _ => (Vec::new(), None),
        }
    }

    /// `ReadRawFilesystem` on the scripted board: its table, then its
    /// `lpfs` row's region.
    pub(crate) fn fake_read_raw_filesystem(
        &self,
    ) -> Result<LinkRawFilesystemReadResult, LinkError> {
        let flash = flash_image_of(&self.flash_state());
        let at = PARTITION_TABLE_OFFSET as usize;
        let partition_table = flash[at..at + PARTITION_TABLE_LEN].to_vec();
        let region = PartitionTable::parse(&partition_table)
            .ok()
            .as_ref()
            .and_then(LinkFlashRegion::lpfs_in)
            .ok_or_else(|| {
                LinkError::other("the board holds no LightPlayer filesystem partition")
            })?;
        let start = region.offset as usize;
        let image = flash[start..start + region.length as usize].to_vec();
        Ok(LinkRawFilesystemReadResult {
            logs: vec![format!(
                "fake filesystem read: {} bytes at {:#x}",
                image.len(),
                region.offset
            )],
            progress: vec![
                LinkManagementProgress::new("Reading filesystem")
                    .with_steps(region.length, region.length)
                    .with_percent(100),
            ],
            image,
            region,
            partition_table,
            chip_name: Some(FAKE_CHIP_NAME.to_string()),
        })
    }
}

/// The flash a boot state implies.
fn flash_image_of(boot: &FakeBootState) -> Vec<u8> {
    let mut flash = vec![0xFFu8; FAKE_FLASH_LEN];
    let FakeBootState::LightPlayer(lp) = boot else {
        return flash;
    };
    if let Some(written) = &lp.flash {
        return written.as_ref().clone();
    }
    let table = match lp.layout {
        FakeFlashLayout::Current => fake_target_table(),
        FakeFlashLayout::Legacy => legacy_c6_v1_table(),
    };
    let bytes = table.to_bytes();
    let at = PARTITION_TABLE_OFFSET as usize;
    flash[at..at + bytes.len()].copy_from_slice(&bytes);
    // Firmware below the filesystem, so a probe sees a plausible board.
    flash[0x1_0000..0x2_0000].fill(0x5A);
    let geometry = LpfsGeometry::from_table(&table).expect("the fake tables declare lpfs");
    let image = write_tree_image(&tree_of(lp), geometry).expect("the fake board's files fit");
    let start = geometry.offset as usize;
    flash[start..start + image.len()].copy_from_slice(&image);
    flash
}

/// Every file the scripted board holds, as a tree.
fn tree_of(lp: &FakeLightPlayerState) -> LpfsTree {
    let mut files: Vec<(String, Vec<u8>)> = lp
        .project_files
        .iter()
        .map(|(relative, bytes)| (format!("{}/{relative}", lp.project_dir), bytes.clone()))
        .collect();
    files.extend(lp.root_files.iter().cloned());
    if let Some(identity) = &lp.identity
        && let Ok(json) = lpc_wire::json::to_string(identity)
    {
        files.push((fw_host::DEVICE_IDENTITY_PATH.to_string(), json.into_bytes()));
    }
    LpfsTree::from_files(files)
}

/// What the new firmware boots from `flash`, as firmware does it: its files
/// when they mount; a held legacy filesystem (the legacy guard — nothing
/// written); or, with neither, a filesystem formatted fresh (which the
/// returned state's flash then holds).
fn boot_from_flash(
    mut flash: Vec<u8>,
    base_mac: Option<String>,
    link_config: lpc_wire::lp_link::LinkConfig,
) -> FakeLightPlayerState {
    let mut state = FakeLightPlayerState {
        provenance: fake_provenance(FAKE_IMAGE_IDENTITY),
        base_mac,
        link_config,
        ..FakeLightPlayerState::new()
    };
    let target = fake_target_table();
    let geometry = LpfsGeometry::from_table(&target).expect("the fake table declares lpfs");
    let start = geometry.offset as usize;
    let end = start + geometry.len() as usize;
    let mounted = LpfsTree::from_image(&flash[start..end], geometry).ok();
    state.fs_boot_state = match mounted {
        Some((tree, _)) => {
            state.root_files = tree
                .files()
                .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
                .collect();
            lpc_wire::FsBootState::Mounted
        }
        None if inspect_flash(&flash, target, true).state == LayoutState::CurrentLegacyPresent => {
            lpc_wire::FsBootState::LegacyHeld
        }
        None => {
            let empty =
                write_tree_image(&LpfsTree::default(), geometry).expect("an empty filesystem fits");
            flash[start..end].copy_from_slice(&empty);
            lpc_wire::FsBootState::Formatted
        }
    };
    state.flash = Some(std::sync::Arc::new(flash));
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_migration::{LayoutDecision, decide};

    fn legacy_board() -> FakeEsp32Device {
        FakeEsp32Device::new(super::super::fake_device_script::FakeDeviceScript::new(
            FakeBootState::LightPlayer(
                FakeLightPlayerState::new()
                    .with_base_mac("60:55:f9:0a:0b:0c")
                    .with_project_files(vec![("project.json".to_string(), b"{}".to_vec())])
                    .with_root_files(vec![(
                        "/hardware.json".to_string(),
                        b"{\"board\":1}".to_vec(),
                    )])
                    .with_identity(super::super::fake_device_script::FakeDeviceIdentity::new(
                        "dev0000000000000011",
                        "Porch",
                    ))
                    .with_legacy_layout(),
            ),
        ))
    }

    fn plan_for(device: &FakeEsp32Device) -> FlashPlan {
        let inspection = device.fake_inspect_layout();
        let classified = LayoutProbe::replay(fake_target_table(), true, &inspection.reads);
        assert_eq!(classified.state, LayoutState::Legacy);
        match decide(
            &classified,
            &fake_target_table(),
            FAKE_FIRMWARE_LEN,
            None,
            false,
        )
        .unwrap()
        {
            LayoutDecision::Migrate { mut plan, .. } => {
                plan.backup_confirmed = true;
                plan
            }
            other => panic!("expected a migration, got {other:?}"),
        }
    }

    #[test]
    fn a_legacy_board_migrates_and_boots_mounted_with_every_file() {
        let device = legacy_board();
        let plan = plan_for(&device);
        device.fake_execute_plan(&plan).unwrap();
        let FakeBootState::LightPlayer(lp) = device.flash_state() else {
            panic!("a LightPlayer board");
        };
        assert_eq!(lp.fs_boot_state, lpc_wire::FsBootState::Mounted);
        let paths: Vec<&str> = lp.root_files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"/hardware.json"));
        assert!(paths.contains(&"/.lp/device.json"));
        assert!(paths.contains(&"/projects/studio/project.json"));
        assert_eq!(
            LayoutProbe::replay(
                fake_target_table(),
                true,
                &device.fake_inspect_layout().reads
            )
            .state,
            LayoutState::CurrentMounts
        );
    }

    #[test]
    fn a_pull_after_the_firmware_write_boots_held() {
        let device = legacy_board();
        let plan = plan_for(&device);
        device.interrupt_next_plan_after(1);
        assert!(device.fake_execute_plan(&plan).is_err());
        let FakeBootState::LightPlayer(lp) = device.flash_state() else {
            panic!("a LightPlayer board");
        };
        assert_eq!(lp.fs_boot_state, lpc_wire::FsBootState::LegacyHeld);
        // …and the held board still migrates from its legacy filesystem.
        let inspection = LayoutProbe::replay(
            fake_target_table(),
            true,
            &device.fake_inspect_layout().reads,
        );
        assert_eq!(inspection.state, LayoutState::CurrentLegacyPresent);
    }

    #[test]
    fn a_pull_during_the_filesystem_write_boots_formatted() {
        let device = legacy_board();
        let plan = plan_for(&device);
        device.interrupt_next_plan_after(4);
        assert!(device.fake_execute_plan(&plan).is_err());
        let FakeBootState::LightPlayer(lp) = device.flash_state() else {
            panic!("a LightPlayer board");
        };
        assert_eq!(lp.fs_boot_state, lpc_wire::FsBootState::Formatted);
    }

    /// The boot after the one that formatted mounts that empty filesystem:
    /// `mounted`, with no files and so no identity (the walk's W7b).
    #[test]
    fn a_power_cycle_after_the_format_mounts_the_empty_filesystem() {
        let device = legacy_board();
        let plan = plan_for(&device);
        device.interrupt_next_plan_after(4);
        assert!(device.fake_execute_plan(&plan).is_err());
        device.fake_power_cycle();
        let (files, fs) = device.fake_board_files();
        assert_eq!(fs, Some(lpc_wire::FsBootState::Mounted));
        assert!(files.is_empty(), "{files:?}");
    }

    #[test]
    fn a_plan_for_another_board_is_refused() {
        let device = legacy_board();
        let mut plan = plan_for(&device);
        plan.base_mac = Some("aa:bb:cc:dd:ee:ff".to_string());
        assert!(device.fake_execute_plan(&plan).is_err());
        let FakeBootState::LightPlayer(lp) = device.flash_state() else {
            panic!("a LightPlayer board");
        };
        assert_eq!(lp.layout, FakeFlashLayout::Legacy, "nothing was written");
    }
}
