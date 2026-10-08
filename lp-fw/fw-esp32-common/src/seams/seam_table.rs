//! [`seam_table!`](crate::seam_table): the chip crate's one seam table.
//!
//! The table is a `#[used] #[no_mangle]` static named `LP_SEAM_TABLE`, in
//! flash `.rodata`. Nothing in the firmware reads it; the emulator scans the
//! flash image for its magic and needs no ELF (the Studio tab has none).
//! `#[used]` and the exported name keep LTO and `--gc-sections` from
//! dropping it, and in a split build it is a **core root**
//! (`tools/lp-fw-split`), so it and the seam functions it names land in the
//! core whatever the engine does.
//!
//! It is a macro and not a static here because two of its facts belong to
//! the chip crate: the app version (`LP_APP_VERSION` comes from the chip
//! crate's `build.rs`) and which entries the image carries (a harness adds
//! the test seams). A chip crate invokes it exactly once:
//!
//! ```text
//! fw_esp32_common::seam_table! {
//!     version: env!("LP_APP_VERSION"),
//!     pending: fw_esp32_common::seams::seam_wake::PENDING,
//!     entries: [fw_esp32_common::seams::ws281x_wait_step::ENTRY],
//! }
//! ```
//!
//! The table names its own address (the emulator's live-table check) and the
//! wake pending word's (`pending`, a static `AtomicU32` in RAM:
//! [`crate::seams::seam_wake::PENDING`]). The wake's handler is the chip
//! crate's, bound only when a capability seam that uses it is engaged; on
//! silicon the word stays zero and the line is never enabled.

/// Instantiate the chip's seam table. See [the module docs](self).
#[macro_export]
macro_rules! seam_table {
    (
        version: $version:expr,
        pending: $pending:path,
        entries: [$($entry:expr),* $(,)?] $(,)?
    ) => {
        /// The emulator seam descriptor table (`lp_seam::table`).
        #[used]
        #[unsafe(no_mangle)]
        pub static LP_SEAM_TABLE: $crate::seams::lp_seam::table::SeamTable<
            { [$(stringify!($entry)),*].len() },
        > = $crate::seams::lp_seam::table::SeamTable::new(
            $version,
            $crate::seams::lp_seam::table::Addr::of(&LP_SEAM_TABLE),
            $crate::seams::lp_seam::table::Addr::of(&$pending),
            [$($entry),*],
        );
    };
}
