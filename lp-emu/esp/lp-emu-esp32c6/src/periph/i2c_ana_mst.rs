//! `I2C_ANA_MST` at `0x600A_F800` — the analog I2C master, modelled as the
//! **register file the guest is talking through it to**.
//!
//! The block is a two-register transaction port onto the chip's analog
//! world: a driver writes `i2c_ctrl(n)` with a `{slave_addr, slave_reg_addr}`
//! pair and reads the `data` field back. `slave_addr` is a *block* — the
//! BBPLL, the bias generator, the digital regulator, the SAR ADC, the RF PLL
//! — and `slave_reg_addr` a register inside it. Two hundred distinct analog
//! registers are reachable through those eight bits.
//!
//! Until 2026-09-08 this was an accept-and-remember [`RegFile`], which meant
//! `data` read back whatever was last written to `i2c_ctrl` — **one byte of
//! state for the whole analog address space**. A read of any analog register
//! answered with the last value written to any other. The direct-load boot
//! happened not to trip on it; the ROM-up boot, whose mask ROM and
//! bootloader have run their own `regi2c` traffic first, printed three
//! `error: pll_cal exceeds 2ms!!!` lines that silicon does not
//! (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`).
//!
//! # The transaction
//!
//! `i2c_ctrl(n)` (`+0x00` for master 0, `+0x04` for master 1), from the PAC's
//! own field descriptions:
//!
//! ```text
//! bits  0:7   slave_addr        the analog block
//! bits  8:15  slave_reg_addr    the register inside it
//! bits 16:23  data              written on a write, answered on a read
//! bit  24     read_write        1 = write, 0 = read
//! bit  25     busy              transfer in flight — always 0 here
//! ```
//!
//! Both sides of the guest agree on that shape. esp-hal's `regi2c_read`
//! writes `slave_addr`/`slave_reg_addr` with `read_write` clear and then
//! reads `data().bits()` (`soc/esp32c6/regi2c.rs:183-198`); the mask ROM's
//! `i2c_paral_read` (`0x40003e9c`) stores `reg << 8 | block` to **both**
//! masters, spins on `busy`, and takes `>> 16` of each; `i2c_paral_write`
//! (`0x40003ef2`) ORs in `data << 16` and the write bit.
//!
//! So: a write with `read_write` set stores the byte at `{block, register}`,
//! and a write with it clear looks the pair up and puts the answer in the
//! `data` field for the guest's next read. That is the whole model. The
//! store starts empty except for [`ANALOG_SEED`], and a read of a pair
//! nothing has written answers **0** — the same answer an accept block gave,
//! but now only for the register actually asked about.
//!
//! # `busy` and `cal_done`
//!
//! Two read overrides survive from the accept block, for the same reasons:
//!
//! - `i2c_ctrl(n).busy` (bit 25) reads **0**: every driver spins on it
//!   (esp-hal `regi2c.rs:187, 194, 209`, and both ROM paths). A transaction
//!   with no duration in this model has always finished.
//! - `ana_conf0.cal_done` (`+0x18` bit 24) reads **1**: the BBPLL
//!   calibration wait at `soc/esp32c6/clocks.rs:181-186`.
//!
//! Everything else in the window is accept-and-remember with the PAC's names
//! and reset values.

use std::collections::BTreeMap;

use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs;

/// `i2c_ctrl(0)` and `i2c_ctrl(1)` — the two transaction ports.
const I2C0_CTRL: u32 = 0x000;
const I2C1_CTRL: u32 = 0x004;
/// `ana_conf0`, whose `cal_done` the BBPLL wait spins on.
const ANA_CONF0: u32 = 0x018;

const SLAVE_ADDR: u32 = 0xff;
const SLAVE_REG_ADDR_SHIFT: u32 = 8;
const DATA_SHIFT: u32 = 16;
const DATA_MASK: u32 = 0xff << DATA_SHIFT;
const READ_WRITE: u32 = 1 << 24;
const BUSY: u32 = 1 << 25;
const CAL_DONE: u32 = 1 << 24;

