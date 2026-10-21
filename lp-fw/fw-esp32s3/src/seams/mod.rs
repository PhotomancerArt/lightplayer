//! Emulator seams on the S3 (S3 radio M5; ADR
//! `docs/adr/2026-10-05-emulator-seams.md`, the Xtensa amendment).
//!
//! **Only the `test_seam_abi` harness carries a table today.** The shipped
//! image has no network stack until M4 brings Wi‑Fi, so it declares no
//! network seam yet (a table naming calls the image never makes would let an
//! emulator report `net=lan` engaged on a board with no network), and no LED
//! seam (performance seams on the S3 are out of M5). The harness proves the
//! Xtensa seam ABI under the real `release-esp32s3` build: the two test seams,
//! and the network seam whole — a seam is never half engaged, so all nine of
//! its calls are declared — of which the harness makes two (`net_mac`, with
//! an out-pointer, and `net_link`).

pub mod seam_wake_handler;
pub mod test_echo;
pub mod test_take;

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
