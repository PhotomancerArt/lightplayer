//! What the install learned about a board, decided on the app side: the
//! inspection's raw facts in, the model's [`BoardIdentity`] out.
//!
//! The provider's bootloader session reports the chip it heard, the build it
//! chose, the flash chip's JEDEC id and the reads it made (plan
//! `lp2025/2026-10-28-0025-install-without-board-pick`). This turns them
//! into what the Flash activity needs: which board this is when exactly one
//! catalog board fits (`lpa_boards::match_boards`, over the chip, the flash
//! size and the link's USB bridge), every board that fits, and what the
//! board ran before (`lpa_link::ResidentFirmware`). Pure; the effects layer
//! sends the answer as a marker, the way it sends the layout verdict.

use lpa_devices::link::UsbIds;
use lpa_devices::{BoardIdentity, Resident};
use lpa_link::{LinkLayoutInspection, ResidentFirmware};

/// The identity `inspection` establishes for a board on a link with `usb`.
pub fn board_identity(inspection: &LinkLayoutInspection, usb: Option<UsbIds>) -> BoardIdentity {
    let chip = inspection
        .chip_name
        .as_deref()
        .and_then(lpa_link::chip_id_from_reported)
        .map(str::to_string);
    let matched = chip.as_deref().map(|chip| {
        lpa_boards::match_boards(&lpa_boards::BoardFingerprint {
            chip,
            flash_bytes: inspection
                .flash_id
                .and_then(lpa_link::flash_size_from_jedec_id),
            usb_vendor: usb.map(|usb| usb.vendor),
        })
    });
    BoardIdentity {
        chip,
        build_id: inspection.build_id.clone(),
        board_id: matched.as_ref().and_then(|m| m.board_id.clone()),
        candidates: matched.map(|m| m.candidates).unwrap_or_default(),
        resident: resident(ResidentFirmware::from_reads(&inspection.reads)),
    }
}

/// The model's mirror of what the board ran.
fn resident(found: ResidentFirmware) -> Resident {
    match found {
        ResidentFirmware::Blank => Resident::Blank,
        ResidentFirmware::LightPlayer => Resident::LightPlayer,
        ResidentFirmware::Foreign {
            project_name,
            version,
        } => Resident::Foreign {
            project_name,
            version,
        },
        ResidentFirmware::Unrecognized => Resident::Unrecognized,
    }
}

#[cfg(test)]
mod tests {
    use lpa_link::{FsKind, LinkFlashRegion, PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET};

    use super::*;

    const NATIVE: UsbIds = UsbIds {
        vendor: 0x303A,
        product: 0x1001,
    };

    #[test]
    fn a_blank_xiao_c6_is_named_by_its_four_megabytes() {
        let identity = board_identity(&inspection(Some(0x0016_4020)), Some(NATIVE));
        assert_eq!(identity.chip.as_deref(), Some("esp32c6"));
        assert_eq!(identity.build_id.as_deref(), Some("esp32c6-4mb"));
        assert_eq!(identity.board_id.as_deref(), Some("seeed/xiao-esp32-c6"));
        assert_eq!(identity.resident, Resident::Blank);
    }

    /// The C6 rev 2 bench finding: the id probe can read nothing over
    /// USB-Serial-JTAG. Both C6 boards fit, so nothing is guessed.
    #[test]
    fn a_c6_whose_flash_size_is_unknown_fits_both_boards() {
        let identity = board_identity(&inspection(None), Some(NATIVE));
        assert_eq!(identity.board_id, None);
        assert_eq!(
            identity.candidates,
            vec![
                "espressif/esp32-c6-devkitc-1".to_string(),
                "seeed/xiao-esp32-c6".to_string()
            ]
        );
    }

    #[test]
    fn a_chip_with_no_build_carries_none() {
        let mut no_build = inspection(None);
        no_build.build_id = None;
        no_build.chip_name = Some("ESP32-P4".to_string());
        let identity = board_identity(&no_build, None);
        assert_eq!(identity.chip.as_deref(), Some("esp32p4"));
        assert_eq!(identity.build_id, None);
        assert!(identity.candidates.is_empty());
    }

    fn inspection(flash_id: Option<u32>) -> LinkLayoutInspection {
        LinkLayoutInspection {
            chip_name: Some("ESP32-C6 (revision v0.2)".to_string()),
            probed_mac: None,
            build_id: Some("esp32c6-4mb".to_string()),
            flash_id,
            target_table: Vec::new(),
            target_image_len: 0,
            target_fs: FsKind::Tree,
            target_manifest: None,
            reads: vec![(
                LinkFlashRegion {
                    offset: PARTITION_TABLE_OFFSET,
                    length: PARTITION_TABLE_LEN as u32,
                },
                vec![0xFF; PARTITION_TABLE_LEN],
            )],
            logs: Vec::new(),
            progress: Vec::new(),
        }
    }
}
