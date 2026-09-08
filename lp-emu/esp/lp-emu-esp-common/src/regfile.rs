//! `RegFile` — the accept-and-remember peripheral.
//!
//! Most of an SoC's register space is a place the firmware writes and later
//! reads back. Modelling those blocks as behaviour would be a lie with a lot
//! of code in it; modelling them as memory is honest and small. What makes
//! the difference between "memory" and "a working stub" is a short table of
//! exceptions, and this type is that table:
//!
//! - **read overrides** — "this bit always reads as …". Every spin site in
//!   the C6 boot path is one of these: `I2C_ANA_MST.ana_conf0.cal_done`
//!   must read 1, `i2c_ctrl.busy` must read 0, `TIMG0.rtccalicfg.
//!   rtc_cali_rdy` must read 1.
//! - **write-one-pulse** — registers that must read back 0 after a 1-write:
//!   `UART0.reg_update`, `TIMG0.t0update`, `SYSTIMER.comp_load`,
//!   `LP_WDT.feed`, `INTPRI.cpu_intr_from_cpu` clears.
//! - **write-one-to-clear** — `int_clr`-shaped registers, where writing a 1
//!   clears the corresponding status bit.
//! - **read-only** — bits the firmware may write but hardware ignores.
//! - **reset values** — what the block reads as before anyone writes it.
//!
//! Anything a block does beyond that table gets a real type. The rule the
//! vision states as the *honest-peripheral policy* applies here too: a
//! `RegFile` never invents behaviour, it only remembers, and everything it
//! pretends about is one line in a table you can read.

use alloc::vec;
use alloc::vec::Vec;

use crate::periph::{BusCx, Peripheral, Width};
use crate::regnames::{self, RegNames};

/// A masked value applied to one register.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct MaskEntry {
    off: u32,
    mask: u32,
    value: u32,
}

/// One "these bits read as a copy of those bits" rule.
#[derive(Clone, Copy, Debug)]
struct MirrorEntry {
    off: u32,
    /// The bits watched, in the stored word.
    from: u32,
    /// The bits driven.
    to: u32,
}


/// A register window that remembers what was written, with a table of the
/// exceptions.
#[derive(Debug)]
pub struct RegFile {
    name: &'static str,
    names: RegNames,
    /// One `u32` per word of the window.
    regs: Vec<u32>,
    reset: Vec<u32>,
    read_overrides: Vec<MaskEntry>,
    read_mirrors: Vec<MirrorEntry>,
    write_one_to_clear: Vec<MaskEntry>,
    write_one_pulse: Vec<MaskEntry>,
    read_only: Vec<MaskEntry>,
}

impl RegFile {
    /// A window `size_bytes` long, all zero. `size_bytes` is rounded up to a
    /// whole number of words.
    pub fn new(name: &'static str, size_bytes: u32) -> Self {
        let words = size_bytes.div_ceil(4) as usize;
        Self {
            name,
            names: regnames::EMPTY,
            regs: vec![0; words],
            reset: vec![0; words],
            read_overrides: Vec::new(),
            read_mirrors: Vec::new(),
            write_one_to_clear: Vec::new(),
            write_one_pulse: Vec::new(),
            read_only: Vec::new(),
        }
    }

    /// Attach a generated register-name table, for the bus trace.
    pub fn with_names(mut self, names: RegNames) -> Self {
        names.assert_sorted();
        self.names = names;
        self
    }

    /// The value this register reads as before anyone writes it. Also what
    /// [`reset`](Self::reset) restores.
    pub fn with_reset(mut self, off: u32, value: u32) -> Self {
        let i = self.index(off).expect("reset offset inside the window");
        self.reset[i] = value;
        self.regs[i] = value;
        self
    }

    /// "These bits always read as `value`, whatever was written."
    ///
    /// The stored value is untouched, so the trace still shows what the
    /// firmware wrote and a later real model can be dropped in without
    /// re-deriving the state.
    pub fn with_read_override(mut self, off: u32, mask: u32, value: u32) -> Self {
        self.read_overrides.push(MaskEntry { off, mask, value });
        self
    }

