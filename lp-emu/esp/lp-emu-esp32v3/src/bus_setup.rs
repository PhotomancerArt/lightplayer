//! Building the classic's [`SocBus`] out of [`crate::memmap`].
//!
//! Every region here cites `memmap`, which cites the linker script, the ROM
//! ELF, the `esp32` PAC and the hardware-measured board facts. Nothing in
//! this file introduces an address.
//!
//! # Two decisions this file makes, and why they are written down here
//!
//! ## 1. RTC fast memory's instruction-bus view is **not mapped**
//!
//! `memory.x:51` and `:54` put the same 8 KiB block behind two windows:
//! [`memmap::RTC_FAST_IBUS`] (`0x400C_0000`) and [`memmap::RTC_FAST_DBUS`]
//! (`0x3FF8_0000`). One memory, two addresses.
//!
//! [`SocBus`] cannot express that. Its regions are asserted non-overlapping
//! and its bytes live in one flat arena keyed on `address - arena_base`, so
//! two regions are two independent stores: a write through one would be
//! invisible through the other. That is wrong in exactly the way the SRAM1
//! I-bus alias would be wrong, and worse than wrong — it is *silent*.
//!
//! So this machine maps the **D-bus view only** and leaves `0x400C_0000`
//! unmapped, consistent with ruling **DD24 / R1**'s treatment of the SRAM1
//! alias: a window nothing is known to reach is named, not mapped, and a
//! strict-bus stop inside it is the evidence that would justify building an
//! alias region and a phase of its own. [`deliberately_unmapped`] is what
//! makes such a stop say *which* window it was.
//!
//! P2 recorded this as a deviation from `memmap`'s doc comment, which then
//! promised "P2 backs them with one store"; the director ruled it **DD36**
//! and P3 amended the comment, so the two files now say the same thing:
//! the D-bus view is mapped, the I-bus view is named and unmapped, and the
//! shared bus (M2's, read-only to this milestone) is where an alias would
//! have to be built.
//!
//! ## 2. SRAM0's word-only rule is **not enforced** by this bus
//!
//! [`memmap::SRAM0_WORD_ONLY`] records a measurement: on the desk board a
//! byte store at `0x4008_8000` faulted with `LoadStoreError` / EXCCAUSE 3,
//! while 16,384 aligned word stores across the same span all read back
//! (`lp-xt-emu/src/board.rs:156-172`). `lp-xt-emu`'s own flat `Memory` has an
//! `AccessRule::WordOnly` for it.
//!
//! [`SocBus`] has no such rule — a [`RamRegion`] carries `exec` and
//! `writable` and nothing else — and adding one would be an edit to
//! `lp-emu-esp-common`, which M2 owns and M3 may only read. So on this
//! machine a guest byte store into SRAM0 **succeeds** where silicon faults.
//! Named here so it is a known gap rather than a surprise, and reported to
//! the director as a P2 finding: it wants either an `AccessRule` on
//! `RamRegion` (an M2 phase) or a chip-side check, and it is not P2's to
//! decide.

use lp_emu_esp_common::SocBus;
use lp_emu_esp_common::bus::RamRegion;

use crate::memmap::{self, Span};

/// `XCHAL_NUM_DBREAK` on the classic LX6: two data breakpoints.
///
/// The C6 has four; `MAX_WATCHPOINT_SLOTS` in the shared bus is the maximum
/// of the family, and a machine says how many of them its silicon has
/// (M2 P1). A third watchpoint on this chip is a bug in whatever armed it.
pub const WATCHPOINT_SLOTS: usize = 2;

/// The regions of [`memmap::RAM_SPANS`] this machine does not register, and
/// why. See the module docs.
const SKIPPED_SPANS: &[(&str, &str)] = &[(
    "rtc-fast-ibus",
    "the I-bus view of the same 8 KiB block as `rtc-fast-dbus`; SocBus cannot \
     alias two windows onto one store, so the D-bus view is mapped and this one is not",
)];

