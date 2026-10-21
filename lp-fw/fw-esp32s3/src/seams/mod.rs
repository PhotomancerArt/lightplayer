//! Emulator seams on the S3 (S3 radio M5; ADR
//! `docs/adr/2026-10-05-emulator-seams.md`, the Xtensa amendment).
//!
//! **Only the `test_seam_abi` harness carries a table today.** The shipped
//! image has no network stack until M4 brings Wi‑Fi, so it declares no
//! network seam yet (a table naming calls the image never makes would let an
//! emulator report `net=lan` engaged on a board with no network), and no LED
//! seam (performance seams on the S3 are out of M5). The harness proves the
//! Xtensa seam ABI under the real `release-esp32s3` build: the two test seams
//! and two of the network seam's calls, the first with an out-pointer.

pub mod test_echo;
pub mod test_take;

fw_esp32_common::seam_table! {
    version: env!("LP_APP_VERSION"),
    pending: fw_esp32_common::seams::seam_wake::PENDING,
    entries: [
        fw_esp32_common::seams::net::net_mac::ENTRY,
        fw_esp32_common::seams::net::net_link::ENTRY,
        test_echo::ENTRY,
        test_take::ENTRY,
    ],
}
