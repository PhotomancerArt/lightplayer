//! ESP-IDF binary partition tables: parse what a device holds at `0x8000`,
//! and write the same bytes espflash does.
//!
//! A raw read of a board's filesystem needs to know where the filesystem is,
//! and since the 2026-10 C6 repartition that is **per board, not per chip**:
//! a C6 flashed before it keeps `lpfs` at `0x310000`, one flashed after at
//! `0x350000`. So the answer comes from the device's own table, read in the
//! same bootloader session (plan `lp2025/2026-10-01-1843-c6-repartition`,
//! Q6/Q8) — and a table that is not a LightPlayer layout is refused for raw
//! reads, as an unknown chip used to be.
//!
//! Tiny and custom on purpose (Q6): 32-byte rows, magic `0xAA 0x50`, an MD5
//! row (`0xEB 0xEB`) over every row before it, `0xff` to the end. espflash's
//! own encoder (`esp-idf-part`) is the oracle in `lp-cli`'s tests, never a
//! dependency here — this module builds for wasm.

use md5::Digest;

/// Where a flasher writes the partition table on every ESP32 LightPlayer
/// ships for.
pub const PARTITION_TABLE_OFFSET: u32 = 0x8000;

/// A compiled table's length: rows, the MD5 row, `0xff` to here (ESP-IDF's
/// `PARTITION_TABLE_MAX_LEN`).
pub const PARTITION_TABLE_LEN: usize = 0xC00;

const ROW_LEN: usize = 32;
const ENTRY_MAGIC: [u8; 2] = [0xAA, 0x50];
const MD5_MAGIC: [u8; 2] = [0xEB, 0xEB];

/// One row of a partition table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionEntry {
    pub label: String,
    /// `0` app, `1` data.
    pub kind: u8,
    pub subtype: u8,
    pub offset: u32,
    pub size: u32,
    pub flags: u32,
}

/// A parsed partition table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionTable {
    entries: Vec<PartitionEntry>,
}

/// Why a table did not parse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PartitionTableError {
    /// Erased flash at the table's offset — a blank chip, which is a normal
    /// state rather than a damaged table.
    Blank,
    /// Fewer bytes than a row, or rows running off the end with no
    /// terminator.
    Truncated,
    /// The first row is not a partition entry.
    NotATable,
    /// The MD5 row does not match the rows before it.
    Md5Mismatch,
    /// A label that is not UTF-8.
    BadLabel,
}

impl core::fmt::Display for PartitionTableError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Blank => "no partition table (erased flash)",
            Self::Truncated => "partition table is truncated",
            Self::NotATable => "not a partition table",
            Self::Md5Mismatch => "partition table failed its MD5 check",
            Self::BadLabel => "partition table has a label that is not text",
        })
    }
}

impl PartitionTable {
    /// Build a table from rows (fixtures, tests, and the frozen legacy
    /// layout).
    pub fn new(entries: Vec<PartitionEntry>) -> Self {
        Self { entries }
    }

    /// Parse the bytes a device holds at [`PARTITION_TABLE_OFFSET`].
    ///
    /// Reads rows until the MD5 row (verified) or an erased row. A table
    /// with no MD5 row is accepted (ESP-IDF can be told to omit it).
    pub fn parse(bytes: &[u8]) -> Result<Self, PartitionTableError> {
        if bytes.len() < ROW_LEN {
            return Err(PartitionTableError::Truncated);
        }
        if bytes[..2] == [0xFF, 0xFF] {
            return Err(PartitionTableError::Blank);
        }
        if bytes[..2] != ENTRY_MAGIC {
            return Err(PartitionTableError::NotATable);
        }
        let mut entries = Vec::new();
        let mut at = 0usize;
        loop {
            let Some(row) = bytes.get(at..at + ROW_LEN) else {
                return Err(PartitionTableError::Truncated);
            };
            match [row[0], row[1]] {
                ENTRY_MAGIC => {
                    entries.push(parse_row(row)?);
                    at += ROW_LEN;
                }
                MD5_MAGIC => {
                    let digest = md5::Md5::digest(&bytes[..at]);
                    if row[16..32] != digest[..] {
                        return Err(PartitionTableError::Md5Mismatch);
                    }
                    break;
                }
                _ => break,
            }
        }
        Ok(Self { entries })
    }

