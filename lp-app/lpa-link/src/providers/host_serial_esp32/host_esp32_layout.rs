//! Layout inspection and flash-plan execution over espflash — the host half
//! of the C6 repartition's migration (plan
//! `lp2025/2026-10-01-1843-c6-repartition`, P04).
//!
//! Decisions are not made here: [`crate::layout_migration`] classifies the
//! reads and plans the writes; this module reads what the probe asks for and
//! runs the plan's steps through [`crate::provider::flash_plan::run_plan`]
//! (order, readback, the one retry).
//!
//! Two shapes:
//!
//! - [`inspect_layout`] / [`execute_plan`] — the provider's two management
//!   requests, each its own bootloader session. Inspect ends WITHOUT a reset,
//!   so the chip stays in ROM download until the execute session (which
//!   resets into download again, never into the app).
//! - [`layout_session`] — inspect, decide, execute in ONE session, for
//!   `lp-cli hardware lpfs migrate`, where the caller is a process that can
//!   decide (and store a backup) inline. No gap at all between the read and
//!   the write.

use espflash::connection::reset::ResetAfterOperation;
use espflash::flasher::Flasher;

use super::host_esp32_flash::{
    EventRecorder, ProgressBridge, ResolvedImage, assert_chip_matches_manifest, chip_name, connect,
    load_manifest, manifest_chip, read_flash_region, restore_lp_analog_i2c_clock,
};
use crate::layout_migration::LayoutProbe;
use crate::provider::flash_plan::{FlashPlan, FlashStepTarget, run_plan};
use crate::{
    LinkError, LinkFirmwareFlashResult, LinkFirmwareManifest, LinkFlashRegion,
    LinkLayoutInspection, LinkManagementEventSink, LinkManagementProgress, PARTITION_TABLE_LEN,
    PARTITION_TABLE_OFFSET, PartitionTable,
};

/// What [`layout_session`] did.
#[derive(Debug)]
pub enum LayoutSessionOutcome {
    /// The caller decided not to write; the board was reset into its
    /// (unchanged) firmware.
    NotWritten(LinkLayoutInspection),
    /// The plan ran to the end and the board was reset.
    Written {
        inspection: LinkLayoutInspection,
        flash: LinkFirmwareFlashResult,
    },
}

/// Inspect the board on `port_name` against the package at `manifest_path`.
/// Leaves the chip in ROM download (see the module docs).
pub(super) fn inspect_layout(
    port_name: &str,
    manifest_path: &str,
    events: &LinkManagementEventSink,
) -> Result<LinkLayoutInspection, LinkError> {
    let mut recorder = EventRecorder::new(events);
    let package = Package::load(manifest_path)?;
    let mut flasher = connect(
        port_name,
        Some(package.chip),
        ResetAfterOperation::NoResetNoStub,
        &mut recorder,
    )?;
    let inspection = inspect_in_session(&mut flasher, &package, &mut recorder)?;
    recorder.log("Layout read; the board stays in its bootloader for the write");
    Ok(inspection)
}

/// Execute `plan` on the board on `port_name`, `WriteFirmware` writing the
/// package at `manifest_path`, then reset into the new firmware.
pub(super) fn execute_plan(
    port_name: &str,
    manifest_path: &str,
    plan: &FlashPlan,
    events: &LinkManagementEventSink,
) -> Result<LinkFirmwareFlashResult, LinkError> {
    let mut recorder = EventRecorder::new(events);
    if !plan.may_execute() {
        return Err(LinkError::other(
            crate::provider::flash_plan::PlanError::BackupNotConfirmed.to_string(),
        ));
    }
    let package = Package::load(manifest_path)?;
    let mut flasher = connect(
        port_name,
        Some(package.chip),
        ResetAfterOperation::NoResetNoStub,
        &mut recorder,
    )?;
    execute_in_session(&mut flasher, &package, plan, &mut recorder)
}

