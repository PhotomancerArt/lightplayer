//! `RNG` at `0x600B_2800` (the `LP_PERI` base; the PAC names `+0x08`
//! `rng_data`) — a deterministic random source.
//!
//! esp-hal reads `RNG.data` (`rng/ll.rs:59-73`) spaced by the CPU cycle
//! counter; the C6's `Trng` path that stirs the SAR ADC into it is never
//! taken by the shipped firmware (discovery §4). Every read of `+0x08`
//! advances an xorshift64\* generator seeded from the machine's `--seed`, so
//! a run is the same run (plan PD5) and a different seed is a different one.
//! Silicon's RNG is not reproducible; this one is, on purpose, and says so.
//!
//! Every other offset is accept-and-remember with `LP_PERI`'s names.

use lp_emu_esp_common::regfile::lane_of;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use super::systimer::Reader;
use crate::regs;

pub const DATA: u32 = 0x08;

/// The RNG block.
#[derive(Debug)]
pub struct Rng {
    regs: RegFile,
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            regs: RegFile::new("RNG", 0x400).with_names(regs::LP_PERI),
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
        regs::LP_PERI.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x400 + 8);
        out.extend_from_slice(&self.state.to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let mut r = Reader(bytes);
        let Some(state) = r.u64() else {
            log::warn!("RNG: load_state blob too short, ignored");
            return;
        };
        self.state = state;
        self.regs.load_state(r.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn every_read_advances_and_the_same_seed_is_the_same_sequence() {
        let mut sb = Sandbox::new();
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let mut c = Rng::new(8);
        let sa: Vec<u32> = (0..4).map(|_| sb.read(&mut a, DATA)).collect();
        let sb_: Vec<u32> = (0..4).map(|_| sb.read(&mut b, DATA)).collect();
        let sc: Vec<u32> = (0..4).map(|_| sb.read(&mut c, DATA)).collect();
        assert_eq!(sa, sb_);
        assert_ne!(sa, sc);
        assert!(sa.windows(2).all(|w| w[0] != w[1]), "advances: {sa:?}");
        assert_eq!(a.reg_name(DATA), Some("rng_data"));
    }

    #[test]
    fn a_zero_seed_is_still_a_live_generator_and_the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut z = Rng::new(0);
        let first = sb.read(&mut z, DATA);
        assert_ne!(first, 0);
        let blob = z.save_state();
        let mut w = Rng::new(0);
        w.load_state(&blob);
        assert_eq!(sb.read(&mut w, DATA), sb.read(&mut z, DATA));
    }
}