    /// Every row, in table order.
    pub fn entries(&self) -> &[PartitionEntry] {
        &self.entries
    }

    /// The row labelled `label`.
    pub fn find(&self, label: &str) -> Option<&PartitionEntry> {
        self.entries.iter().find(|e| e.label == label)
    }

    /// Do two tables describe the same layout — the same rows, by label,
    /// type, subtype, offset and size, in the same order? Flags are not
    /// layout.
    pub fn same_layout(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().zip(&other.entries).all(|(a, b)| {
                a.label == b.label
                    && a.kind == b.kind
                    && a.subtype == b.subtype
                    && a.offset == b.offset
                    && a.size == b.size
            })
    }

    /// The table as espflash writes it: rows, the MD5 row, `0xff` to
    /// [`PARTITION_TABLE_LEN`].
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(PARTITION_TABLE_LEN);
        for entry in &self.entries {
            out.extend_from_slice(&ENTRY_MAGIC);
            out.push(entry.kind);
            out.push(entry.subtype);
            out.extend_from_slice(&entry.offset.to_le_bytes());
            out.extend_from_slice(&entry.size.to_le_bytes());
            let mut label = [0u8; 16];
            let n = entry.label.len().min(16);
            label[..n].copy_from_slice(&entry.label.as_bytes()[..n]);
            out.extend_from_slice(&label);
            out.extend_from_slice(&entry.flags.to_le_bytes());
        }
        let digest = md5::Md5::digest(&out);
        out.extend_from_slice(&MD5_MAGIC);
        out.extend_from_slice(&[0xFF; 14]);
        out.extend_from_slice(&digest);
        out.resize(PARTITION_TABLE_LEN, 0xFF);
        out
    }

    /// Parse a `partitions.csv` the way ESP-IDF does, for the subset
    /// LightPlayer's tables use: `name, type, subtype, offset, size[, flags]`
    /// with hex or decimal numbers (`K`/`M` suffixes accepted), `#` comments,
    /// and every offset given explicitly. Used by `lp-cli`'s layout
    /// preflight and by fixtures; the production table always comes from a
    /// firmware image's own bytes.
    pub fn from_csv(text: &str) -> Result<Self, String> {
        let mut entries = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            if fields.len() < 5 {
                return Err(format!("line {}: expected 5 fields", number + 1));
            }
            let kind = match fields[1] {
                "app" => 0x00,
                "data" => 0x01,
                other => parse_number(other)
                    .map(|v| v as u8)
                    .ok_or_else(|| format!("line {}: type {other:?}", number + 1))?,
            };
            let subtype = parse_subtype(kind, fields[2])
                .ok_or_else(|| format!("line {}: subtype {:?}", number + 1, fields[2]))?;
            let offset = parse_number(fields[3])
                .ok_or_else(|| format!("line {}: offset {:?}", number + 1, fields[3]))?;
            let size = parse_number(fields[4])
                .ok_or_else(|| format!("line {}: size {:?}", number + 1, fields[4]))?;
            let flags = match fields.get(5) {
                Some(&"encrypted") => 1,
                _ => 0,
            };
            entries.push(PartitionEntry {
                label: String::from(fields[0]),
                kind,
                subtype,
                offset,
                size,
                flags,
            });
        }
        Ok(Self { entries })
    }
}

fn parse_row(row: &[u8]) -> Result<PartitionEntry, PartitionTableError> {
    let le = |at: usize| u32::from_le_bytes([row[at], row[at + 1], row[at + 2], row[at + 3]]);
    let label_bytes = &row[12..28];
    let end = label_bytes.iter().position(|b| *b == 0).unwrap_or(16);
    let label =
        core::str::from_utf8(&label_bytes[..end]).map_err(|_| PartitionTableError::BadLabel)?;
    Ok(PartitionEntry {
        label: String::from(label),
        kind: row[2],
        subtype: row[3],
        offset: le(4),
        size: le(8),
        flags: le(28),
    })
}