/// Inspect, let `decide` choose a plan (or none), and execute it — one
/// bootloader session. `decide` runs with the chip still in ROM download;
/// the caller stores its backup inside it and returns the plan with
/// `backup_confirmed` set.
pub fn layout_session(
    port_name: &str,
    manifest_path: &str,
    decide: impl FnOnce(&LinkLayoutInspection) -> Result<Option<FlashPlan>, LinkError>,
    events: &LinkManagementEventSink,
) -> Result<LayoutSessionOutcome, LinkError> {
    let mut recorder = EventRecorder::new(events);
    let package = Package::load(manifest_path)?;
    let mut flasher = connect(
        port_name,
        Some(package.chip),
        ResetAfterOperation::NoResetNoStub,
        &mut recorder,
    )?;
    let inspection = inspect_in_session(&mut flasher, &package, &mut recorder)?;
    // A refusal (or a decision not to write) still leaves the board running
    // its firmware, untouched.
    let plan = match decide(&inspection) {
        Ok(Some(plan)) => plan,
        Ok(None) => {
            reset(&mut flasher, &mut recorder)?;
            return Ok(LayoutSessionOutcome::NotWritten(inspection));
        }
        Err(error) => {
            let _ = reset(&mut flasher, &mut recorder);
            return Err(error);
        }
    };
    let flash = execute_in_session(&mut flasher, &package, &plan, &mut recorder)?;
    Ok(LayoutSessionOutcome::Written { inspection, flash })
}

/// Read the board's partition table (3 KB at `0x8000`) and reset it back
/// into its firmware — the layout preflight's whole read (`lp-cli hardware
/// lpfs preflight`).
pub fn read_partition_table(
    port_name: &str,
    events: &LinkManagementEventSink,
) -> Result<(Option<String>, Vec<u8>), LinkError> {
    let mut recorder = EventRecorder::new(events);
    let mut flasher = connect(
        port_name,
        None,
        ResetAfterOperation::NoResetNoStub,
        &mut recorder,
    )?;
    let chip = chip_name(&mut flasher);
    let table = read_flash_region(
        &mut flasher,
        LinkFlashRegion {
            offset: PARTITION_TABLE_OFFSET,
            length: PARTITION_TABLE_LEN as u32,
        },
        "Reading partition table",
        &mut recorder,
    )?;
    reset(&mut flasher, &mut recorder)?;
    Ok((chip, table))
}

/// Erase `length` bytes at `offset` and reset the board — the preflight's
/// `--discard-lpfs` for a test board whose files are not wanted.
pub fn erase_region(
    port_name: &str,
    offset: u32,
    length: u32,
    events: &LinkManagementEventSink,
) -> Result<(), LinkError> {
    let mut recorder = EventRecorder::new(events);
    let mut flasher = connect(
        port_name,
        None,
        ResetAfterOperation::NoResetNoStub,
        &mut recorder,
    )?;
    recorder.log(format!("Erasing {length:#x} bytes at {offset:#x}"));
    flasher
        .erase_region(offset, length)
        .map_err(|error| LinkError::other(format!("erase failed: {error}")))?;
    reset(&mut flasher, &mut recorder)
}

/// The package being written: its manifest, images, chip, and the target
/// table its merged image carries.
struct Package {
    manifest: LinkFirmwareManifest,
    images: Vec<(ResolvedImage, Vec<u8>)>,
    chip: espflash::targets::Chip,
    target_table: Vec<u8>,
    image_len: u32,
}

impl Package {
    fn load(manifest_path: &str) -> Result<Self, LinkError> {
        let (manifest, resolved) = load_manifest(manifest_path)?;
        let chip = manifest_chip(&manifest.target_chip).ok_or_else(|| {
            LinkError::other(format!(
                "firmware manifest {manifest_path} targets chip `{}`, which this build cannot \
                 flash",
                manifest.target_chip
            ))
        })?;
        let mut images = Vec::new();
        for image in resolved {
            let data = std::fs::read(&image.absolute_path).map_err(|error| {
                LinkError::other(format!(
                    "failed to read firmware image {}: {error}",
                    image.absolute_path.display()
                ))
            })?;
            images.push((image, data));
        }
        // The target layout is the package's own: the merged image's bytes at
        // the table offset — never a second copy of `partitions.csv`.
        let merged = images
            .iter()
            .find(|(image, _)| image.address == 0)
            .map(|(_, data)| data)
            .ok_or_else(|| LinkError::other("the firmware package has no merged image at 0x0"))?;
        let at = PARTITION_TABLE_OFFSET as usize;
        let target_table = merged
            .get(at..at + PARTITION_TABLE_LEN)
            .ok_or_else(|| {
                LinkError::other("the merged image is shorter than its partition table")
            })?
            .to_vec();
        PartitionTable::parse(&target_table).map_err(|error| {
            LinkError::other(format!("the firmware package's partition table: {error}"))
        })?;
        let image_len = images
            .iter()
            .map(|(image, data)| image.address + data.len() as u32)
            .max()
            .unwrap_or(0);
        Ok(Self {
            manifest,
            images,
            chip,
            target_table,
            image_len,
        })
    }
}

