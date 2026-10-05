//! The C6's seam descriptor table, in flash `.rodata`.
//!
//! `#[used]` and `#[no_mangle]` so neither LTO nor `--gc-sections` drops it:
//! nothing in the firmware reads it. The emulator scans the flash image for
//! its magic; it needs no ELF (the Studio tab has none).

#[cfg(not(feature = "spike_l0_wfi_wait"))]
use lp_seam::table::SeamEntry;
use lp_seam::table::{Addr, SeamTable};

#[cfg(all(not(feature = "spike_l0_wfi_wait"), not(feature = "spike_seam_wake_probe")))]
const ENTRIES: usize = 1;
#[cfg(all(not(feature = "spike_l0_wfi_wait"), feature = "spike_seam_wake_probe"))]
const ENTRIES: usize = 3;
#[cfg(feature = "spike_l0_wfi_wait")]
const ENTRIES: usize = 0;

#[cfg(not(feature = "spike_seam_wake_probe"))]
const PENDING: Addr = Addr::NONE;
#[cfg(feature = "spike_seam_wake_probe")]
const PENDING: Addr = Addr(&super::wake_probe::LP_SEAM_PENDING as *const _ as *const ());

#[used]
#[unsafe(no_mangle)]
pub static LP_SEAM_TABLE: SeamTable<ENTRIES> = SeamTable::new(
    env!("LP_APP_VERSION"),
    PENDING,
    [
        #[cfg(not(feature = "spike_l0_wfi_wait"))]
        SeamEntry::new(
            &lp_seam::ALL[0],
            Addr(super::ws281x_wait_step::lp_seam_ws281x_wait_step as *const ()),
            Addr::NONE,
        ),
        #[cfg(all(not(feature = "spike_l0_wfi_wait"), feature = "spike_seam_wake_probe"))]
        SeamEntry::new(
            &lp_seam::ALL[1],
            Addr(super::wake_probe::lp_seam_engaged as *const ()),
            Addr::NONE,
        ),
        #[cfg(all(not(feature = "spike_l0_wfi_wait"), feature = "spike_seam_wake_probe"))]
        SeamEntry::new(
            &lp_seam::ALL[2],
            Addr(super::wake_probe::lp_seam_probe_take as *const ()),
            Addr(&super::wake_probe::LP_SEAM_PROBE_ENGAGED as *const u8 as *const ()),
        ),
    ],
);

const _: () = assert!(lp_seam::ALL[0].id == lp_seam::ws281x_wait_step::ID);
const _: () = assert!(lp_seam::ALL[1].id == lp_seam::engaged::ID);
const _: () = assert!(lp_seam::ALL[2].id == lp_seam::probe_take::ID);
