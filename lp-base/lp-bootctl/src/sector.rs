//! The on-flash byte layout, and the order its bytes must be written in.

use crate::boot_control::{BootControl, DecodeOutcome};
use crate::boot_flags::BootFlags;
use crate::crc32::crc32;

/// Flash offset of the `bootctl` partition. Identical on every supported
/// board, so host writers and firmware readers agree without consulting a
/// partition table.
///
/// Kept in sync by hand with `lp-fw/fw-esp32c6/partitions.csv` and
/// `lp-fw/fw-esp32s3/partitions.csv`; `tests/partition_layout.rs` is the
/// guard.
pub const BOOTCTL_PARTITION_OFFSET: u32 = 0x0000_E000;

/// Size of the `bootctl` partition: one 4 KB flash erase sector.
pub const BOOTCTL_PARTITION_SIZE: u32 = 0x0000_1000;

/// Identifies an initialized record. Compared as bytes, so there is no
/// endianness to get wrong across the host writer and the device reader.
pub const SECTOR_MAGIC: [u8; 4] = *b"LPBC";

/// Bump on any layout change. Old records are **discarded, never migrated** —
/// a record this build does not understand decodes to a normal boot.
pub const SECTOR_VERSION: u16 = 1;

/// Bytes of the record. The rest of the sector is left erased.
pub const RECORD_LEN: usize = 16;

const MAGIC_RANGE: core::ops::Range<usize> = 0..4;
const VERSION_RANGE: core::ops::Range<usize> = 4..6;
const PAD_RANGE: core::ops::Range<usize> = 6..8;
const FLAGS_RANGE: core::ops::Range<usize> = 8..12;
const CRC_RANGE: core::ops::Range<usize> = 12..16;

/// Bytes the CRC covers: magic, version, pad, flags — everything but the
/// CRC itself.
const CRC_COVERED: core::ops::Range<usize> = 0..12;

/// Erased NOR flash reads as all-ones.
const ERASED_BYTE: u8 = 0xFF;

/// Encode a record for writing to an **erased** sector.
///
/// Write it in a single operation. Do not split it into multiple writes:
/// every flash-write API that can reach this sector (the ESP ROM/stub
/// `FLASH_BEGIN`, and therefore both `espflash` and `esptool-js`) **erases
/// the sectors it is about to write**, so a second write to the same sector
/// destroys the first. Integrity comes from the magic and CRC, not from
/// write ordering: any partial or interrupted write fails one of those two
/// checks and decodes as "no record", which boots normally.
pub fn encode_record(flags: BootFlags) -> [u8; RECORD_LEN] {
    let mut record = [0u8; RECORD_LEN];
    record[MAGIC_RANGE].copy_from_slice(&SECTOR_MAGIC);
    record[VERSION_RANGE].copy_from_slice(&SECTOR_VERSION.to_le_bytes());
    record[PAD_RANGE].copy_from_slice(&0u16.to_le_bytes());
    record[FLAGS_RANGE].copy_from_slice(&flags.bits().to_le_bytes());
    let crc = crc32(&record[CRC_COVERED]);
    record[CRC_RANGE].copy_from_slice(&crc.to_le_bytes());
    record
}

