//! The direct load: the application's `PT_LOAD`s, placed by vaddr.
//!
//! # What a direct load is, and what it is not
//!
//! It is the shortest path from a `fw-esp32s3` ELF to a running hart: place
//! every loadable segment where the linker said it lives, seed core 0's
//! entry, `PS` and a boot frame ([`crate::machine::BootFrame`]), and run. It
//! is how a bring-up iterates in seconds instead of in a whole boot chain,
//! and it is what M6 P03 through P05 use.
//!
//! It is **not** a boot. What a real ESP32-S3 power-on does that this does
//! not, each owned by a later phase:
//!
//! 1. Run the mask ROM's reset path — the eFuse patches, `mmu_init`, the
//!    console banner. **P06.**
//! 2. Run the ESP-IDF second-stage bootloader: parse the image header, verify
//!    the app's MD5, program the flash MMU for its segments and jump. **P06.**
//! 3. Leave the flash cache **on**, with the IROM and DROM windows served
//!    through the MMU from the chip rather than pre-filled. Here the windows
//!    are plain RAM holding the bytes the ELF put there. **P06.**
//! 4. Leave the flash chip holding the image, so `esp_storage`'s reads find
//!    something. **P06.**
//! 5. Tell the ROM how big the flash chip is, so `esp_rom_spiflash_*` works.
//!    **P06.**
//! 6. Leave a real reset cause in `RTC_CNTL.reset_state`. **P04.**
//! 7. Leave real eFuse contents — the MAC, the chip revision. **P04.**
//! 8. Leave the RTC watchdog armed, which on this chip the shipped image then
//!    feeds on every boot (`m6/notes.md` §5.2). **P04.**
//! 9. Leave `CPENABLE` at whatever the boot chain leaves it at. **P09** — see
//!    [`crate::machine::Esp32S3Builder::cpenable_reset`], which is why that is
//!    a parameter and not a constant.
//! 10. Leave the strapping pins latched. **P06.**
//! 11. Start on a stack the bootloader chose, at a window depth it chose.
//!     [`crate::machine::BootFrame`] is the seam; **P06** measures it.
//!
//! Every one of those is a *difference a run can show*, which is why the list
//! is here rather than in a commit message: a P03 run that disagrees with
//! silicon disagrees for a reason on this list, or for a reason that is a
//! finding.
//!
//! # Placed by `p_vaddr`, and the S3 makes that matter
//!
//! The shipped image's `.rtc_fast.persistent` links to RTC fast memory at
//! `0x600F_E000` with a load address in the DROM window (`m6/notes.md` §2.6
//! lists `0x600f_e000 RW` as its own `PT_LOAD`). A loader that placed by
//! `p_paddr` would put `lp_recovery`'s ledger in flash and the firmware would
//! read zeros from the address it actually uses. So placement is by vaddr —
//! the address the running code names — and a segment whose two differ is
//! **logged**, not silently resolved.

use lp_emu_esp_common::{ElfImage, SocBus};

use crate::rom::{self, RomError, place_spanning};

/// One application segment, placed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedAppSegment {
    pub vaddr: u32,
    pub paddr: u32,
    pub filesz: u32,
    pub memsz: u32,
    pub execute: bool,
    /// The region names the bytes landed in, in address order.
    ///
    /// ⚠️ An executable segment at `0x4037_8000` reports **`sram1-dbus`**,
    /// not an I-bus name, and that is correct: the I-bus view is a RAM alias
    /// and `SocBus` translates to the canonical address before anything looks
    /// a region up (DD81). The region a byte lives in is the D-bus one
    /// whichever door it arrived through.
    pub regions: Vec<&'static str>,
}

impl PlacedAppSegment {
    /// `p_vaddr != p_paddr`: placed by vaddr, and recorded rather than picked
    /// silently. See the module docs for the S3 segment where this bites.
    pub fn relocated(&self) -> bool {
        self.vaddr != self.paddr
    }
}

/// Something wrong with the app image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    Rom(RomError),
    /// The entry point is not inside any `PT_LOAD`. (`entry != 0` is not a
    /// usable check in this repository: the guest images under `lp-emu/` link
    /// at zero on purpose.)
    EntryNotLoadable {
        entry: u32,
    },
    /// No loadable segment at all.
    NothingToLoad,
}

impl core::fmt::Display for LoadError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LoadError::Rom(e) => write!(f, "{e}"),
            LoadError::EntryNotLoadable { entry } => write!(
                f,
                "the ELF's entry point {entry:#010x} is not inside any PT_LOAD segment — this \
                 is not a bootable image for this machine"
            ),
            LoadError::NothingToLoad => write!(f, "the ELF has no loadable segment"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<RomError> for LoadError {
    fn from(e: RomError) -> Self {
        LoadError::Rom(e)
    }
}

/// Place the app's segments. The ROM must already be loaded — a direct load
/// overwrites what the app owns exactly as a real bootloader would, and the
/// ROM's own `.data_*` sections go down first.
pub fn load_app(bus: &mut SocBus, app: &ElfImage) -> Result<Vec<PlacedAppSegment>, LoadError> {
    // The same two skips the ROM loader makes: an empty segment, and a
    // segment whose file bytes are the ELF's own headers
    // (`rom::is_header_map` — the S3 mask ROM has one, and an application
    // linked the same way would too).
    let loadable: Vec<_> = app
        .segments
        .iter()
        .filter(|s| s.memsz > 0 && !rom::is_header_map(&s.data))
        .collect();
    if loadable.is_empty() {
        return Err(LoadError::NothingToLoad);
    }

    let entry_is_loadable = loadable.iter().any(|s| {
        app.entry >= s.vaddr && u64::from(app.entry) < u64::from(s.vaddr) + u64::from(s.memsz)
    });
    if !entry_is_loadable {
        return Err(LoadError::EntryNotLoadable { entry: app.entry });
    }

    let mut placed = Vec::new();
    for seg in loadable {
        if seg.paddr != seg.vaddr {
            log::info!(
                "loader: segment at vaddr {:#010x} has paddr {:#010x}; direct load places at \
                 vaddr (the address the running code uses)",
                seg.vaddr,
                seg.paddr
            );
        }
        let regions = place_spanning(bus, seg.vaddr, &seg.data, seg.memsz)?;
        placed.push(PlacedAppSegment {
            vaddr: seg.vaddr,
            paddr: seg.paddr,
            filesz: seg.filesz(),
            memsz: seg.memsz,
            execute: seg.execute,
            regions,
        });
    }
    Ok(placed)
}
