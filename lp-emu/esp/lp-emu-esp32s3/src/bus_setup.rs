//! Building the S3's [`SocBus`] out of [`crate::memmap`].
//!
//! Every region here cites `memmap`, which cites the linker script, the ROM
//! ELF, the PAC and the inventory. **Nothing in this file introduces an
//! address.**
//!
//! # The one decision this file makes, and why it is written down here
//!
//! **SRAM1's instruction-bus view is a RAM alias, not a second region.**
//!
//! `memory.x:13-14` puts the same 416 KiB behind two addresses:
//! [`memmap::SRAM1_DBUS_BASE`] (`0x3FC8_8000`) and
//! [`memmap::SRAM1_IBUS_BASE`] (`0x4037_8000`), `0x6F_0000` apart. Two
//! regions would be two independent stores keyed on `address - arena_base`,
//! so a write through one would be invisible through the other — the failure
//! DD24/DD36 refused on the classic and the C6, where nothing reached the
//! alias so the honest answer was to name it and leave it unmapped.
//!
//! On this chip something *does* reach it, on the product path, on every
//! shader the firmware compiles (`m6/notes.md` §2.6). So M6 P02 taught the
//! shared bus [`SocBus::add_ram_alias`]: one store, two doors, translated at
//! the top of every access path before the region lookup, the watchpoints,
//! the cost model and the guest-code spans (ruling DD81). This file is that
//! API's only caller in the tree.
//!
//! ⚠️ **Which is why `sram1-dbus` is in [`EXECUTABLE`].** A fetch through
//! `0x4037_8400` reaches the region lookup as `0x3FC8_8400`; the D-bus
//! region's flag is what decides whether it is allowed. A reader who moved
//! the flag to "the I-bus side" would find there is no I-bus side to put it
//! on.
//!
//! # What this file does NOT do
//!
//! - **It registers no peripheral.** That is the point of P03: with the MMIO
//!   window declared and nothing inside it, a `--strict-bus` run stops at the
//!   **first** block the boot touches and says which — the reading P04 needs
//!   to model them in the order the boot meets them.
//! - **It declares no access rule.** The classic's `word_only` on SRAM0 is a
//!   measurement on classic silicon; no equivalent measurement exists for the
//!   S3 and this phase does not invent one (see `memmap`'s module docs).
//! - **It registers no peripheral alias.** The classic's AHB mirror (DD38) is
//!   the classic's; the S3's blocks live at one address each.

use lp_emu_esp_common::SocBus;
use lp_emu_esp_common::bus::RamRegion;

use crate::memmap::{self, Span};

/// `XCHAL_NUM_DBREAK` on the LX7: two data breakpoints, as on the classic's
/// LX6. `MAX_WATCHPOINT_SLOTS` in the shared bus is the maximum of the family
/// (the C6 has four) and a machine says how many its silicon has.
///
/// The shipped S3 image reaches the **second** slot — `rsr.dbreakc1` ×4 and
/// `rsr.dbreaka1` ×1 (`m6/notes.md` §2.3) — where the classic image only ever
/// touched slot 0. `tests/isa_gaps.rs` runs both.
pub const WATCHPOINT_SLOTS: usize = 2;

/// Executable regions. Everything else is data.
///
/// `rom-mask` and `irom-window` execute because they are code windows.
/// `rtc-fast` executes because `memory.x:44` declares `rtc_fast_seg` `RWX`,
/// even though this image's `.rtc_fast.text` is size 0. **`sram1-dbus`
/// executes because the I-bus alias's fetches arrive at its canonical
/// address** — see the module docs; this is the load-bearing one.
pub const EXECUTABLE: &[&str] = &["sram1-dbus", "rom-mask", "irom-window", "rtc-fast"];

/// Read-only regions: the mask ROM's two, and the two flash cache windows
/// (P06 fills those through the MMU, not through a guest store).
pub const READ_ONLY: &[&str] = &["rom-mask", "rom-data", "drom-window", "irom-window"];

/// Build the S3's bus: every region of [`memmap::RAM_SPANS`], the SRAM1 I-bus
/// alias over the top of it, the MMIO window declared and **unmodelled**, and
/// two watchpoint slots.
pub fn build() -> SocBus {
    let mut bus = SocBus::new();

    // Reserve the whole span first so the arena is allocated once and never
    // moves: `add_region` would otherwise grow and copy it seven times, and
    // the two 32 MiB flash windows make that expensive rather than merely
    // wasteful. The arena is `mmap`ed where the host can, so the span between
    // the regions costs address space and not pages.
    let lo = memmap::RAM_SPANS
        .iter()
        .map(|s| s.base)
        .min()
        .expect("the map has regions");
    let hi = memmap::RAM_SPANS
        .iter()
        .map(Span::end)
        .max()
        .expect("the map has regions");
    bus.reserve_guest_span(lo, hi - lo);

    for span in memmap::RAM_SPANS {
        let mut region = RamRegion::new(span.name, span.base, span.len);
        if EXECUTABLE.contains(&span.name) {
            region = region.executable();
        }
        if READ_ONLY.contains(&span.name) {
            region = region.read_only();
        }
        bus.add_region(region);
    }

    // After the regions: an alias needs its target registered, and the bus
    // asserts as much.
    for (alias, target) in memmap::RAM_ALIASES {
        bus.add_ram_alias(alias.base, alias.len, *target);
    }

    for window in memmap::MMIO_WINDOWS {
        bus.add_mmio_window(window.base, window.len);
    }

    bus.set_watchpoint_slots(WATCHPOINT_SLOTS);
    bus
}

/// Every window this machine names and deliberately does **not** map, with
/// the reason. A strict stop inside one of these says which window it was and
/// why it is absent, instead of the far less useful "unmapped".
///
/// One entry: the icache reserve. See [`crate::memmap`]'s module docs (b) for
/// why P03 leaves it unmapped and what would change the answer.
pub fn deliberately_unmapped() -> Vec<(Span, &'static str)> {
    memmap::UNMAPPED_BY_DESIGN
        .iter()
        .map(|s| {
            (
                *s,
                "RESERVE_ICACHE (memory.x:1-7): with the instruction cache on this memory IS \
                 the cache array, and `vectors_seg` starts above it. The classic maps its \
                 equivalent because its ROM-up bootloader really executes there (DD24 R1); \
                 the S3 has no ROM-up path until P06, so there is no evidence to map it on \
                 and this phase invents none",
            )
        })
        .collect()
}

/// The name of the deliberately-unmapped window holding `address`, if any.
/// What a strict-stop report asks before it prints "unmapped".
pub fn unmapped_window(address: u32) -> Option<(Span, &'static str)> {
    deliberately_unmapped()
        .into_iter()
        .find(|(span, _)| span.contains(address))
}

/// The alias span holding `address`, and the canonical address it resolves
/// to, if any.
///
/// Reporting only — the translation itself is the bus's ([`SocBus::canonical`]
/// is private, and DD81 says it stays that way). A `--map` printer and a
/// fault message use this to say *which door* an address is, which DD81
/// otherwise deliberately hides: a fault through an alias reports the
/// canonical address, so without this nothing could name the alias at all.
pub fn ram_alias_of(address: u32) -> Option<(Span, u32)> {
    memmap::RAM_ALIASES
        .iter()
        .find(|(span, _)| span.contains(address))
        .map(|(span, target)| (*span, address - span.base + target))
}
