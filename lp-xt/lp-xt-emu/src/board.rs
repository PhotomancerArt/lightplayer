//! Per-board memory-map profiles.
//!
//! The emulator's instruction semantics are board-independent (FINDINGS:
//! "no ISA divergence in the executed instruction set" between LX7 and LX6 —
//! the divergence is entirely in the memory system). What differs per board is
//! *where code and stack live* and *how the D-bus (data) view of code memory
//! maps to the I-bus (executable) view*. [`BoardProfile`] captures exactly
//! that; [`crate::Emulator::with_profile`] builds an emulator on any profile,
//! and [`crate::Emulator::new`] keeps the ESP32-S3 profile as the default.
//!
//! Every SRAM number below is hardware-measured, never recalled: the S3 map
//! from the original spike (FINDINGS E2, `fw/spike-esp32s3`), the classic map
//! from the C1–C5 ladder (FINDINGS "classic ESP32 (LX6)" section,
//! `fw/spike-esp32`, run 2026-07-28 on rev v3.0 silicon).
//!
//! The **flash** numbers are a weaker but still stated grade of evidence:
//! *documented and observed*, not probed by us. Each carries its source
//! inline — an MIT/Apache-2.0 linker script, an in-repo citation, or a boot
//! log. They are labelled as such rather than blended in with the measured
//! ones, because the difference matters when one of them turns out wrong.
//!
//! # Why flash is modeled at all
//!
//! On every ESP32 the application's code executes from **flash through the
//! cache** (XIP; a boot log shows `vaddr=0x4200_0020 map` segments). Only
//! code a JIT produces at runtime lives in SRAM. Collapsing the two into one
//! SRAM region — which this emulator did until 2026-08-01 — makes the
//! resident builtins image compete with JIT'd shader code for the same bytes,
//! and hides the fact that a shader→builtin call crosses from SRAM to flash,
//! tens of megabytes away and far outside any direct-call displacement. See
//! `docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md`.

use crate::memory::{AliasRule, IBUS_ALIAS_OFFSET, Memory, SRAM1_DBUS_END, SRAM1_DBUS_START};

/// A board's memory map, as the emulator needs it: where payload code goes,
/// where the stack goes, and the rule mapping D-bus code addresses to their
/// executable I-bus view.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BoardProfile {
    /// Human-readable board name (diagnostics only).
    pub name: &'static str,
    /// D-bus base of the payload code region.
    pub code_dbus_base: u32,
    /// Length of the code region in bytes.
    pub code_region_len: usize,
    /// D-bus base of the stack region.
    pub stack_dbus_base: u32,
    /// Length of the stack region in bytes.
    pub stack_region_len: usize,
    /// Whether the stack region is inside the dual-mapped window (and thus
    /// also fetchable via `alias`). True on the S3, where everything sits in
    /// SRAM1; false on classic, where the stack models plain data RAM with no
    /// I-bus view — so fetching a stack address faults exactly as the classic
    /// heap does on hardware (FINDINGS C2g, EXCCAUSE=2).
    pub stack_dual_mapped: bool,
    /// The D-bus → I-bus mapping rule for the dual-mapped window.
    pub alias: AliasRule,
    /// D-bus start of the dual-mapped window the code region must sit in.
    pub dbus_window_start: u32,
    /// D-bus end (exclusive) of the dual-mapped window.
    pub dbus_window_end: u32,
    /// Base of the flash **instruction** window (XIP `.text`), read-only and
    /// directly executable — the flash cache presents instruction addresses
    /// with no separate data view to alias from, so the region carries
    /// [`AliasRule::Identity`].
    pub irom_base: u32,
    /// Modeled length of the flash instruction window. See
    /// [`Self::MODELED_IROM_LEN`] for why this is not the hardware length.
    pub irom_len: usize,
    /// Base of the flash **data** window (XIP `.rodata`), read-only and not
    /// fetchable.
    pub drom_base: u32,
    /// Modeled length of the flash data window.
    pub drom_len: usize,
    /// Base of the internal-SRAM data region a flash-resident image's
    /// `.data`/`.bss` live in — the emulator's `dram_seg`. Plain data RAM: no
    /// I-bus view, so fetching there faults.
    pub image_data_base: u32,
    /// Length of that region.
    pub image_data_len: usize,
}

