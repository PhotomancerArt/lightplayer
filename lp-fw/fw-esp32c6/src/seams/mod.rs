//! Emulator seams on the C6: the image's one seam table
//! (`fw_esp32_common::seam_table!`; ADR `docs/adr/2026-10-05-emulator-seams.md`).
//!
//! The shipped image carries the LED wait step
//! (`fw_esp32_common::seams::ws281x_wait_step`) and the network seam's nine
//! calls (`fw_esp32_common::seams::net`), and names the wake pending word
//! (`fw_esp32_common::seams::seam_wake::PENDING`), whose handler is
//! [`seam_wake_handler`]. The `test_seam_abi` harness adds the two test
//! seams, which never ship. On silicon nothing here runs: the table is data
//! nobody reads, and the wake line is never bound.

#[cfg(lp_net)]
pub mod seam_wake_handler;
#[cfg(feature = "test_seam_abi")]
pub mod test_echo;
#[cfg(feature = "test_seam_abi")]
pub mod test_take;

#[cfg(not(feature = "test_seam_abi"))]
fw_esp32_common::seam_table! {
    version: env!("LP_APP_VERSION"),
    pending: fw_esp32_common::seams::seam_wake::PENDING,
    entries: [
        fw_esp32_common::seams::ws281x_wait_step::ENTRY,
        fw_esp32_common::seams::net::net_mac::ENTRY,
        fw_esp32_common::seams::net::net_take_frame::ENTRY,
        fw_esp32_common::seams::net::net_give_frame::ENTRY,
        fw_esp32_common::seams::net::net_link::ENTRY,
        fw_esp32_common::seams::net::net_scan_start::ENTRY,
        fw_esp32_common::seams::net::net_scan_take::ENTRY,
        fw_esp32_common::seams::net::net_connect::ENTRY,
        fw_esp32_common::seams::net::net_disconnect::ENTRY,
        fw_esp32_common::seams::net::net_event_take::ENTRY,
    ],
}

#[cfg(feature = "test_seam_abi")]
fw_esp32_common::seam_table! {
    version: env!("LP_APP_VERSION"),
    pending: fw_esp32_common::seams::seam_wake::PENDING,
    entries: [
        fw_esp32_common::seams::ws281x_wait_step::ENTRY,
        fw_esp32_common::seams::net::net_mac::ENTRY,
        fw_esp32_common::seams::net::net_take_frame::ENTRY,
        fw_esp32_common::seams::net::net_give_frame::ENTRY,
        fw_esp32_common::seams::net::net_link::ENTRY,
        fw_esp32_common::seams::net::net_scan_start::ENTRY,
        fw_esp32_common::seams::net::net_scan_take::ENTRY,
        fw_esp32_common::seams::net::net_connect::ENTRY,
        fw_esp32_common::seams::net::net_disconnect::ENTRY,
        fw_esp32_common::seams::net::net_event_take::ENTRY,
        test_echo::ENTRY,
        test_take::ENTRY,
    ],
}
