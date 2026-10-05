//! The C6's seam descriptor table, in flash `.rodata`.
//!
//! `#[used]` and `#[no_mangle]` so neither LTO nor `--gc-sections` drops it:
//! nothing in the firmware reads it. The emulator scans the flash image for
//! its magic; it needs no ELF (the Studio tab has none).

use lp_seam::table::{Addr, SeamEntry, SeamTable};

#[cfg(not(feature = "spike_l0_wfi_wait"))]
const ENTRIES: usize = 1;
#[cfg(feature = "spike_l0_wfi_wait")]
const ENTRIES: usize = 0;

#[used]
#[unsafe(no_mangle)]
pub static LP_SEAM_TABLE: SeamTable<ENTRIES> = SeamTable::new(
    env!("LP_APP_VERSION"),
    Addr::NONE,
    [
        #[cfg(not(feature = "spike_l0_wfi_wait"))]
        SeamEntry::new(
            &lp_seam::ALL[0],
            Addr(super::ws281x_wait_step::lp_seam_ws281x_wait_step as *const ()),
            Addr::NONE,
        ),
    ],
);

const _: () = assert!(lp_seam::ALL[0].id == lp_seam::ws281x_wait_step::ID);