/// The analog registers this machine answers with something other than 0
/// before anyone writes them, and the evidence for each.
///
/// `(block, register, value, why)`. Deliberately tiny: an analog register
/// whose value nobody has needed is one nobody has evidence about, and 0 is
/// the honest answer for it.
pub const ANALOG_SEED: &[(u8, u8, u8, &str)] = &[(
    0x62,
    7,
    0b10,
    "the RF PLL's calibration-end flag. The mask ROM's `wait_rfpll_cal_end` \
     (0x40005984) calls `rom_i2c_readReg_Mask(0x62, 1, 7, 1, 1)` sixty-four \
     times, 20 us apart, and prints `error: pll_cal exceeds 2ms!!!` on the \
     hundredth try; silicon prints it never, so on silicon the bit is set by \
     the time the ROM looks. Nothing in this machine can calibrate a PLL, so \
     the flag is where a calibrated one leaves it.",
)];

/// The analog I2C master.
#[derive(Debug)]
pub struct I2cAnaMst {
    regs: RegFile,
    /// `{slave_addr, slave_reg_addr}` → the byte last written there.
    /// A `BTreeMap` so `save_state` is a run-to-run identical blob.
    analog: BTreeMap<(u8, u8), u8>,
}

impl Default for I2cAnaMst {
    fn default() -> Self {
        Self::new()
    }
}

impl I2cAnaMst {
    pub fn new() -> Self {
        let mut analog = BTreeMap::new();
        for (block, reg, value, _) in ANALOG_SEED {
            analog.insert((*block, *reg), *value);
        }
        Self {
            regs: RegFile::new("I2C_ANA_MST", 0x100)
                .with_names(regs::I2C_ANA_MST)
                .with_read_override(I2C0_CTRL, BUSY, 0)
                .with_read_override(I2C1_CTRL, BUSY, 0)
                .with_read_override(ANA_CONF0, CAL_DONE, CAL_DONE),
            analog,
        }
    }

    /// The byte at `{block, register}`, or 0 where nothing has been written.
    pub fn analog(&self, block: u8, reg: u8) -> u8 {
        self.analog.get(&(block, reg)).copied().unwrap_or(0)
    }

    /// Run the transaction a write to `i2c_ctrl(n)` just asked for, and
    /// leave the `data` field holding what the guest will read back.
    fn transact(&mut self, ctrl: u32) {
        let word = self.regs.stored(ctrl);
        let block = (word & SLAVE_ADDR) as u8;
        let reg = ((word >> SLAVE_REG_ADDR_SHIFT) & 0xff) as u8;
        let data = ((word >> DATA_SHIFT) & 0xff) as u8;
        if word & READ_WRITE != 0 {
            self.analog.insert((block, reg), data);
            return;
        }
        let answer = self.analog(block, reg);
        self.regs.poke(
            ctrl,
            (word & !DATA_MASK) | (u32::from(answer) << DATA_SHIFT),
        );
    }
}

