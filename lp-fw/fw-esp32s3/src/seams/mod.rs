//! Emulator seams on the S3 (S3 radio M5; ADR
//! `docs/adr/2026-10-05-emulator-seams.md`, the Xtensa amendment).
//!
//! Two tables, one per image kind, as on the C6:
//!
//! - **The `wifi` image** declares the network seam, all nine of its calls (a
//!   seam is never half engaged), so an emulated S3 runs `net=lan` under the
//!   same IP stack, link and server it ships (`fw_esp32_radio::net::net_bringup`
//!   chooses the seam's frame device and station; [`seam_wake_handler`] is the
//!   chip's wake). An image without `wifi` has no network stack and declares
//!   no seam: a table naming calls the image never makes would let an emulator
//!   report `net=lan` engaged on a board with no network. There is no LED seam
//!   (performance seams on the S3 are out of M5).
//! - **The `test_seam_abi` harness** proves the Xtensa seam ABI under the real
//!   `release-esp32s3` build: the two test seams, and the network seam whole,
//!   of which the harness makes two calls (`net_mac`, with an out-pointer, and
//!   `net_link`).

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
