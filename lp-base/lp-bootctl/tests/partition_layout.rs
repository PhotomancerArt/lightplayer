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

/// Per-board expectations. The boards' tables legitimately DIVERGE above
/// `0x10000` — the S3 has an 8 MB partition floor
/// (`2026-07-30-esp32s3-partition-floor.md`) — so the invariant lp-bootctl
/// actually needs is narrower than whole-table identity: the LOW region
/// (nvs, bootctl, phy_init, factory start) is identical everywhere, which is
/// what lets `BOOTCTL_PARTITION_OFFSET` be a constant instead of a
/// partition-table lookup.
const COMMON_LOW_REGION: &[(&str, u32)] =
    &[("nvs", 0x9000), ("phy_init", 0xf000), ("factory", 0x10000)];

/// `lpfs` must not move on either board — an existing device's filesystem
/// image stays valid only if its partition stays put. The expected offset is
/// per-board because the S3's 8 MB floor placed it differently.
const LPFS_OFFSETS: &[(&str, u32, u32)] = &[
    ("esp32c6", 0x310000, 0x40_0000),
    ("esp32s3", 0x610000, 0x80_0000),
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
fn the_low_region_is_identical_on_every_board() {
    for (board, csv) in [("esp32c6", C6_PARTITIONS), ("esp32s3", S3_PARTITIONS)] {
        for &(name, expected) in COMMON_LOW_REGION {
            let found = partition(csv, name)
                .unwrap_or_else(|| panic!("{board} partitions.csv still declares {name}"));
            assert_eq!(
                found.offset, expected,
                "{board}: {name} moved — the shared low region is what lets \
                 lp-bootctl hardcode its offset"
            );
        }
    }
}

#[test]
fn lpfs_stays_put_on_each_board() {
    for &(board, expected_offset, _) in LPFS_OFFSETS {
        let csv = csv_for(board);
        let lpfs = partition(csv, "lpfs")
            .unwrap_or_else(|| panic!("{board} partitions.csv still declares lpfs"));
        assert_eq!(
            lpfs.offset, expected_offset,
            "{board}: lpfs moved — existing devices' filesystem images would \
             be invalidated"
        );
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
fn no_partitions_overlap_and_each_board_fits_its_flash() {
    for &(board, _, flash_size) in LPFS_OFFSETS {
        let csv = csv_for(board);
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
            last.offset + last.size <= flash_size,
            "{board}: layout exceeds its {flash_size:#x} flash floor"
        );
    }
}

fn csv_for(board: &str) -> &'static str {
    match board {
        "esp32c6" => C6_PARTITIONS,
        "esp32s3" => S3_PARTITIONS,
        other => panic!("unknown board {other}"),
    }
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
