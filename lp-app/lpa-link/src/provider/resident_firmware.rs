//! [`ResidentFirmware`]: what a board's flash already holds, read in the
//! bootloader before anything is written over it.
//!
//! Installing LightPlayer onto a blank board loses nothing, and onto a
//! LightPlayer board keeps its files; onto somebody else's firmware it
//! replaces that firmware, which is the user's call. The difference is read
//! from the board, not guessed from what it printed: the passive evidence
//! (a boot banner, a hello) is missing exactly when a board is plugged in
//! mid-stream or runs a quiet firmware.
//!
//! Two reads, both in the inspection's one session:
//!
//! - the partition table at `0x8000` (the layout probe's first read, which
//!   is reused rather than repeated). Erased → [`ResidentFirmware::Blank`].
//!   A table with an `lpfs` partition → [`ResidentFirmware::LightPlayer`]:
//!   every LightPlayer layout has one, and nobody else's does.
//! - otherwise the first app partition's first bytes, for its application
//!   description ([`AppImage`]) → [`ResidentFirmware::Foreign`] with what it
//!   calls itself, or [`ResidentFirmware::Unrecognized`].
//!
//! Stateless over the session's read list, so the app side replays the same
//! answer from the same bytes ([`ResidentFirmware::from_reads`]) and the
//! executor only ever asks [`next_resident_read`] what to read next.

use serde::{Deserialize, Serialize};

use super::app_descriptor::{APP_HEADER_READ_LEN, AppImage};
use super::flash_region::LinkFlashRegion;
use super::partition_table::{
    PARTITION_TABLE_LEN, PARTITION_TABLE_OFFSET, PartitionEntry, PartitionTable,
    PartitionTableError,
};

/// The partition table read the classification starts from.
pub const RESIDENT_TABLE_READ: LinkFlashRegion = LinkFlashRegion {
    offset: PARTITION_TABLE_OFFSET,
    length: PARTITION_TABLE_LEN as u32,
};

/// What a board's flash holds.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ResidentFirmware {
    /// No partition table: nothing to lose.
    Blank,
    /// A LightPlayer layout (any version): an install keeps its files.
    LightPlayer,
    /// Somebody else's firmware, as its application description names it.
    Foreign {
        project_name: Option<String>,
        version: Option<String>,
    },
    /// Something is there, and it does not say what: a damaged table, an
    /// app with no description, or no app at all.
    Unrecognized,
}

/// The next read the classification needs, given the session's `reads` so
/// far, or `None` when it has what it needs. Asks for the table only when
/// no read covered it yet.
pub fn next_resident_read(reads: &[(LinkFlashRegion, Vec<u8>)]) -> Option<LinkFlashRegion> {
    let Some(table) = bytes_of(reads, RESIDENT_TABLE_READ) else {
        return Some(RESIDENT_TABLE_READ);
    };
    let app = match PartitionTable::parse(table) {
        Ok(table) if table.find("lpfs").is_none() => first_app(&table)?.clone(),
        _ => return None,
    };
    let read = app_header_read(&app);
    bytes_of(reads, read).is_none().then_some(read)
}

impl ResidentFirmware {
    /// Classify from the session's reads. Missing reads read as
    /// [`Self::Unrecognized`] — never as blank, which would skip the
    /// question.
    pub fn from_reads(reads: &[(LinkFlashRegion, Vec<u8>)]) -> Self {
        let Some(table) = bytes_of(reads, RESIDENT_TABLE_READ) else {
            return Self::Unrecognized;
        };
        let table = match PartitionTable::parse(table) {
            Ok(table) => table,
            Err(PartitionTableError::Blank) => return Self::Blank,
            Err(_) => return Self::Unrecognized,
        };
        if table.find("lpfs").is_some() {
            return Self::LightPlayer;
        }
        let Some(app) = first_app(&table) else {
            return Self::Unrecognized;
        };
        match bytes_of(reads, app_header_read(app)).map(AppImage::parse) {
            Some(AppImage::Described(desc)) => Self::Foreign {
                project_name: desc.project_name,
                version: desc.version,
            },
            // A table with an erased app is still somebody's layout, and
            // its data partitions may hold their files.
            Some(AppImage::Erased | AppImage::Undescribed) | None => Self::Unrecognized,
        }
    }

    /// Whether writing LightPlayer here replaces somebody else's firmware —
    /// the question the install asks first.
    pub fn is_replaced_by_install(&self) -> bool {
        matches!(self, Self::Foreign { .. } | Self::Unrecognized)
    }
}

/// The app partition a bootloader would start: `factory` when there is
/// one, else the lowest-addressed app.
fn first_app(table: &PartitionTable) -> Option<&PartitionEntry> {
    let apps = || table.entries().iter().filter(|entry| entry.kind == 0);
    apps()
        .find(|entry| entry.subtype == 0)
        .or_else(|| apps().min_by_key(|entry| entry.offset))
}