impl BoardProfile {
    /// Modeled length of a flash instruction window.
    ///
    /// Hardware gives far more — 32 MB of `irom_seg` on the S3, 3 MB on
    /// classic — but the emulator *allocates* its regions, and one is built per
    /// host call, so the modeled window is sized to the job: the resident
    /// builtins image plus generous headroom. The base addresses are hardware;
    /// only the lengths are the model's, and a segment past the end is a loud
    /// load error, never a silent wrap.
    pub const MODELED_IROM_LEN: usize = 0x0004_0000; // 256 KiB
    /// Modeled length of a flash data window. Same reasoning as
    /// [`Self::MODELED_IROM_LEN`].
    pub const MODELED_DROM_LEN: usize = 0x0001_0000; // 64 KiB
    /// Length of the internal-SRAM region holding a flash-resident image's
    /// `.data`/`.bss`. The builtins image currently emits **neither** (its
    /// only segments are `.text` and `.rodata`), so this is headroom for the
    /// day one appears rather than space in use.
    pub const IMAGE_DATA_LEN: usize = 0x0000_8000; // 32 KiB

    /// ESP32-S3 (LX7). The SRAM numbers are the spike's measured map (FINDINGS
    /// E2): SRAM1 dual-mapped D-bus `0x3FC8_8000..0x3FCF_0000`, executable
    /// alias a constant `+0x6F_0000`. Code and stack are both SRAM1 regions,
    /// exactly as [`crate::Emulator::new`] has always laid them out.
    ///
    /// The flash numbers are **documented, not probed**:
    ///
    /// - IROM `0x4200_0000` — esp-hal 1.1.1 (MIT OR Apache-2.0),
    ///   `ld/esp32s3/memory.x`: `irom_seg ORIGIN = 0x42000020, len = 32M - 0x20`
    ///   (the `0x20` is an image-header convenience, not a hardware boundary).
    ///   Corroborated in-repo: `lpc-shared`'s backtrace validator already
    ///   accepts S3 text at `0x4200_0000..0x4400_0000`
    ///   (`lp-core/lpc-shared/src/backtrace.rs`), and S3 boot logs show
    ///   `vaddr=0x42000020 map`.
    /// - DROM `0x3C00_0000` — same file, `drom_seg ORIGIN = 0x3C000020`.
    ///
    /// `image_data_base` `0x3FCA_8000` is the free SRAM1 span between the code
    /// region (ends `0x3FCA_8000`) and the stack region (starts `0x3FCC_0000`)
    /// inside the measured `0x3FC8_8000..0x3FCF_0000` window — internal data
    /// RAM by the same measurement that placed the other two.
    pub fn esp32s3() -> BoardProfile {
        BoardProfile {
            name: "esp32s3",
            code_dbus_base: 0x3FC8_8000,
            code_region_len: 0x0002_0000, // 128 KiB
            stack_dbus_base: 0x3FCC_0000,
            stack_region_len: 0x0002_0000, // 128 KiB
            stack_dual_mapped: true,
            alias: AliasRule::Offset(IBUS_ALIAS_OFFSET),
            dbus_window_start: SRAM1_DBUS_START,
            dbus_window_end: SRAM1_DBUS_END,
            irom_base: 0x4200_0000,
            irom_len: Self::MODELED_IROM_LEN,
            drom_base: 0x3C00_0000,
            drom_len: Self::MODELED_DROM_LEN,
            image_data_base: 0x3FCA_8000,
            image_data_len: Self::IMAGE_DATA_LEN,
        }
    }

