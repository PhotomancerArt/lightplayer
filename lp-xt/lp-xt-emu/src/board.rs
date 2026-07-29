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
//! Every number below is hardware-measured, never recalled: the S3 map from
//! the original spike (FINDINGS E2, `fw/spike-esp32s3`), the classic map from
//! the C1–C5 ladder (FINDINGS "classic ESP32 (LX6)" section, `fw/spike-esp32`,
//! run 2026-07-28 on rev v3.0 silicon).

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
}

impl BoardProfile {
    /// ESP32-S3 (LX7). The numbers are the spike's measured map (FINDINGS E2):
    /// SRAM1 dual-mapped D-bus `0x3FC8_8000..0x3FCF_0000`, executable alias a
    /// constant `+0x6F_0000`. Code and stack are both SRAM1 regions, exactly
    /// as [`crate::Emulator::new`] has always laid them out.
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
    pub fn esp32() -> BoardProfile {
        BoardProfile {
            name: "esp32",
            code_dbus_base: 0x3FFE_8000,
            code_region_len: 0x0001_7000, // 92 KiB
            stack_dbus_base: 0x3FFC_0000,
            stack_region_len: 0x0001_0000, // 64 KiB
            stack_dual_mapped: false,
            alias: AliasRule::WordMirrored {
                dram_base: 0x3FFE_0000,
                iram_top: 0x400B_FFFC,
            },
            dbus_window_start: 0x3FFE_0000,
            dbus_window_end: 0x4000_0000,
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

    /// Install this profile's regions into `mem`, validating that anything
    /// dual-mapped actually sits inside the dual-mapped window (the
    /// profile-relative form of the assert `Memory::add_sram1` applies for
    /// the S3).
    pub fn install(&self, mem: &mut Memory) {
        self.assert_in_window("code", self.code_dbus_base, self.code_region_len);
        mem.add_executable(self.code_dbus_base, self.code_region_len, self.alias);
        if self.stack_dual_mapped {
            self.assert_in_window("stack", self.stack_dbus_base, self.stack_region_len);
            mem.add_executable(self.stack_dbus_base, self.stack_region_len, self.alias);
        } else {
            mem.add_ram(self.stack_dbus_base, self.stack_region_len);
        }
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