fn app_header_read(app: &PartitionEntry) -> LinkFlashRegion {
    LinkFlashRegion {
        offset: app.offset,
        length: APP_HEADER_READ_LEN.min(app.size),
    }
}

/// The bytes a read of exactly `region` returned, if the session made it.
fn bytes_of(reads: &[(LinkFlashRegion, Vec<u8>)], region: LinkFlashRegion) -> Option<&[u8]> {
    reads
        .iter()
        .find(|(read, bytes)| *read == region && bytes.len() == region.length as usize)
        .map(|(_, bytes)| bytes.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::app_descriptor::tests::image_with_description;

    const LIGHTPLAYER_C6: &str = include_str!("../../testdata/partitions-esp32c6-legacy-v1.csv");

    #[test]
    fn an_erased_table_is_a_blank_board_after_one_read() {
        let reads = vec![(RESIDENT_TABLE_READ, vec![0xFF; PARTITION_TABLE_LEN])];
        assert_eq!(next_resident_read(&[]), Some(RESIDENT_TABLE_READ));
        assert_eq!(next_resident_read(&reads), None);
        assert_eq!(
            ResidentFirmware::from_reads(&reads),
            ResidentFirmware::Blank
        );
        assert!(!ResidentFirmware::Blank.is_replaced_by_install());
    }

    #[test]
    fn a_table_with_lpfs_is_lightplayer_and_reads_no_app() {
        let table = PartitionTable::from_csv(LIGHTPLAYER_C6).unwrap().to_bytes();
        let reads = vec![(RESIDENT_TABLE_READ, table)];
        assert_eq!(next_resident_read(&reads), None);
        assert_eq!(
            ResidentFirmware::from_reads(&reads),
            ResidentFirmware::LightPlayer
        );
    }

    #[test]
    fn somebody_elses_firmware_is_named_from_its_factory_app() {
        let table = esp_idf_table();
        let app_read = LinkFlashRegion {
            offset: 0x10000,
            length: APP_HEADER_READ_LEN,
        };
        let mut reads = vec![(RESIDENT_TABLE_READ, table)];
        assert_eq!(next_resident_read(&reads), Some(app_read));
        reads.push((app_read, image_with_description("WLED", "0.15.0")));
        assert_eq!(next_resident_read(&reads), None);
        let resident = ResidentFirmware::from_reads(&reads);
        assert_eq!(
            resident,
            ResidentFirmware::Foreign {
                project_name: Some("WLED".into()),
                version: Some("0.15.0".into()),
            }
        );
        assert!(resident.is_replaced_by_install());
    }

    #[test]
    fn an_ota_only_table_reads_its_lowest_app() {
        let table = PartitionTable::new(vec![
            entry("nvs", 1, 2, 0x9000, 0x5000),
            entry("ota_1", 0, 0x11, 0x210000, 0x200000),
            entry("ota_0", 0, 0x10, 0x10000, 0x200000),
        ])
        .to_bytes();
        let reads = vec![(RESIDENT_TABLE_READ, table)];
        assert_eq!(
            next_resident_read(&reads),
            Some(LinkFlashRegion {
                offset: 0x10000,
                length: APP_HEADER_READ_LEN,
            })
        );
    }

    #[test]
    fn what_does_not_say_what_it_is_is_unrecognized_never_blank() {
        let garbage = vec![(RESIDENT_TABLE_READ, vec![0x12; PARTITION_TABLE_LEN])];
        assert_eq!(next_resident_read(&garbage), None);
        assert_eq!(
            ResidentFirmware::from_reads(&garbage),
            ResidentFirmware::Unrecognized
        );
        assert_eq!(
            ResidentFirmware::from_reads(&[]),
            ResidentFirmware::Unrecognized
        );

        let app_read = LinkFlashRegion {
            offset: 0x10000,
            length: APP_HEADER_READ_LEN,
        };
        for app in [vec![0xFF; 0x120], vec![0xE9; 0x120]] {
            let reads = vec![(RESIDENT_TABLE_READ, esp_idf_table()), (app_read, app)];
            assert_eq!(
                ResidentFirmware::from_reads(&reads),
                ResidentFirmware::Unrecognized
            );
        }
        // The app read never made (an executor that stopped early).
        let reads = vec![(RESIDENT_TABLE_READ, esp_idf_table())];
        assert_eq!(
            ResidentFirmware::from_reads(&reads),
            ResidentFirmware::Unrecognized
        );
    }

    /// ESP-IDF's default single-app table: nvs, phy_init, factory.
    fn esp_idf_table() -> Vec<u8> {
        PartitionTable::new(vec![
            entry("nvs", 1, 2, 0x9000, 0x6000),
            entry("phy_init", 1, 1, 0xF000, 0x1000),
            entry("factory", 0, 0, 0x10000, 0x100000),
        ])
        .to_bytes()
    }

    fn entry(label: &str, kind: u8, subtype: u8, offset: u32, size: u32) -> PartitionEntry {
        PartitionEntry {
            label: label.into(),
            kind,
            subtype,
            offset,
            size,
            flags: 0,
        }
    }
}
