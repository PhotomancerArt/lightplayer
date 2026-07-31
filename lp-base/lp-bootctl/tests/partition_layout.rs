//! Guards the hand-maintained agreement between this crate's constants and
//! the firmware partition tables.
//!
//! `lp-bootctl` hardcodes the sector's flash offset so a host writer (over
//! esptool, on a device that cannot boot) and the firmware reader agree
//! without either one parsing a partition table. That only stays true if
//! `partitions.csv` and [`lp_bootctl::BOOTCTL_PARTITION_OFFSET`] move
//! together — hence this test.
//!
//! It lives here rather than in `fw-esp32c6` because that crate is RV32-only
//! and excluded from host builds: a `#[cfg(test)]` module there would never
//! run. See `docs/adr/2026-07-30-boot-control-sector.md`.

use lp_bootctl::{BOOTCTL_PARTITION_OFFSET, BOOTCTL_PARTITION_SIZE, RECORD_LEN};

const C6_PARTITIONS: &str = include_str!("../../../lp-fw/fw-esp32c6/partitions.csv");
const S3_PARTITIONS: &str = include_str!("../../../lp-fw/fw-esp32s3/partitions.csv");

/// Offsets that existed before `bootctl` and must not have shifted — an
/// existing device's `lpfs` image stays valid only if `lpfs` stays put.
const PRE_EXISTING_OFFSETS: &[(&str, u32)] = &[
    ("nvs", 0x9000),
    ("phy_init", 0xf000),
    ("factory", 0x10000),
    ("lpfs", 0x310000),
];

#[test]
fn bootctl_offset_and_size_match_every_board() {
    for (board, csv) in [("esp32c6", C6_PARTITIONS), ("esp32s3", S3_PARTITIONS)] {
        let bootctl = partition(csv, "bootctl")
            .unwrap_or_else(|| panic!("{board} partitions.csv declares a bootctl partition"));
        assert_eq!(
            bootctl.offset, BOOTCTL_PARTITION_OFFSET,
            "{board}: bootctl offset must match lp_bootctl::BOOTCTL_PARTITION_OFFSET"
        );
        assert_eq!(
            bootctl.size, BOOTCTL_PARTITION_SIZE,
            "{board}: bootctl size must match lp_bootctl::BOOTCTL_PARTITION_SIZE"
        );
    }
}

#[test]
fn the_record_fits_the_sector() {
    assert!(
        RECORD_LEN as u32 <= BOOTCTL_PARTITION_SIZE,
        "the record must fit the partition the firmware reads it from"
    );
}

#[test]
fn adding_bootctl_did_not_move_any_pre_existing_partition() {
    for (board, csv) in [("esp32c6", C6_PARTITIONS), ("esp32s3", S3_PARTITIONS)] {
        for &(name, expected) in PRE_EXISTING_OFFSETS {
            let found = partition(csv, name)
                .unwrap_or_else(|| panic!("{board} partitions.csv still declares {name}"));
            assert_eq!(
                found.offset, expected,
                "{board}: {name} moved — existing devices' flash layout must not shift"
            );
        }
    }
}

#[test]
fn bootctl_came_out_of_nvs() {
    // The 4 KB was taken from nvs (unused: no LightPlayer code touches NVS,
    // and esp-radio's "NVS" is a RAM array). If someone re-grows nvs, the
    // two partitions overlap and this catches it.
    for (board, csv) in [("esp32c6", C6_PARTITIONS), ("esp32s3", S3_PARTITIONS)] {
        let nvs = partition(csv, "nvs").expect("nvs exists");
        let bootctl = partition(csv, "bootctl").expect("bootctl exists");
        assert_eq!(
            nvs.offset + nvs.size,
            bootctl.offset,
            "{board}: bootctl must start where nvs ends"
        );
    }
}

#[test]
fn no_partitions_overlap_and_the_image_still_fits_4mb() {
    for (board, csv) in [("esp32c6", C6_PARTITIONS), ("esp32s3", S3_PARTITIONS)] {
        let mut parts = partitions(csv);
        parts.sort_by_key(|p| p.offset);
        for pair in parts.windows(2) {
            let (first, second) = (&pair[0], &pair[1]);
            assert!(
                first.offset + first.size <= second.offset,
                "{board}: {} overlaps {}",
                first.name,
                second.name
            );
        }
        let last = parts.last().expect("at least one partition");
        assert!(
            last.offset + last.size <= 0x40_0000,
            "{board}: layout exceeds the 4 MB image"
        );
    }
}

#[test]
fn both_boards_declare_byte_identical_layouts() {
    // lp-bootctl hardcodes one offset for every board; that is only sound
    // while the boards agree.
    assert_eq!(
        partitions(C6_PARTITIONS),
        partitions(S3_PARTITIONS),
        "the C6 and S3 partition layouts must stay identical"
    );
}

#[derive(PartialEq, Eq, Debug)]
struct Partition {
    name: String,
    offset: u32,
    size: u32,
}

fn partitions(csv: &str) -> Vec<Partition> {
    csv.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            assert!(
                fields.len() >= 5,
                "partition row {line:?} should have at least 5 fields"
            );
            Partition {
                name: fields[0].to_string(),
                offset: parse_hex(fields[3]),
                size: parse_hex(fields[4]),
            }
        })
        .collect()
}

fn partition(csv: &str, name: &str) -> Option<Partition> {
    partitions(csv).into_iter().find(|p| p.name == name)
}

fn parse_hex(text: &str) -> u32 {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    u32::from_str_radix(digits, 16)
        .unwrap_or_else(|_| panic!("partition field {text:?} is not hex"))
}
