//! Emulator seams on the C6: the image's one seam table
//! (`fw_esp32_common::seam_table!`; ADR `docs/adr/2026-10-05-emulator-seams.md`).
//!
//! The shipped image carries one seam, the LED wait step
//! (`fw_esp32_common::seams::ws281x_wait_step`). The `test_seam_abi` harness
//! adds the two test seams, which never ship. On silicon nothing here runs:
//! the table is data nobody reads.

#[cfg(feature = "test_seam_abi")]
pub mod test_echo;
#[cfg(feature = "test_seam_abi")]
pub mod test_take;

#[cfg(not(feature = "test_seam_abi"))]
fw_esp32_common::seam_table! {
    version: env!("LP_APP_VERSION"),
    entries: [fw_esp32_common::seams::ws281x_wait_step::ENTRY],
}

#[cfg(feature = "test_seam_abi")]
fw_esp32_common::seam_table! {
    version: env!("LP_APP_VERSION"),
    entries: [
        fw_esp32_common::seams::ws281x_wait_step::ENTRY,
        test_echo::ENTRY,
        test_take::ENTRY,
    ],
}
