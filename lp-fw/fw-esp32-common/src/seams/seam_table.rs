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
//!     entries: [fw_esp32_common::seams::ws281x_wait_step::ENTRY],
//! }
//! ```
//!
//! The table names its own address (the emulator's live-table check) and no
//! wake pending word: the firmware's wake handler ships with the first
//! capability seam (Bluetooth), not before.

/// Instantiate the chip's seam table. See [the module docs](self).
#[macro_export]
macro_rules! seam_table {
    (version: $version:expr, entries: [$($entry:expr),* $(,)?] $(,)?) => {
        /// The emulator seam descriptor table (`lp_seam::table`).
        #[used]
        #[unsafe(no_mangle)]
        pub static LP_SEAM_TABLE: $crate::seams::lp_seam::table::SeamTable<
            { [$(stringify!($entry)),*].len() },
        > = $crate::seams::lp_seam::table::SeamTable::new(
            $version,
            $crate::seams::lp_seam::table::Addr::of(&LP_SEAM_TABLE),
            // No wake handler ships yet (it lands with the Bluetooth seam).
            $crate::seams::lp_seam::table::Addr::NONE,
            [$($entry),*],
        );
    };
}
