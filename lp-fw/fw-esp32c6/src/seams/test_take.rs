//! TEST ONLY (`test_seam_abi` harness; never in a shipped table): the wake
//! consumer's shape — a switch-shape seam with an engaged byte, whose take
//! copies what the host queued into a buffer the call hands over. On
//! silicon: not engaged, and a take returns 0.

use fw_esp32_common::seams::lp_seam::table::SeamEntry;

fw_esp32_common::seams::lp_seam::seam_fn! {
    test_take => fn(endpoint: u32, buf: *mut u8, cap: u32) -> u32 {
        let _ = (endpoint, buf, cap);
        0
    }
}

fw_esp32_common::seams::lp_seam::engaged_byte!(test_take);

pub const ENTRY: SeamEntry = SeamEntry::new(&DECL, ADDRESS, ENGAGED_ADDRESS);
