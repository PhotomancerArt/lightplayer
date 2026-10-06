//! TEST ONLY (`test_seam_abi` harness; never in a shipped table): a value
//! seam, three arguments in and a result out, so the emulator's test can see
//! that the generated call shim keeps both through the real `release-esp32`
//! LTO + `opt-level = "z"` build. On silicon: `a ^ b ^ c`.

use fw_esp32_common::seams::lp_seam::table::{Addr, SeamEntry};

fw_esp32_common::seams::lp_seam::seam_fn! {
    test_echo => fn(a: u32, b: u32, c: u32) -> u32 { a ^ b ^ c }
}

pub const ENTRY: SeamEntry = SeamEntry::new(&DECL, ADDRESS, Addr::NONE);
