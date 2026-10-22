//! The split image's update board side, shared by the chips that ship one
//! (feature `split-core`; used by `LP_SPLIT_LINK=1` builds): the boot
//! records' marks, the engine guard, the update session's edge, core-only.
//!
//! The image is one link, split by reachability into a **core** (boot,
//! radios, links, the filesystem, this module) and an **engine** (everything
//! else — the server and the shader compiler), laid out inside `factory` by
//! `lp_bootctl::SplitLayout` — layout 1 on the C6:
//!
//! ```text
//! 0x10000 loader · 0x15000 update progress · 0x16000/0x17000 boot records · 0x18000.. core, then engine
//! ```
//!
//! and layout 2 on the S3 (the same below `0x18000`, the region from
//! `0x20000` on 64 KiB pages).
//!
//! # The chip
//!
//! Everything here is chip-free. What needs a chip — its flash MMU, its raw
//! flash calls, its SHA, its reset, the watchdog and the update light — is
//! one [`SplitChip`], generic and instantiated once per image, so every call
//! is as direct as it was when this code was the C6's own
//! (`fw-esp32c6/src/split_core/` is the C6's chip). The two statics that hold
//! chip types are the chip's [`SplitStatics`].
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
//! 5. The core reads the engine header behind
//!    [`SplitChip::ENGINE_DATA_VADDR`] ([`choose_engine`]): a committed
//!    header of this build that fits is mapped by its own length. On a boot
//!    whose record has no `confirmed` mark (the first after a USB flash) the
//!    **engine guard** ([`engine_guard`], DD34) hashes it once against the
//!    digest slot. Then it is entered, with the **running hooks** installed
//!    ([`running_hook`]): the engine's USB transport, and its link mux for
//!    each radio link (with the tier the link's login or key granted), hand
//!    channel 3 — the over-the-air update protocol — to the core's update
//!    session. Otherwise — no engine, a guard mismatch, an engine that keeps
//!    crashing, or a core on trial — the core runs **core-only**
//!    ([`core_only`]): it serves the update protocol itself on USB and on
//!    every radio link, confirms a trial once any link comes up, takes a
//!    core or its engine, and heals.
//!
//! Which of the two it is also decides the boot's **radio link mode**
//! (`RadioLinkMode`), before the radio side may open any link: engine →
//! `Serve`, core-only → `Update` (the wide receive window). `split_boot`
//! decides it in the same synchronous run as the engine choice, so no link
//! can open first, and a link that tries waits for it.
//!
//! The update session is `lpc-update`'s `BoardSession`; [`update_edge`] is
//! where the firmware drives it, over [`update_target_impl`]'s
//! `UpdateTarget` (the fenced flash, the chip's layout, `lp-bootctl`'s
//! formats), and
//! answers on the link each message came from ([`update_links`], one
//! [`update_outbox`] per link). Core-only runs as its own embassy task, as
//! the engine's server loop does. The protocol and its compatibility rules
//! are `docs/adr/2026-10-06-ota-update-protocol.md`.
//!
//! # The update light
//!
//! The engine records the first WS281x strip it opens in
//! `/.lp/status-light.json` (the chip's `output::status_light_note`), and
//! core-only lights the first few LEDs of it ([`SplitChip::Light`]; the C6's
//! from RMT RAM alone): dark yellow while updating, dark red while it needs
//! an engine, off otherwise. Its log line says which colour it drove.
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
//! - **No second `FlashStorage`** ([`SplitFlash`]): made while the radios
//!   ran, its size probe came back as garbage too.
//! - **The write fence** ([`SplitFlash`]): nothing is written outside the
//!   region, inside the running core, or before the fence knows both.
//! - **Trust the boot state only when it matches MMU entry 0**, every read
//!   succeeded and the flashed table gave a layout ([`BootState`]).

mod board_identity;
mod boot_state;
mod core_only;
mod engine_choice;
mod engine_guard;
mod running_hook;
mod split_chip;
mod split_flash;
mod split_statics;
mod update_edge;
mod update_links;
mod update_outbox;
mod update_target_impl;
mod update_timing;

pub use board_identity::CoreIdentity;
pub use boot_state::BootState;
pub use core_only::{CoreOnly, CoreOnlyReason, core_only};
pub use engine_choice::{choose_engine, log_boot_state};
pub use engine_guard::engine_guard;
#[cfg(feature = "radio-link")]
pub use running_hook::radio_update_hook;
pub use running_hook::{
    hash_step as core_hash_step, hello_ready, install as install_running_hook,
    manifest as running_manifest,
};
pub use split_chip::{FeedWatchdog, ScratchWindow, Sha256Stream, SplitChip, UpdateLight};
pub use split_flash::{BLOCK, SECTOR, SplitFlash};
pub use split_statics::SplitStatics;
pub use update_edge::state_word;
pub use update_links::UpdateLinks;

/// Incomplete boots after which the core stops starting the engine: the
/// recovery ledger's safe mode (2) already skipped the project; two more
/// failures with no project loaded say the engine itself is broken.
pub const INCOMPLETE_BOOTS_TO_CORE_ONLY: u32 = 4;

/// Read this boot's state and, for a trial core, count a cold retry and
/// mark it attempted — the first writes of a new core's boot, before
/// anything that could fail. `factory` is `(offset, len)` from the flashed
/// partition table, if it could be read.
pub fn begin<C: SplitChip>(factory: Option<(u32, u32)>) -> BootState {
    let mut flash = SplitFlash::<C>::take();
    let mut state = BootState::read(&mut flash, factory, C::reset_kind());
    if let (true, Some(layout)) = (state.trusted(), state.layout) {
        flash.protect(state.core_extent(), layout.region_end);
        state.begin_trial(&mut flash);
    }
    state
}

/// A trial core finished its bring-up: mark it started.
pub fn mark_started<C: SplitChip>(state: &mut BootState) {
    if let (true, Some(layout)) = (state.trusted(), state.layout) {
        let mut flash = SplitFlash::<C>::take();
        flash.protect(state.core_extent(), layout.region_end);
        state.mark_started(&mut flash);
    }
}
