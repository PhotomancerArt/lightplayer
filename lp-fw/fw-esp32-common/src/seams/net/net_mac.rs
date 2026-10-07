//! `net_mac`: the station's MAC, and the network seam's engaged byte.
//!
//! The byte is the switch (seams ADR §3): the firmware reads it once at
//! network bring-up. On silicon it reads 0 and the radio runs; an emulator
//! that engaged `net=lan` patches it to 1 and answers this call with the
//! board's MAC. On silicon the call returns 0 and writes nothing, and it is
//! never made there: nothing calls it unless the byte reads 1.

lp_seam::seam_fn! {
    net_mac => fn(out: *mut u8) -> u32 {
        let _ = out;
        0
    }
}

lp_seam::engaged_byte!(net_mac);

/// This seam's entry in the chip's [`crate::seam_table!`].
#[cfg(target_arch = "riscv32")]
pub const ENTRY: lp_seam::table::SeamEntry =
    lp_seam::table::SeamEntry::new(&DECL, ADDRESS, ENGAGED_ADDRESS);