    /// "These bits read as 1 exactly when those bits are set."
    ///
    /// The `done` half of a `set the enable, spin until done` pair, where the
    /// operation has no duration in this model: the C6 mask ROM's
    /// `Cache_Freeze_ICache_Enable` sets `l1_cache_freeze_ctrl` bit 16 and
    /// spins until bit 18 is 1, and `Cache_Freeze_ICache_Disable` clears bit
    /// 16 and spins until bit 18 is **0**. No constant satisfies both. A
    /// mirror does, and says the true thing while it is at it: the freeze
    /// finished, in whichever direction it was asked for.
    ///
    /// Prefer [`with_read_override`](Self::with_read_override) where a
    /// constant is honest; reach for this only when the guest waits for the
    /// bit to go both ways.
    pub fn with_read_mirror(mut self, off: u32, from: u32, to: u32) -> Self {
        self.read_mirrors.push(MirrorEntry { off, from, to });
        self
    }

    /// "Writing a 1 to these bits clears them" (`int_clr` registers).
    pub fn with_write_one_to_clear(mut self, off: u32, mask: u32) -> Self {
        self.write_one_to_clear.push(MaskEntry {
            off,
            mask,
            value: 0,
        });
        self
    }

    /// "These bits read back 0 after any write" — the update/feed/load
    /// strobes the C6 boot path spins on.
    pub fn with_write_one_pulse(mut self, off: u32, mask: u32) -> Self {
        self.write_one_pulse.push(MaskEntry {
            off,
            mask,
            value: 0,
        });
        self
    }

    /// "Hardware ignores writes to these bits."
    pub fn with_read_only(mut self, off: u32, mask: u32) -> Self {
        self.read_only.push(MaskEntry {
            off,
            mask,
            value: 0,
        });
        self
    }

    /// Length of the window in bytes.
    pub fn len_bytes(&self) -> u32 {
        (self.regs.len() * 4) as u32
    }

    /// Restore every register to its reset value.
    pub fn reset(&mut self) {
        self.regs.copy_from_slice(&self.reset);
    }

    /// The stored word at `off`, ignoring read overrides. What the firmware
    /// actually wrote.
    pub fn stored(&self, off: u32) -> u32 {
        self.index(off).map(|i| self.regs[i]).unwrap_or(0)
    }

    /// Set a register from the host side (eFuse values from a transcript
    /// header, a strap latch). Bypasses read-only and pulse masks, because
    /// this is hardware writing, not the guest.
    pub fn poke(&mut self, off: u32, value: u32) {
        if let Some(i) = self.index(off) {
            self.regs[i] = value;
        }
    }

    /// The word at `off` as the guest would read it: stored value with the
    /// read overrides applied.
    pub fn effective(&self, off: u32) -> u32 {
        let word = off & !3;
        let mut v = self.stored(word);
        for e in &self.read_overrides {
            if e.off == word {
                v = (v & !e.mask) | (e.value & e.mask);
            }
        }
        // Mirrors read the STORED word, not the overridden one: a `done`
        // bit follows what the guest wrote, and would otherwise chase
        // whatever an override had just forced.
        let stored = self.stored(word);
        for m in &self.read_mirrors {
            if m.off == word {
                v = if stored & m.from != 0 { v | m.to } else { v & !m.to };
            }
        }
        v
    }

    fn index(&self, off: u32) -> Option<usize> {
        let i = (off >> 2) as usize;
        (i < self.regs.len()).then_some(i)
    }

    fn masks_at(entries: &[MaskEntry], off: u32) -> u32 {
        entries
            .iter()
            .filter(|e| e.off == off)
            .fold(0, |acc, e| acc | e.mask)
    }
}

/// Extract the byte lane `off & 3` of `word` for `width`, right-aligned.
pub fn lane_of(word: u32, off: u32, width: Width) -> u32 {
    let shift = (off & 3) * 8;
    (word >> shift) & width.mask()
}

/// Merge a right-aligned `value` of `width` into `word` at lane `off & 3`.
pub fn merge_lane(word: u32, off: u32, width: Width, value: u32) -> u32 {
    let shift = (off & 3) * 8;
    let mask = width.lane_mask(off & 3);
    (word & !mask) | ((value << shift) & mask)
}

impl Peripheral for RegFile {
    fn name(&self) -> &'static str {
        self.name
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.effective(off), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, _cx: &mut BusCx<'_>) {
        let word_off = off & !3;
        let Some(i) = self.index(word_off) else {
            return;
        };
        let old = self.regs[i];
        let incoming = merge_lane(old, off, width, value);