fn parse_number(text: &str) -> Option<u32> {
    let text = text.trim();
    let (digits, multiplier) = if let Some(d) = text.strip_suffix(['K', 'k']) {
        (d, 1024)
    } else if let Some(d) = text.strip_suffix(['M', 'm']) {
        (d, 1024 * 1024)
    } else {
        (text, 1)
    };
    let value = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        u32::from_str_radix(hex, 16).ok()?
    } else {
        digits.parse::<u32>().ok()?
    };
    value.checked_mul(multiplier)
}

/// ESP-IDF's subtype names for the types LightPlayer tables use.
fn parse_subtype(kind: u8, text: &str) -> Option<u8> {
    let named = match (kind, text) {
        (0x00, "factory") => Some(0x00),
        (0x00, "test") => Some(0x20),
        (0x01, "ota") => Some(0x00),
        (0x01, "phy") => Some(0x01),
        (0x01, "nvs") => Some(0x02),
        (0x01, "coredump") => Some(0x03),
        (0x01, "nvs_keys") => Some(0x04),
        (0x01, "efuse") => Some(0x05),
        (0x01, "undefined") => Some(0x06),
        (0x01, "esphttpd") => Some(0x80),
        (0x01, "fat") => Some(0x81),
        (0x01, "spiffs") => Some(0x82),
        (0x01, "littlefs") => Some(0x83),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if kind == 0x00
        && let Some(n) = text.strip_prefix("ota_")
    {
        return n.parse::<u8>().ok().map(|n| 0x10 + n);
    }
    parse_number(text).map(|v| v as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    const C6_CSV: &str = include_str!("../../../../lp-fw/fw-esp32c6/partitions.csv");
    const S3_CSV: &str = include_str!("../../../../lp-fw/fw-esp32s3/partitions.csv");

    #[test]
    fn the_writer_and_the_parser_round_trip_the_shipped_tables() {
        for csv in [C6_CSV, S3_CSV] {
            let table = PartitionTable::from_csv(csv).unwrap();
            let bytes = table.to_bytes();
            assert_eq!(bytes.len(), PARTITION_TABLE_LEN);
            let back = PartitionTable::parse(&bytes).unwrap();
            assert_eq!(back, table);
            assert!(back.same_layout(&table));
            assert!(table.find("lpfs").is_some());
        }
    }

    #[test]
    fn erased_flash_is_blank_not_damaged() {
        assert_eq!(
            PartitionTable::parse(&[0xFF; PARTITION_TABLE_LEN]),
            Err(PartitionTableError::Blank)
        );
    }

    #[test]
    fn a_corrupted_row_fails_its_md5() {
        let mut bytes = PartitionTable::from_csv(C6_CSV).unwrap().to_bytes();
        bytes[4] ^= 0x01; // the first row's offset
        assert_eq!(
            PartitionTable::parse(&bytes),
            Err(PartitionTableError::Md5Mismatch)
        );
    }

    #[test]
    fn a_short_read_is_truncated_and_garbage_is_not_a_table() {
        let bytes = PartitionTable::from_csv(C6_CSV).unwrap().to_bytes();
        assert_eq!(
            PartitionTable::parse(&bytes[..40]),
            Err(PartitionTableError::Truncated)
        );
        assert_eq!(
            PartitionTable::parse(&[0x12; 64]),
            Err(PartitionTableError::NotATable)
        );
    }

    #[test]
    fn layouts_compare_by_rows_not_flags() {
        let a = PartitionTable::from_csv(C6_CSV).unwrap();
        let mut entries = a.entries().to_vec();
        entries[0].flags = 1;
        assert!(a.same_layout(&PartitionTable::new(entries.clone())));
        entries[3].size += 0x1000;
        assert!(!a.same_layout(&PartitionTable::new(entries)));
    }
}
