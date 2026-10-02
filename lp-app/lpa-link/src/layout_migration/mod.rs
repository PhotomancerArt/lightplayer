//! Moving a board's files across a partition-layout change — decided here,
//! executed by the providers.
//!
//! The 2026-10 C6 repartition (plan `lp2025/2026-10-01-1843-c6-repartition`)
//! grows the app partition and moves `lpfs` from `0x310000` (960 KB) to
//! `0x350000` (704 KB). The new region is the old filesystem's blocks
//! 64–239, so a board's files cannot stay where they are: they are read raw
//! in the bootloader session, mounted here, **re-packed** into a fresh image
//! at the new geometry, verified, and written back in an order that never
//! leaves a half-written filesystem that mounts (MQ1, MQ9).
//!
//! Everything in this module is **pure** (sans-IO): bytes in, a plan or a
//! refusal out. The host provider (espflash), the browser provider
//! (esptool-js) and the fake execute the same [`FlashStep`]s.
//!
//! | file | concept |
//! |---|---|
//! | [`lpfs_geometry`] | where a filesystem is and the firmware's littlefs config |
//! | [`legacy_layout`] | the frozen pre-repartition C6 table |
//! | [`layout_state`] | what a board holds, and what to read to find out |
//! | [`lpfs_tree`] | every file and directory of a mounted image |
//! | [`lpfs_repack`] | the tree re-packed into a fresh image, verified |
//! | [`migration_plan`] | the ordered flash steps, or a refusal |
//! | [`flash_map`] | the steps applied to a flash image in memory (fake, tests) |
//! | [`device_backup_archive`] | the v2 backup ZIP every migration stores first |

pub mod device_backup_archive;
pub mod flash_map;
pub mod layout_state;
pub mod legacy_layout;
pub mod lpfs_geometry;
pub mod lpfs_repack;
pub mod lpfs_tree;
pub mod migration_plan;

pub use flash_map::apply_steps;
pub use layout_state::{LayoutInspection, LayoutProbe, LayoutState, inspect_flash};
pub use legacy_layout::{LEGACY_C6_V1_LPFS, is_legacy_c6_v1, legacy_c6_v1_table};
pub use lpfs_geometry::LpfsGeometry;
pub use lpfs_repack::{RepackedImage, repack};
pub use lpfs_tree::{LpfsNode, LpfsTree, LpfsTreeError};
pub use migration_plan::{
    FlashPlan, FlashStep, LayoutDecision, MigrationSummary, Refusal, decide, plan_lpfs_restore,
    plan_migration, plan_plain_flash,
};