/// Decode a record read from flash.
///
/// Every failure mode — blank, foreign, torn, corrupt, or from a future
/// format — resolves to a variant that callers treat as "boot normally".
/// There is exactly one variant that changes behavior.
pub(crate) fn decode_record(bytes: &[u8]) -> DecodeOutcome {
    if bytes.len() < RECORD_LEN {
        return DecodeOutcome::Invalid;
    }
    let record = &bytes[..RECORD_LEN];

    if record[MAGIC_RANGE] != SECTOR_MAGIC {
        // Distinguish "never written" from "written with something else" so
        // the firmware log can say which. Both boot normally.
        return if record.iter().all(|&b| b == ERASED_BYTE) {
            DecodeOutcome::Blank
        } else {
            DecodeOutcome::Invalid
        };
    }

    let stored_crc = u32::from_le_bytes(
        record[CRC_RANGE]
            .try_into()
            .expect("CRC_RANGE is exactly 4 bytes"),
    );
    if crc32(&record[CRC_COVERED]) != stored_crc {
        return DecodeOutcome::CrcMismatch;
    }

    let version = u16::from_le_bytes(
        record[VERSION_RANGE]
            .try_into()
            .expect("VERSION_RANGE is exactly 2 bytes"),
    );
    if version != SECTOR_VERSION {
        return DecodeOutcome::UnsupportedVersion { found: version };
    }

    let flags = BootFlags::from_bits(u32::from_le_bytes(
        record[FLAGS_RANGE]
            .try_into()
            .expect("FLAGS_RANGE is exactly 4 bytes"),
    ));
    DecodeOutcome::Valid(BootControl::new(flags))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_record() -> [u8; RECORD_LEN] {
        encode_record(BootFlags::SKIP_PROJECT_AUTOLOAD)
    }

    #[test]
    fn round_trips_a_valid_record() {
        let record = valid_record();
        match decode_record(&record) {
            DecodeOutcome::Valid(control) => {
                assert!(control.flags().contains(BootFlags::SKIP_PROJECT_AUTOLOAD));
                assert!(control.skip_project_autoload());
            }
            other => panic!("expected Valid, got {other:?}"),
        }
    }

    #[test]
    fn erased_sector_is_blank() {
        let erased = [ERASED_BYTE; RECORD_LEN];
        assert_eq!(decode_record(&erased), DecodeOutcome::Blank);
        assert!(!decode_record(&erased).skip_project_autoload());
    }

    #[test]
    fn an_erased_full_sector_is_blank() {
        // The firmware hands us a whole 4 KB read, not just the record.
        let sector = [ERASED_BYTE; BOOTCTL_PARTITION_SIZE as usize];
        assert_eq!(decode_record(&sector), DecodeOutcome::Blank);
    }

    #[test]
    fn foreign_bytes_are_invalid_not_blank() {
        let foreign = [0x00u8; RECORD_LEN];
        assert_eq!(decode_record(&foreign), DecodeOutcome::Invalid);
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut record = valid_record();
        record[0] = b'X';
        assert_eq!(decode_record(&record), DecodeOutcome::Invalid);
    }

    #[test]
    fn corrupt_flags_fail_the_crc() {
        let mut record = valid_record();
        record[FLAGS_RANGE.start] ^= 0xFF;
        assert_eq!(decode_record(&record), DecodeOutcome::CrcMismatch);
    }

    #[test]
    fn corrupt_crc_is_rejected() {
        let mut record = valid_record();
        record[CRC_RANGE.start] ^= 0xFF;
        assert_eq!(decode_record(&record), DecodeOutcome::CrcMismatch);
    }

    #[test]
    fn a_future_version_is_not_honored() {
        let mut record = valid_record();
        record[VERSION_RANGE].copy_from_slice(&(SECTOR_VERSION + 1).to_le_bytes());
        // Re-CRC so the version, not the checksum, is what rejects it.
        let crc = crc32(&record[CRC_COVERED]);
        record[CRC_RANGE].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(
            decode_record(&record),
            DecodeOutcome::UnsupportedVersion {
                found: SECTOR_VERSION + 1
            }
        );
        assert!(!decode_record(&record).skip_project_autoload());
    }

    #[test]
    fn a_short_read_is_invalid() {
        let record = valid_record();
        assert_eq!(
            decode_record(&record[..RECORD_LEN - 1]),
            DecodeOutcome::Invalid
        );
        assert_eq!(decode_record(&[]), DecodeOutcome::Invalid);
    }

    #[test]
    fn a_torn_write_that_lost_the_magic_is_blank() {
        // Payload landed, magic did not: exactly what the write order
        // guarantees an interrupted write looks like.
        let mut record = valid_record();
        record[MAGIC_RANGE].copy_from_slice(&[ERASED_BYTE; 4]);
        assert!(!decode_record(&record).skip_project_autoload());
    }

    #[test]
    fn a_torn_write_that_lost_the_payload_fails_the_crc() {
        // The impossible-by-ordering case, checked anyway: magic present
        // over an erased payload.
        let mut record = valid_record();
        for byte in &mut record[MAGIC_RANGE.end..] {
            *byte = ERASED_BYTE;
        }
        assert_eq!(decode_record(&record), DecodeOutcome::CrcMismatch);
        assert!(!decode_record(&record).skip_project_autoload());
    }

    /// Every prefix of a record — what a write interrupted partway through
    /// leaves behind — must decode as "no record". This is the property that
    /// replaces write-ordering: the sector cannot be published in two steps,
    /// because every flash API that writes it also erases it first.
    #[test]
    fn no_partial_write_is_ever_honored() {
        let record = valid_record();
        for written in 0..RECORD_LEN {
            let mut partial = [ERASED_BYTE; RECORD_LEN];
            partial[..written].copy_from_slice(&record[..written]);
            assert!(
                !decode_record(&partial).skip_project_autoload(),
                "a write interrupted after {written} bytes must not be honored"
            );
        }
        // ...and the complete record still is.
        assert!(decode_record(&record).skip_project_autoload());
    }

    #[test]
    fn unknown_flag_bits_still_decode_and_keep_known_instructions() {
        let flags = BootFlags::from_bits(BootFlags::SKIP_PROJECT_AUTOLOAD.bits() | (0x5 << 24));
        let record = encode_record(flags);
        match decode_record(&record) {
            DecodeOutcome::Valid(control) => {
                assert!(control.skip_project_autoload());
                assert!(control.flags().has_unknown_bits());
            }
            other => panic!("expected Valid, got {other:?}"),
        }
    }

    #[test]
    fn empty_flags_decode_as_valid_but_instruct_nothing() {
        let record = encode_record(BootFlags::NONE);
        assert!(matches!(decode_record(&record), DecodeOutcome::Valid(_)));
        assert!(!decode_record(&record).skip_project_autoload());
    }

    #[test]
    fn the_record_fits_the_partition() {
        assert!(RECORD_LEN as u32 <= BOOTCTL_PARTITION_SIZE);
    }

    /// Golden vector: the exact 16 bytes of a `SKIP_PROJECT_AUTOLOAD` record.
    ///
    /// This crate is not the only thing that will ever produce these bytes —
    /// host writers over esptool encode the same record, and this is the
    /// fixture they can be checked against. Changing it means changing the
    /// on-flash format, which needs a `SECTOR_VERSION` bump, not a new
    /// expected value here.
    ///
    /// Verified against a device: written to `bootctl` with esptool and
    /// honored by `fw-esp32c6` on real silicon (2026-07-30).
    #[test]
    fn skip_autoload_matches_its_golden_bytes() {
        let record = encode_record(BootFlags::SKIP_PROJECT_AUTOLOAD);
        assert_eq!(
            record,
            [
                0x4c, 0x50, 0x42, 0x43, // magic "LPBC"
                0x01, 0x00, // version 1
                0x00, 0x00, // pad
                0x01, 0x00, 0x00, 0x00, // flags = SKIP_PROJECT_AUTOLOAD
                0x9e, 0x6e, 0x44, 0x3f, // CRC-32 of the preceding 12 bytes
            ],
            "on-flash format changed; bump SECTOR_VERSION rather than this vector"
        );
    }
}