impl Peripheral for I2cAnaMst {
    fn name(&self) -> &'static str {
        "I2C_ANA_MST"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        self.regs.write(off, width, value, cx);
        // The transaction runs off the *stored* word, so a byte-lane write
        // that completes a `{block, register}` pair works the way a word
        // write does. No driver on this chip writes it in lanes; nothing
        // here has to care that they might.
        let word = off & !3;
        if word == I2C0_CTRL || word == I2C1_CTRL {
            self.transact(word);
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::I2C_ANA_MST.name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = self.regs.save_state();
        out.extend_from_slice(&(self.analog.len() as u32).to_le_bytes());
        for ((block, reg), value) in &self.analog {
            out.extend_from_slice(&[*block, *reg, *value]);
        }
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let window = 0x100;
        if bytes.len() < window + 4 {
            log::warn!("I2C_ANA_MST: load_state blob too short, ignored");
            return;
        }
        self.regs.load_state(&bytes[..window]);
        let count = u32::from_le_bytes(
            bytes[window..window + 4]
                .try_into()
                .expect("four bytes at the count"),
        ) as usize;
        let rows = &bytes[window + 4..];
        if rows.len() != count * 3 {
            log::warn!(
                "I2C_ANA_MST: load_state has {count} pairs but {} bytes",
                rows.len()
            );
            return;
        }
        self.analog.clear();
        for row in rows.chunks_exact(3) {
            self.analog.insert((row[0], row[1]), row[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// A word for `i2c_ctrl`, the way `regi2c_read` builds one.
    fn read_request(block: u8, reg: u8) -> u32 {
        u32::from(block) | (u32::from(reg) << SLAVE_REG_ADDR_SHIFT)
    }

    /// A word for `i2c_ctrl`, the way `regi2c_write` builds one.
    fn write_request(block: u8, reg: u8, data: u8) -> u32 {
        read_request(block, reg) | READ_WRITE | (u32::from(data) << DATA_SHIFT)
    }

    /// The transaction, both ways, on the port esp-hal picks.
    fn regi2c_write(sb: &mut Sandbox, m: &mut I2cAnaMst, block: u8, reg: u8, data: u8) {
        sb.write(m, I2C1_CTRL, write_request(block, reg, data));
    }

    fn regi2c_read(sb: &mut Sandbox, m: &mut I2cAnaMst, block: u8, reg: u8) -> u8 {
        sb.write(m, I2C1_CTRL, read_request(block, reg));
        ((sb.read(m, I2C1_CTRL) >> DATA_SHIFT) & 0xff) as u8
    }

    #[test]
    fn a_read_answers_the_register_it_asked_for_and_not_the_last_one_written() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        regi2c_write(&mut sb, &mut m, 0x66, 3, 0xaa);
        regi2c_write(&mut sb, &mut m, 0x66, 4, 0xbb);
        regi2c_write(&mut sb, &mut m, 0x6a, 3, 0xcc);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x66, 3), 0xaa);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x66, 4), 0xbb);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x6a, 3), 0xcc);
        // The register file's whole point: a block/register pair nobody has
        // touched is 0, whatever was written to its neighbours.
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x66, 5), 0);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x6d, 3), 0);
    }

    /// What the accept block did, stated as the thing that no longer
    /// happens: `data` used to read back the last byte written to
    /// `i2c_ctrl`, whatever register it belonged to.
    #[test]
    fn the_one_shared_data_byte_is_gone() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        regi2c_write(&mut sb, &mut m, 0x66, 3, 0xaa);
        assert_ne!(
            regi2c_read(&mut sb, &mut m, 0x69, 0),
            0xaa,
            "a SAR ADC register answering with a BBPLL byte"
        );
    }

    #[test]
    fn the_rf_plls_calibration_flag_is_set_before_anyone_writes_it() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        // `rom_i2c_readReg_Mask(0x62, 1, 7, 1, 1)`: bit 1 of register 7.
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x62, 7) & 0b10, 0b10);
        assert_eq!(m.analog(0x62, 7), 0b10);
        // And it is still a register: writing it clears it.
        regi2c_write(&mut sb, &mut m, 0x62, 7, 0);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x62, 7) & 0b10, 0);
    }

    #[test]
    fn every_seed_carries_its_reason() {
        for (block, reg, value, why) in ANALOG_SEED {
            assert_ne!(*value, 0, "a seed of 0 is not a seed ({block:#04x}/{reg})");
            assert!(why.len() > 40, "{block:#04x}/{reg} has no reason");
        }
    }

    #[test]
    fn both_masters_are_the_same_register_file() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        // The ROM's `i2c_paral_read` drives both ports; either must answer
        // the same store.
        sb.write(&mut m, I2C0_CTRL, write_request(0x66, 8, 0x5a));
        sb.write(&mut m, I2C1_CTRL, read_request(0x66, 8));
        assert_eq!((sb.read(&mut m, I2C1_CTRL) >> DATA_SHIFT) & 0xff, 0x5a);
    }

    #[test]
    fn the_spin_bits_still_read_the_way_the_drivers_need() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        sb.write(&mut m, I2C0_CTRL, 0xffff_ffff);
        sb.write(&mut m, I2C1_CTRL, 0xffff_ffff);
        assert_eq!(sb.read(&mut m, I2C0_CTRL) & BUSY, 0, "i2c0 busy");
        assert_eq!(sb.read(&mut m, I2C1_CTRL) & BUSY, 0, "i2c1 busy");
        sb.write(&mut m, ANA_CONF0, 0);
        assert!(sb.read(&mut m, ANA_CONF0) & CAL_DONE != 0, "cal_done");
        assert_eq!(sb.read(&mut m, 0x020), 0, "ana_conf2 resets to 0: master 1");
        assert_eq!(m.reg_name(ANA_CONF0), Some("ana_conf0"));
    }

    #[test]
    fn the_analog_store_round_trips_through_save_state() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        regi2c_write(&mut sb, &mut m, 0x6d, 13, 0x0c);
        regi2c_write(&mut sb, &mut m, 0x61, 3, 0x09);
        let blob = m.save_state();
        let mut other = I2cAnaMst::new();
        other.load_state(&blob);
        assert_eq!(other.analog(0x6d, 13), 0x0c);
        assert_eq!(other.analog(0x61, 3), 0x09);
        assert_eq!(other.analog(0x62, 7), 0b10, "the seed survives too");
        assert_eq!(other.save_state(), blob);
    }
}
