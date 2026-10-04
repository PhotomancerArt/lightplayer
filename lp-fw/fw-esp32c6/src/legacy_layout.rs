//! The C6's `lpfs` as every board flashed before the 2026-10 repartition
//! holds it.
//!
//! A **historical fact, not a mirror of `partitions.csv`**: it never changes,
//! and nothing but the legacy guard reads it. The pre-repartition table is
//! frozen beside it at `lp-app/lpa-link/testdata/partitions-esp32c6-legacy-v1.csv`
//! (plan `lp2025/2026-10-01-1843-c6-repartition`; the repartition ADR).
//!
//! # The guard
//!
//! The repartitioned `lpfs` starts at `0x350000` — inside the old one, which
//! ran `0x310000..0x400000`. A board flashed with the new table by a path that
//! skipped the migration (a hand-typed `espflash`, an interrupted Studio run)
//! finds no filesystem at `0x350000`. Formatting there would erase the old
//! filesystem's blocks 64–239 and destroy the board's files. So before
//! formatting, the C6 probes the old location **read-only**; if a LightPlayer
//! filesystem mounts there, it does not format, boots on a RAM filesystem and
//! says `fs: legacy_held` in its hello. Studio's Update firmware (or
//! `lp-cli hardware lpfs migrate`) then carries the files across.

/// Where the pre-repartition `lpfs` starts.
pub const LEGACY_LPFS_V1_OFFSET: u32 = 0x0031_0000;
/// How many 4 KB littlefs blocks it holds (`0xF0000` bytes).
pub const LEGACY_LPFS_V1_BLOCKS: u32 = 240;