    /// Classic ESP32 (LX6). Every number is from the C1–C5 hardware ladder
    /// (FINDINGS classic section, measured 2026-07-28 on rev v3.0):
    ///
    /// - **Code = SRAM1**, the region a runner would use (~96 KB usable vs the
    ///   8 KB RTC-fast ceiling; SRAM0 is the ~125 KB alternative but takes
    ///   word-only writes and has no D-bus view). SRAM1's dual mapping is
    ///   **word-mirrored** — C2b: `iram = 0x400B_FFFC − (dram − 0x3FFE_0000)`,
    ///   H2 matched all 5 sentinels, H1-linear none. Window: D-bus
    ///   `0x3FFE_0000..0x4000_0000` ↔ I-bus `0x400A_0000..0x400C_0000`.
    ///   The code region `0x3FFE_8000 + 0x1_7000` sits inside the measured
    ///   free span (dram2_seg `0x3FFE_7E30..0x3FFF_FF80`, ~96 KB), rounded in
    ///   to word-aligned bases; its I-bus image is
    ///   `0x400A_1000..0x400B_8000`.
    /// - **Stack = SRAM2** (dram_seg, the plain data RAM at `0x3FFA_E000`,
    ///   192 KB total): 64 KiB at `0x3FFC_0000`. On hardware the runner's
    ///   stack comes from ordinary data RAM (C5 measured 98 304 B heap free,
    ///   so a 64 KiB stack arena fits with headroom); modeling it as a plain
    ///   region also reproduces "SRAM2 is NOT executable" (C2g).
    ///
    /// The flash numbers are **documented, not probed**, from esp-hal 1.1.1
    /// (MIT OR Apache-2.0), `ld/esp32/memory.x`: `irom_seg ORIGIN = 0x400D0020,
    /// len = 3M - 0x20` and `drom_seg ORIGIN = 0x3F400020, len = 4M - 0x20`
    /// (the `0x20` is an image-header convenience, not a hardware boundary).
    /// Classic's IROM sits *above* SRAM1's I-bus window `0x400A_0000..
    /// 0x400C_0000`, so it neither overlaps the measured mirror nor disturbs
    /// it — the word-mirrored alias is untouched by this profile's flash.
    ///
    /// `image_data_base` `0x3FFD_0000` is SRAM2 immediately above the stack
    /// region, inside the same `dram_seg` (`0x3FFA_E000 + 192 KB`) the stack
    /// was measured into.
    pub fn esp32() -> BoardProfile {
        BoardProfile {
            name: "esp32",
            code_dbus_base: 0x3FFE_8000,
            // 32 KiB. Kept in lockstep with `lpvm_native::codemem_esp32::
            // CodeRegion::ESP32_DEFAULT` — `lpvm-native`'s
            // `tests/xt_classic_profile.rs` asserts the two describe the same
            // region, so a change on either side alone fails there. Sized
            // from the measured shader corpus, not chosen; the remaining
            // 64 KiB of SRAM1 is heap on the device.
            code_region_len: 0x0000_8000,
            stack_dbus_base: 0x3FFC_0000,
            stack_region_len: 0x0001_0000, // 64 KiB
            stack_dual_mapped: false,
            alias: AliasRule::WordMirrored {
                dram_base: 0x3FFE_0000,
                iram_top: 0x400B_FFFC,
            },
            dbus_window_start: 0x3FFE_0000,
            dbus_window_end: 0x4000_0000,
            irom_base: 0x400D_0000,
            irom_len: Self::MODELED_IROM_LEN,
            drom_base: 0x3F40_0000,
            drom_len: Self::MODELED_DROM_LEN,
            image_data_base: 0x3FFD_0000,
            image_data_len: Self::IMAGE_DATA_LEN,
        }
    }

    /// The I-bus address where byte 0 of a payload blob goes — the *lowest*
    /// I-bus address of the code region's executable image, so a blob loaded
    /// there is I-bus-contiguous (word `i` at `code_ibus_base() + 4*i`).
    ///
    /// Under an offset/identity alias that is the image of the D-bus base;
    /// under a word-mirrored alias the D-bus base maps to the *top* of the
    /// image, so the base of the image is the D-bus *last word* — the runner's
    /// D-bus write address walks downward as the code grows (FINDINGS C2b).
    pub fn code_ibus_base(&self) -> u32 {
        match self.alias {
            AliasRule::WordMirrored { .. } => self
                .alias
                .dbus_to_ibus(self.code_dbus_base + self.code_region_len as u32 - 4),
            _ => self.alias.dbus_to_ibus(self.code_dbus_base),
        }
    }