/// Executable regions. Everything else is data.
///
/// `rom-mask` and `irom-window` execute because they are code windows;
/// `sram0` executes because the vectors, the app's `.rwtext` and — during a
/// ROM-up boot — the IDF bootloader itself all run from it. `sram1-dbus`
/// does **not**: the classic's instruction view of SRAM1 is the alias at
/// `0x400A_0000`, which this machine deliberately does not map.
const EXECUTABLE: &[&str] = &["rom-mask", "sram0", "irom-window"];

/// Read-only regions: the mask ROM's own two, and the two flash cache
/// windows (P7 fills those through the cache MMU, not through a guest store).
const READ_ONLY: &[&str] = &["rom-mask", "rom-data", "drom-window", "irom-window"];

/// Build the classic's bus: every region of [`memmap::RAM_SPANS`] this
/// machine maps, the MMIO windows declared and **unmodelled**, and two
/// watchpoint slots.
///
/// No peripheral is registered here. That is the point of P2: with the MMIO
/// window declared and nothing inside it, a `--strict-bus` run stops at the
/// **first** block the boot touches and says which, which is exactly the
/// reading P3 needs to fix the order it models them in.
pub fn build() -> SocBus {
    let mut bus = SocBus::new();

    // Reserve the whole span first so the arena is allocated once and never
    // moves: `add_region` would otherwise grow and copy it eleven times.
    let lo = mapped_spans().map(|s| s.base).min().unwrap_or(0);
    let hi = mapped_spans().map(Span::end).max().unwrap_or(0);
    bus.reserve_guest_span(lo, hi - lo);

    for span in mapped_spans() {
        let mut region = RamRegion::new(span.name, span.base, span.len);
        if EXECUTABLE.contains(&span.name) {
            region = region.executable();
        }
        if READ_ONLY.contains(&span.name) {
            region = region.read_only();
        }
        bus.add_region(region);
    }

    for window in memmap::MMIO_WINDOWS {
        bus.add_mmio_window(window.base, window.len);
    }

    bus.set_watchpoint_slots(WATCHPOINT_SLOTS);
    bus
}

/// The spans [`build`] registers: [`memmap::RAM_SPANS`] minus
/// [`SKIPPED_SPANS`].
fn mapped_spans() -> impl Iterator<Item = &'static Span> {
    memmap::RAM_SPANS
        .iter()
        .filter(|s| !SKIPPED_SPANS.iter().any(|(name, _)| *name == s.name))
}

/// Every window this machine names and deliberately does **not** map, with
/// the reason. A strict stop inside one of these says which window it was
/// and why it is absent, instead of the far less useful "unmapped".
///
/// [`memmap::UNMAPPED_BY_DESIGN`]'s entries (the SRAM1 I-bus alias) plus
/// this file's own (RTC fast's I-bus view).
pub fn deliberately_unmapped() -> Vec<(Span, &'static str)> {
    let mut out: Vec<(Span, &'static str)> = memmap::UNMAPPED_BY_DESIGN
        .iter()
        .map(|s| {
            (
                *s,
                "the SRAM1 instruction-bus alias; M0 measured zero references to it in the \
                 shipped image and in the mask ROM (DD24)",
            )
        })
        .collect();
    for (name, why) in SKIPPED_SPANS {
        if let Some(span) = memmap::RAM_SPANS.iter().find(|s| s.name == *name) {
            out.push((*span, why));
        }
    }
    out.sort_by_key(|(s, _)| s.base);
    out
}

/// The name of the deliberately-unmapped window holding `address`, if any.
/// What a strict-stop report asks before it prints "unmapped".
pub fn unmapped_window(address: u32) -> Option<(Span, &'static str)> {
    deliberately_unmapped()
        .into_iter()
        .find(|(span, _)| span.contains(address))
}
