//! The split image's boot bookkeeping (`LP_SPLIT_LINK=1` builds).
//!
//! The image is one link, split by reachability into a **core** (boot,
//! radios, links, the filesystem, this module) and an **engine** (everything
//! else — the server and the shader compiler), laid out inside `factory` by
//! `lp_bootctl::SplitLayout`:
//!
//! ```text
//! 0x10000 loader · 0x15000 update progress · 0x16000/0x17000 boot records · 0x18000.. core, then engine
//! ```
//!
//! # The boot sequence
//!
//! 1. The loader (`lp-fw/fw-esp32c6-loader`) reads the two boot records,
//!    `choose`s one by the reset reason, and boots the core it names
//!    (falling back to the other record's core when that one will not load).
//! 2. [`begin`], right after `FlashStorage::new` and the partition-table
//!    read: the core reads the same records, classifies the same reset and
//!    makes the same choice. On a trial, it first counts this boot as a cold
//!    retry when it is one, then marks the record **attempted** — before its
//!    radios come up, so a core that dies in that bring-up is accountable.
//! 3. `core_boot` brings up the board, links, filesystem and radios.
//! 4. `split_boot` marks a trial **started** (from here a power cycle never
//!    counts against it).
//! 5. The core reads the engine header behind [`ENGINE_VADDR`]: a committed
//!    header of this build that fits is mapped by its own length. On a boot
//!    whose record has no `confirmed` mark (the first after a USB flash) the
//!    **engine guard** ([`engine_guard`], DD34) hashes it once against the
//!    digest slot. Then it is entered, with the **running hook** installed
//!    ([`running_hook`]): the engine's USB transport hands channel 3 — the
//!    over-the-air update protocol — to the core's update session. Otherwise
//!    — no engine, a guard mismatch, an engine that keeps crashing, or a
//!    core on trial — the core runs **core-only** ([`core_only`]): it serves
//!    the update protocol itself, confirms a trial once its link comes up,
//!    takes a core or its engine, and heals.
//!
//! The update session is `lpc-update`'s `BoardSession`; [`update_edge`] is
//! where the firmware drives it, over [`update_target_impl`]'s
//! `UpdateTarget` (the fenced flash, layout 1, `lp-bootctl`'s formats).
//!
//! The emulator scenarios for these rules are `just test-emu-c6-split-boot`
//! (`lp-cli/tests/emu_split_scenarios.rs`): run them when you touch
//! `lp-bootctl`'s records, `choose`, the loader or this module.
//!
//! # Rules from silicon (XIAO C6, 2026-10-02)
//!
//! - **No ROM SPI1 access before `FlashStorage::new`**, and none in the
//!   loader: esp-storage sizes the part with an `RDID` on SPI1, and any ROM
//!   flash access before it left that probe returning garbage, so every
//!   lpfs read failed.
//! - **No second `FlashStorage`** ([`split_flash`]): made while the radios
//!   ran, its size probe came back as garbage too.
//! - **The write fence** ([`split_flash`]): nothing is written outside the
//!   region, inside the running core, or before the fence knows both.
//! - **Trust the boot state only when it matches MMU entry 0**, every read
//!   succeeded and the flashed table gave a layout ([`BootState`]).

mod board_identity;
mod boot_state;
mod core_only;
mod engine_guard;
mod engine_window;
mod running_hook;
mod split_flash;
mod status_light;
mod update_edge;
mod update_outbox;
mod update_target_impl;

pub use board_identity::CoreIdentity;
pub use boot_state::BootState;
pub use core_only::{CoreOnly, CoreOnlyReason, core_only};
pub use engine_guard::engine_guard;
pub use engine_window::{ENGINE_VADDR, map_engine, page_size};
pub use running_hook::{install as install_running_hook, manifest as running_manifest};
pub use status_light::StatusLight;

use split_flash::SplitFlash;

/// Incomplete boots after which the core stops starting the engine: the
/// recovery ledger's safe mode (2) already skipped the project; two more
/// failures with no project loaded say the engine itself is broken.
pub const INCOMPLETE_BOOTS_TO_CORE_ONLY: u32 = 4;

/// The chip reset an update ends with (`esp_hal`'s software reset, which
/// the loader reads as warm).
pub fn reset_now() -> ! {
    esp_hal::system::software_reset()
}

/// This boot's reset, as the loader classified it.
pub fn reset_kind() -> lp_bootctl::ResetKind {
    unsafe extern "C" {
        fn rtc_get_reset_reason(cpu: u32) -> u32;
    }
    // SAFETY: a ROM routine reading a status register.
    lp_bootctl::ResetKind::from_c6_reason(unsafe { rtc_get_reset_reason(0) })
}

/// Read this boot's state and, for a trial core, count a cold retry and
/// mark it attempted — the first writes of a new core's boot, before
/// anything that could fail. `factory` is `(offset, len)` from the flashed
/// partition table, if it could be read.
pub fn begin(factory: Option<(u32, u32)>) -> BootState {
    let mut flash = SplitFlash::take();
    let mut state = BootState::read(&mut flash, factory, reset_kind());
    if let (true, Some(layout)) = (state.trusted(), state.layout) {
        flash.protect(state.core_extent(), layout.region_end);
        state.begin_trial(&mut flash);
    }
    state
}

/// A trial core finished its bring-up: mark it started.
pub fn mark_started(state: &mut BootState) {
    if let (true, Some(layout)) = (state.trusted(), state.layout) {
        let mut flash = SplitFlash::take();
        flash.protect(state.core_extent(), layout.region_end);
        state.mark_started(&mut flash);
    }
}