    /// Initial stack pointer: top of the stack region, 16-aligned.
    pub fn initial_sp(&self) -> u32 {
        self.stack_dbus_base + self.stack_region_len as u32 - 16
    }

    /// Offset of `vaddr` within the SRAM code region, named through **either**
    /// view — its D-bus address or its executable I-bus alias.
    ///
    /// This region belongs to JIT-produced code. A resident image landing here
    /// is the bug the flash model exists to prevent, so consumers check.
    pub fn code_region_offset(&self, vaddr: u32) -> Option<usize> {
        offset_in(vaddr, self.code_dbus_base, self.code_region_len).or_else(|| {
            offset_in(
                self.alias.ibus_to_dbus(vaddr),
                self.code_dbus_base,
                self.code_region_len,
            )
        })
    }

    /// Whether `vaddr` falls in the modeled flash instruction window, and its
    /// offset there.
    pub fn irom_offset(&self, vaddr: u32) -> Option<usize> {
        offset_in(vaddr, self.irom_base, self.irom_len)
    }

    /// Whether `vaddr` falls in the modeled flash data window, and its offset.
    pub fn drom_offset(&self, vaddr: u32) -> Option<usize> {
        offset_in(vaddr, self.drom_base, self.drom_len)
    }

    /// Whether `vaddr` falls in the image's internal-SRAM data region, and its
    /// offset.
    pub fn image_data_offset(&self, vaddr: u32) -> Option<usize> {
        offset_in(vaddr, self.image_data_base, self.image_data_len)
    }

    /// Install this profile's regions into `mem`, validating that anything
    /// dual-mapped actually sits inside the dual-mapped window (the
    /// profile-relative form of the assert `Memory::add_sram1` applies for
    /// the S3).
    ///
    /// Five regions: the SRAM code region (JIT'd code only — the resident
    /// image is in flash), the stack, the image's SRAM `.data`/`.bss`, and the
    /// two read-only flash windows. `Memory` asserts they are mutually
    /// disjoint in both address views.
    pub fn install(&self, mem: &mut Memory) {
        self.assert_in_window("code", self.code_dbus_base, self.code_region_len);
        mem.add_executable(self.code_dbus_base, self.code_region_len, self.alias);
        if self.stack_dual_mapped {
            self.assert_in_window("stack", self.stack_dbus_base, self.stack_region_len);
            mem.add_executable(self.stack_dbus_base, self.stack_region_len, self.alias);
        } else {
            mem.add_ram(self.stack_dbus_base, self.stack_region_len);
        }
        mem.add_ram(self.image_data_base, self.image_data_len);
        // Flash: read-only both ways. The instruction window is fetchable at
        // its own addresses (Identity); the data window is not fetchable at all.
        mem.add_rom(self.irom_base, self.irom_len, Some(AliasRule::Identity));
        mem.add_rom(self.drom_base, self.drom_len, None);
    }

    fn assert_in_window(&self, what: &str, base: u32, len: usize) {
        let end = base as u64 + len as u64;
        assert!(
            base >= self.dbus_window_start && end <= self.dbus_window_end as u64,
            "{}: {what} region {base:#x}+{len:#x} outside the dual-mapped window \
             {:#x}..{:#x}",
            self.name,
            self.dbus_window_start,
            self.dbus_window_end,
        );
    }
}

/// Offset of `vaddr` within `[base, base + len)`, if it lands there.
fn offset_in(vaddr: u32, base: u32, len: usize) -> Option<usize> {
    (vaddr >= base && (vaddr as u64) < base as u64 + len as u64).then(|| (vaddr - base) as usize)
}