        let ro = Self::masks_at(&self.read_only, word_off);
        let w1c = Self::masks_at(&self.write_one_to_clear, word_off);
        let w1p = Self::masks_at(&self.write_one_pulse, word_off);

        let mut new = incoming;
        // Read-only bits keep whatever they had.
        new = (new & !ro) | (old & ro);
        // Write-one-to-clear: a written 1 clears; a written 0 leaves it.
        new = (new & !w1c) | ((old & !incoming) & w1c);
        // Write-one-pulse: the strobe is consumed, so it reads back 0.
        new &= !w1p;

        self.regs[i] = new;
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.names.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.regs.len() * 4);
        for r in &self.regs {
            out.extend_from_slice(&r.to_le_bytes());
        }
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        if bytes.len() != self.regs.len() * 4 {
            log::warn!(
                "{}: load_state got {} bytes for a {}-byte window, ignored",
                self.name,
                bytes.len(),
                self.regs.len() * 4
            );
            return;
        }
        for (i, chunk) in bytes.chunks_exact(4).enumerate() {
            self.regs[i] = u32::from_le_bytes(chunk.try_into().expect("chunks_exact(4)"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::periph::Sandbox as Env;

    static NAMES: RegNames = RegNames {
        block: "uart0",
        entries: &[(0x000, "fifo"), (0x004, "int_raw"), (0x01c, "status")],
    };

    #[test]
    fn it_remembers_what_was_written() {
        let mut env = Env::new();
        let mut rf = RegFile::new("TEST", 0x20);
        rf.write(0x08, Width::Word, 0xdead_beef, &mut env.cx());
        assert_eq!(rf.read(0x08, Width::Word, &mut env.cx()), 0xdead_beef);
    }

    #[test]
    fn byte_and_halfword_lanes_land_where_they_should() {
        let mut env = Env::new();
        let mut rf = RegFile::new("TEST", 0x10);
        rf.write(0x00, Width::Byte, 0xaa, &mut env.cx());
        rf.write(0x03, Width::Byte, 0xdd, &mut env.cx());
        assert_eq!(rf.stored(0x00), 0xdd00_00aa);
        assert_eq!(rf.read(0x03, Width::Byte, &mut env.cx()), 0xdd);
        assert_eq!(rf.read(0x00, Width::Byte, &mut env.cx()), 0xaa);

        rf.write(0x04, Width::Half, 0x1234, &mut env.cx());
        rf.write(0x06, Width::Half, 0x5678, &mut env.cx());
        assert_eq!(rf.stored(0x04), 0x5678_1234);
        assert_eq!(rf.read(0x06, Width::Half, &mut env.cx()), 0x5678);
    }

    #[test]
    fn a_read_override_forces_bits_without_losing_what_was_written() {
        let mut env = Env::new();
        // `TIMG0.rtccalicfg.rtc_cali_rdy` (bit 15) must read 1 or the boot
        // path spins forever.
        let mut rf = RegFile::new("TIMG0", 0x80).with_read_override(0x68, 1 << 15, 1 << 15);
        rf.write(0x68, Width::Word, 0x0000_0001, &mut env.cx());
        assert_eq!(rf.read(0x68, Width::Word, &mut env.cx()), 0x0000_8001);
        // The stored value is still what the firmware wrote.
        assert_eq!(rf.stored(0x68), 0x0000_0001);
    }

    #[test]
    fn a_read_override_can_force_a_bit_low() {
        let mut env = Env::new();
        // `I2C_ANA_MST.i2c_ctrl(n).busy` must read 0.
        let mut rf = RegFile::new("I2C_ANA_MST", 0x20).with_read_override(0x00, 1 << 25, 0);
        rf.write(0x00, Width::Word, 0xffff_ffff, &mut env.cx());
        assert_eq!(rf.read(0x00, Width::Word, &mut env.cx()) & (1 << 25), 0);
    }

    #[test]
    fn write_one_pulse_reads_back_zero() {
        let mut env = Env::new();
        // `UART0.reg_update`: the driver writes 1 and spins until it reads 0.
        let mut rf = RegFile::new("UART0", 0x100).with_write_one_pulse(0x98, 1);
        rf.write(0x98, Width::Word, 1, &mut env.cx());
        assert_eq!(rf.read(0x98, Width::Word, &mut env.cx()), 0);
        assert_eq!(rf.stored(0x98), 0);
    }

    #[test]
    fn write_one_pulse_leaves_the_other_bits_of_the_register_alone() {
        let mut env = Env::new();
        let mut rf = RegFile::new("TEST", 0x10).with_write_one_pulse(0x00, 1);
        rf.write(0x00, Width::Word, 0x0000_00ff, &mut env.cx());
        assert_eq!(rf.stored(0x00), 0x0000_00fe);
    }

    #[test]
    fn write_one_to_clear_clears_only_the_written_ones() {
        let mut env = Env::new();
        let mut rf = RegFile::new("UART0", 0x100).with_write_one_to_clear(0x10, 0xffff_ffff);
        rf.poke(0x10, 0b1111);
        rf.write(0x10, Width::Word, 0b0101, &mut env.cx());
        assert_eq!(rf.stored(0x10), 0b1010);
        // Writing zeroes changes nothing.
        rf.write(0x10, Width::Word, 0, &mut env.cx());
        assert_eq!(rf.stored(0x10), 0b1010);
    }

    #[test]
    fn read_only_bits_ignore_writes_and_the_rest_of_the_word_still_takes() {
        let mut env = Env::new();
        let mut rf = RegFile::new("TEST", 0x10).with_read_only(0x00, 0xffff_0000);
        rf.poke(0x00, 0xabcd_0000);
        rf.write(0x00, Width::Word, 0x1234_5678, &mut env.cx());
        assert_eq!(rf.stored(0x00), 0xabcd_5678);
    }

    #[test]
    fn reset_values_are_what_it_reads_before_anyone_writes() {
        let mut env = Env::new();
        let mut rf = RegFile::new("TEST", 0x10).with_reset(0x04, 0x0000_1234);
        assert_eq!(rf.read(0x04, Width::Word, &mut env.cx()), 0x0000_1234);
        rf.write(0x04, Width::Word, 0, &mut env.cx());
        assert_eq!(rf.read(0x04, Width::Word, &mut env.cx()), 0);
        rf.reset();
        assert_eq!(rf.read(0x04, Width::Word, &mut env.cx()), 0x0000_1234);
    }

    #[test]
    fn names_come_from_the_attached_table() {
        let rf = RegFile::new("UART0", 0x100).with_names(NAMES);
        assert_eq!(rf.reg_name(0x000), Some("fifo"));
        assert_eq!(rf.reg_name(0x01c), Some("status"));
        assert_eq!(rf.reg_name(0x008), None);
        // Without a table, no names — never a wrong one.
        assert_eq!(RegFile::new("UART0", 0x100).reg_name(0x000), None);
    }

    #[test]
    fn accesses_past_the_window_are_dropped_and_read_zero() {
        let mut env = Env::new();
        let mut rf = RegFile::new("TEST", 0x10);
        rf.write(0x40, Width::Word, 0xffff_ffff, &mut env.cx());
        assert_eq!(rf.read(0x40, Width::Word, &mut env.cx()), 0);
        assert_eq!(rf.len_bytes(), 0x10);
    }

    #[test]
    fn save_and_load_round_trip() {
        let mut env = Env::new();
        let mut rf = RegFile::new("TEST", 0x10);
        rf.write(0x00, Width::Word, 0x1111_2222, &mut env.cx());
        rf.write(0x0c, Width::Word, 0x3333_4444, &mut env.cx());
        let blob = rf.save_state();
        assert_eq!(blob.len(), 0x10);

        let mut other = RegFile::new("TEST", 0x10);
        other.load_state(&blob);
        assert_eq!(other.stored(0x00), 0x1111_2222);
        assert_eq!(other.stored(0x0c), 0x3333_4444);
    }

    #[test]
    fn load_state_refuses_a_blob_of_the_wrong_size() {
        let mut rf = RegFile::new("TEST", 0x10);
        rf.poke(0x00, 0xaaaa_aaaa);
        rf.load_state(&[1, 2, 3]);
        assert_eq!(rf.stored(0x00), 0xaaaa_aaaa);
    }

    #[test]
    fn lane_helpers_agree_with_each_other() {
        for off in 0..4u32 {
            let merged = merge_lane(0, off, Width::Byte, 0xab);
            assert_eq!(lane_of(merged, off, Width::Byte), 0xab);
        }
        for off in [0u32, 2] {
            let merged = merge_lane(0, off, Width::Half, 0xbeef);
            assert_eq!(lane_of(merged, off, Width::Half), 0xbeef);
        }
    }
}
