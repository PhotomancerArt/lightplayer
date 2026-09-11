//! `RNG` — one register, and it is the only thing in this machine that is
//! allowed to look random.
//!
//! # Where it is, and why the PAC's base was right after all
//!
//! `esp32-0.40.2/src/lib.rs:647` puts `RNG` at `0x6003_5000`, an address
//! P1 read as an SVD leak from another chip family and excluded from the
//! generator. P3's fifth strict stop found the classic's **second peripheral
//! window** — the AHB bus at `0x6000_0000`, a mirror of the DPORT blocks
//! from `0x3FF4_0000` up — and `0x6003_5000` is the AHB address of the WiFi
//! window's WDEV block. Its single register, `data` at `+0x144`, is
//! `0x6003_5144`: the classic's **`WDEV_RND_REG`**. P3 removed the
//! generator's `SKIP` entry and struck the wrong claim from `m3/notes.md`;
//! this is the block behind the name.
//!
//! P7's ROM-up boot is what reaches it, at cycle 22,247,148 and six lines
//! into the ESP-IDF bootloader's partition table: `bootloader_fill_random()`
//! reads `WDEV_RND_REG` in a loop to seed the image-hash salt and the RNG
//! the app inherits. Ruling **R4** named this block and said it would be
//! modelled here or left unmapped and said so; the ROM-up path reached it,
//! so it is modelled here.
//!
//! # What it answers, and what that is not
//!
//! ⚠️ **This is the machine's seeded PRNG, not entropy, and the difference
//! is the point.** Plan **PD5**: a run with the same seed is the same run.
//! Silicon's `WDEV_RND_REG` is fed by the RF and SAR noise the bootloader's
//! `bootloader_random_enable()` has just switched on (`I2S0` and `SENS`, the
//! two accept blocks before this one); this register is a SplitMix64 stepped
//! once per read from [`crate::machine::Esp32V3Builder::seed`]. It is
//! therefore:
//!
//! - **not** what silicon had, and no transcript comparison may treat a
//!   value derived from it as a silicon fact;
//! - **exactly** reproducible, which is what lets the direct-load ↔ ROM-up
//!   cross-check compare two runs at all, and what puts "the RNG-stirred
//!   bytes" on the list of things the two paths are *allowed* to differ on
//!   (`crate::loader`'s eleven, item 5).
//!
//! SplitMix64 rather than anything cleverer for the machine's own reason:
//! [`crate::machine::Machine::next_random`] is the same generator, so there
//! is one notion of "this run's randomness" in the crate and not two.

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::lane_of;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs;

/// The block's aperture: one register at `+0x144`, and the window that holds
/// it. Tight — everything else in the WDEV block is a strict stop.
pub const LEN: u32 = 0x148;

/// `data` — `WDEV_RND_REG`.
pub const DATA: u32 = 0x144;

/// The hardware random-number register.
pub struct Rng {
    regs: RegFile,
    /// SplitMix64's state. Stepped once per read of [`DATA`].
    state: u64,
    /// How many words this run has handed out, for the run report.
    reads: u64,
}

impl core::fmt::Debug for Rng {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Rng").field("reads", &self.reads).finish()
    }
}

impl Rng {
    /// A generator seeded from the machine's own seed.
    pub fn new(seed: u64) -> Self {
        Self {
            regs: RegFile::new("RNG", LEN)
                .with_names(regs::RNG)
                // The PAC calls `data` read-only, which `with_pac_grades`
                // would grade `modeled`; it is graded by hand below, because
                // this block's whole behaviour is the read.
                .with_pac_grades(),
            state: seed,
            reads: 0,
        }
    }

    /// How many words this run has handed the guest.
    pub fn reads(&self) -> u64 {
        self.reads
    }

    /// One step of SplitMix64 — [`crate::machine::Machine::next_random`]'s
    /// generator, so the crate has one notion of this run's randomness.
    fn next(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        self.reads += 1;
        ((z ^ (z >> 31)) >> 32) as u32
    }
}

impl Peripheral for Rng {
    fn name(&self) -> &'static str {
        "RNG"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        if off & !3 == DATA {
            let word = self.next();
            return lane_of(word, off, width);
        }
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        if off & !3 == DATA {
            // Read-only in the PAC, and a write moves no state here: the
            // next read is the next step of the generator either way.
            return;
        }
        self.regs.write(off, width, value, cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::RNG.name(off)
    }

    /// `data` is **`modeled`**, and the grade is the claim: this register's
    /// value is a seeded PRNG's, not silicon's noise. Nothing in this crate
    /// may grade it `documented`, because what is documented about it is
    /// exactly the property the model does not have.
    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        if off & !3 == DATA {
            return Some(RegGrade::Modeled);
        }
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16);
        out.extend_from_slice(&self.state.to_le_bytes());
        out.extend_from_slice(&self.reads.to_le_bytes());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        if bytes.len() < 16 {
            log::warn!("RNG: load_state blob too short, ignored");
            return;
        }
        self.state = u64::from_le_bytes(bytes[..8].try_into().expect("8 bytes"));
        self.reads = u64::from_le_bytes(bytes[8..16].try_into().expect("8 bytes"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn the_register_is_wdev_rnd_reg_at_the_ahb_address() {
        assert_eq!(crate::memmap::periph::RNG + DATA, 0x6003_5144);
        let rng = Rng::new(0);
        assert_eq!(rng.reg_name(DATA), Some("data"));
    }

    #[test]
    fn the_same_seed_is_the_same_run() {
        let words = |seed| {
            let mut sb = Sandbox::new();
            let mut rng = Rng::new(seed);
            (0..8).map(|_| sb.read(&mut rng, DATA)).collect::<Vec<_>>()
        };
        assert_eq!(words(0), words(0), "PD5: the same seed is the same run");
        assert_ne!(words(0), words(1), "and a different one is not");
    }

    #[test]
    fn every_read_is_a_new_word() {
        let mut sb = Sandbox::new();
        let mut rng = Rng::new(7);
        let mut seen = Vec::new();
        for _ in 0..64 {
            let w = sb.read(&mut rng, DATA);
            assert!(!seen.contains(&w), "{w:#010x} came out twice in 64 reads");
            seen.push(w);
        }
        assert_eq!(rng.reads(), 64);
    }

    #[test]
    fn a_write_moves_nothing_and_the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut rng = Rng::new(3);
        sb.read(&mut rng, DATA);
        sb.write(&mut rng, DATA, 0xdead_beef);
        let blob = Peripheral::save_state(&rng);
        let next = sb.read(&mut rng, DATA);

        let mut back = Rng::new(0);
        Peripheral::load_state(&mut back, &blob);
        assert_eq!(sb.read(&mut back, DATA), next, "a restore resumes the run");
        assert_eq!(back.reads(), 2);
    }
}
