//! `RNG` at `0x6003_4F6C` — the PAC's one-register block (`data` at
//! `+0x110`, `0x6003_507C`) — a **deterministic** random source.
//!
//! The ESP-IDF second-stage bootloader's `bootloader_fill_random` reads this
//! word for the salt of the image hash it prints nothing about and the
//! `esp_random` behind it; the shipped application never reads it
//! (`m6/notes.md` §2.4 names no RNG). Every read of `data` advances an
//! xorshift64\* generator seeded from the machine's `--seed`, so a run is
//! the same run (plan PD5) and a different seed is a different one —
//! ruling R4, the C6's and the classic's `RNG` view at the S3's address.
//! Silicon's RNG is not reproducible; this one is, on purpose, and says so:
//! the bootloader's *"Enabling RNG early entropy source"* step ends in a
//! number this machine chose.
//!
//! Every other offset in the block's aperture is unmapped-inside-a-block:
//! the PAC names one register, and an access anywhere else is a strict
//! stop naming this block rather than a value invented for it.

use lp_emu_esp_common::regfile::lane_of;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs;

/// `data` — "Random number data", read-only.
pub const DATA: u32 = 0x110;

/// The block's aperture, through `data`.
pub const LEN: u32 = 0x120;

/// The RNG block.
#[derive(Debug)]
pub struct Rng {
    regs: RegFile,
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            regs: RegFile::new("RNG", LEN)
                .with_names(regs::RNG)
                .with_pac_grades(),
            // xorshift needs a non-zero state; the constant is SplitMix64's.
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// The next 32 bits (xorshift64\*, Vigna 2016), advancing the state.
    pub fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
}

impl Peripheral for Rng {
    fn name(&self) -> &'static str {
        "RNG"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        if off & !3 == DATA {
            return lane_of(self.next(), off, width);
        }
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        if off & !3 == DATA {
            return;
        }
        self.regs.write(off, width, value, cx);
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::RNG.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<lp_emu_esp_common::periph::RegGrade> {
        if off & !3 == DATA {
            // A seeded generator is a stand-in for hardware, said so.
            return Some(lp_emu_esp_common::periph::RegGrade::Modeled);
        }
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(LEN as usize + 8);
        out.extend_from_slice(&self.state.to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let Some(state) = bytes.get(..8) else {
            log::warn!("RNG: load_state blob too short, ignored");
            return;
        };
        self.state = u64::from_le_bytes(state.try_into().expect("8 bytes"));
        self.regs.load_state(&bytes[8..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn the_same_seed_is_the_same_sequence_and_a_new_seed_is_a_new_one() {
        let mut sb = Sandbox::new();
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let mut c = Rng::new(8);
        assert_eq!(a.reg_name(DATA), Some("data"));
        let xa: Vec<u32> = (0..4).map(|_| sb.read(&mut a, DATA)).collect();
        let xb: Vec<u32> = (0..4).map(|_| sb.read(&mut b, DATA)).collect();
        let xc: Vec<u32> = (0..4).map(|_| sb.read(&mut c, DATA)).collect();
        assert_eq!(xa, xb);
        assert_ne!(xa, xc);
        assert!(xa.windows(2).all(|w| w[0] != w[1]), "it advances");
        // Writes to a read-only register change nothing.
        sb.write(&mut a, DATA, 0);
        let blob = a.save_state();
        let mut back = Rng::new(0);
        back.load_state(&blob);
        assert_eq!(sb.read(&mut back, DATA), sb.read(&mut a, DATA));
    }
}