fn inspect_in_session(
    flasher: &mut Flasher,
    package: &Package,
    recorder: &mut EventRecorder,
) -> Result<LinkLayoutInspection, LinkError> {
    assert_chip_matches_manifest(flasher.chip(), &package.manifest)?;
    let chip = chip_name(flasher);
    let probed_mac = flasher.device_info().ok().map(|info| info.mac_address);
    let target = PartitionTable::parse(&package.target_table)
        .map_err(|error| LinkError::other(error.to_string()))?;
    let mut probe = LayoutProbe::new(target, flasher.chip() == espflash::targets::Chip::Esp32c6);
    let mut reads = Vec::new();
    while let Some(read) = probe.next_read() {
        let label = if read.length > 0x10000 {
            "Reading the board's files"
        } else {
            "Reading the board's layout"
        };
        let bytes = read_flash_region(flasher, read, label, recorder)?;
        probe.record(read, bytes.clone());
        reads.push((read, bytes));
    }
    Ok(LinkLayoutInspection {
        chip_name: chip,
        probed_mac,
        target_table: package.target_table.clone(),
        target_image_len: package.image_len,
        reads,
        logs: recorder.logs.clone(),
        progress: recorder.progress.clone(),
    })
}

fn execute_in_session(
    flasher: &mut Flasher,
    package: &Package,
    plan: &FlashPlan,
    recorder: &mut EventRecorder,
) -> Result<LinkFirmwareFlashResult, LinkError> {
    assert_chip_matches_manifest(flasher.chip(), &package.manifest)?;
    let chip = chip_name(flasher);
    let probed_mac = flasher.device_info().ok().map(|info| info.mac_address);
    if let Some(refusal) = plan.refuse_board(probed_mac.as_deref()) {
        return Err(LinkError::other(refusal));
    }
    {
        // Each step narrates itself through the recorder the target holds.
        let mut target = EspflashTarget {
            flasher,
            package,
            recorder,
        };
        run_plan(&mut target, plan, |_, _| {})
            .map_err(|error| LinkError::other(error.to_string()))?;
    }
    recorder.log("Plan complete");
    restore_lp_analog_i2c_clock(flasher, recorder);
    reset(flasher, recorder)?;
    Ok(LinkFirmwareFlashResult {
        manifest: package.manifest.clone(),
        chip_name: chip,
        probed_mac,
        logs: recorder.logs.clone(),
        progress: recorder.progress.clone(),
    })
}

fn reset(flasher: &mut Flasher, recorder: &mut EventRecorder) -> Result<(), LinkError> {
    recorder.log("Resetting device");
    flasher
        .connection()
        .reset()
        .map_err(|error| LinkError::other(format!("reset failed: {error}")))
}

/// [`FlashStepTarget`] over an open espflash session.
struct EspflashTarget<'a, 'b> {
    flasher: &'a mut Flasher,
    package: &'a Package,
    recorder: &'a mut EventRecorder<'b>,
}

impl FlashStepTarget for EspflashTarget<'_, '_> {
    type Error = String;

    fn write_firmware(&mut self) -> Result<(), String> {
        for (image, data) in &self.package.images {
            self.recorder.log(format!(
                "Writing {} bytes at 0x{:x}",
                data.len(),
                image.address
            ));
            let mut bridge = ProgressBridge::new(self.recorder, "Writing firmware".to_string());
            self.flasher
                .write_bin_to_flash(image.address, data, Some(&mut bridge))
                .map_err(|error| format!("firmware write failed: {error}"))?;
        }
        Ok(())
    }

    fn erase(&mut self, offset: u32, length: u32) -> Result<(), String> {
        self.recorder
            .progress(LinkManagementProgress::new("Moving files"));
        self.flasher
            .erase_region(offset, length)
            .map_err(|error| format!("erase {offset:#x}+{length:#x} failed: {error}"))
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), String> {
        let mut bridge = ProgressBridge::new(self.recorder, "Moving files".to_string());
        self.flasher
            .write_bin_to_flash(offset, bytes, Some(&mut bridge))
            .map_err(|error| format!("write at {offset:#x} failed: {error}"))
    }

    fn read(&mut self, offset: u32, length: u32) -> Result<Vec<u8>, String> {
        read_flash_region(
            self.flasher,
            LinkFlashRegion { offset, length },
            "Verifying files",
            self.recorder,
        )
        .map_err(|error| error.to_string())
    }
}
