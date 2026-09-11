//! `I2C_ANA_MST` at `0x6000_E000` — the analog I2C master, modelled as the
//! **register file the guest is talking through it to**.
//!
//! On the **AHB bus** ([`crate::memmap::MMIO_AHB_BASE`]), which is where P3
//! found it: the fifth strict stop of the direct load was
//! `rom_chip_i2c_writeReg+0x2f` writing `0x6000_E010`, outside every window
//! this machine declared at the time.
//!
//! # The transaction, from the mask ROM's own code
//!
//! The PAC has no block at this address, so the layout is read out of the
//! ROM (`docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md` §3.2):
//!
//! ```text
//! 40004168 <rom_chip_i2c_writeReg>:          (block, host_id, reg, data)
//! 4000416b:  l32r  a9, (0x01000000)          ; bit 24: write
//! 40004177:  l32r  a9, (0x18003800)
//! 40004183:  add.n a9, a3, a9                ; + host_id
//! 4000418b:  slli  a9, a9, 2                 ; ×4 → 0x6000E000 + 4·host_id
//! 40004180:  slli  a4, a4, 8                 ; reg  << 8
//! 40004188:  slli  a8, a5, 16                ; data << 16
//! 40004197:  s32i.n a2, a9, 0                ; write the command word
//! 4000419c:  l32i.n a8, a9, 0
//! 4000419e:  bany  a8, a10(0x02000000), -5   ; spin while bit 25 (busy)
//!
//! 40004110 <rom_chip_i2c_readReg>:  same word, no bit 24; after the spin,
//! 40004141:  extui a2, a2, 16, 8             ; data = bits 23:16
//! ```
//!
//! One command word per `host_id`: `[7:0]` slave address, `[15:8]` register,
//! `[23:16]` data, `[24]` write, `[25]` busy.
//!
//! **The classic's port is not the C6's.** The C6 has two `i2c_ctrl`
//! registers and reaches every analog block through either of them; the
//! classic has **eight** command words, one per `host_id`, and the BBPLL is
//! host 4. The `{slave_addr, slave_reg_addr}` pair inside the word is the
//! same idea on both parts, which is why both views keep the same store.
//!
//! # Why a store and not an accept block
//!
//! `slave_addr` is a *block* — the BBPLL, the bias generator, the digital
//! regulator, the SAR ADC, the RF PLL — and `slave_reg_addr` a register
//! inside it: two hundred distinct analog registers behind eight words. An
//! accept block answers a read of **any** of them with the last byte written
//! to **any other**, which on the C6 was a real defect with a real symptom
//! (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`:
//! three `error: pll_cal exceeds 2ms!!!` lines silicon does not print).
//!
//! P3 recorded that the shipped image only ever *writes* through this master
//! (`clocks.rs:222-264` is all `write_reg`), so the accept block was not
//! wrong for the direct load — it was wrong *in waiting*, and the ROM-up
//! path P5 opens is exactly the path that reads back.
//!
//! So: a write with bit 24 set stores the byte at `{block, register}`; a
//! write with it clear looks the pair up and leaves the answer in the word's
//! `data` field for the guest's next read. A pair nobody has written answers
//! **0** — the same answer an accept block gave, but only for the register
//! actually asked about.
//!
//! # `busy` reads 0, and there is no seed
//!
//! Bit 25 is the ROM's spin (`4000419e: bany`), the guest never writes it,
//! and a transaction with no duration in this model has always finished.
//!
//! The C6's view carries one `ANALOG_SEED` entry — the RF PLL's
//! calibration-end flag, because its ROM's `wait_rfpll_cal_end` polls for a
//! bit nothing in the model can ever set. **This block carries none**: no
//! run on this chip has met such a poll yet, and a seed is a claim about
//! silicon that needs a spin to justify it. If the ROM-up boot P7 takes
//! further meets one, the entry belongs there with the disassembly line that
//! found it.

use std::collections::BTreeMap;

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regnames::RegNames;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

/// How many command/status words the block has, one per `host_id`
/// (`0x6000_E000 + 4·host_id`; the BBPLL is host 4, and the highest host
/// esp-idf names for this chip is 7).
///
/// Only these eight run a transaction. A ninth *host* is still a strict
/// stop rather than a silent zero: the aperture below is larger than the
/// host words, and everything past them is accept-and-remember.
pub const I2C_ANA_MST_HOSTS: u32 = 8;

/// The block's aperture.
///
/// P5 had this tight at `0x20` — the eight host words and nothing else —
/// with a note that "the ROM's other literals in the block (`+0x50`,
/// `+0x5c`, `+0x80`, the PHY's `ANA_CONFIG` words, P3's §4.3) are outside it
/// until a boot reaches them". **P7's ROM-up boot reaches them**: the
/// ESP-IDF second-stage bootloader's clock bring-up reads `+0x44` at cycle
/// 2,360,588, eleven lines into the log and one instruction into its own
/// `.text` at `0x4008_10BA`.
///
/// `0x6000_E044` and `0x6000_E048` are `ANA_CONFIG_REG` and
/// `ANA_CONFIG2_REG` in ESP-IDF's `soc/esp32/include/soc/rtc.h` — the words
/// that gate which analog blocks the I2C master may reach. Nothing in this
/// machine acts on them: they are **accept-and-remember**, because the
/// analog store behind the host words answers whatever was written to it
/// regardless, and a model that gated it would be inventing a gate nobody
/// measured. The aperture is `0x90` so `+0x80` (the last literal P3's §4.3
/// found in the ROM's `.text`) is inside it too.
pub const I2C_ANA_MST_LEN: u32 = 0x90;

/// `ANA_CONFIG_REG` / `ANA_CONFIG2_REG`.
pub const ANA_CONFIG: u32 = 0x044;
pub const ANA_CONFIG2: u32 = 0x048;

/// The analog I2C master's register names. **Hand-written**, because the
/// `esp32` PAC has no block at this address; the layout is the mask ROM's
/// (see the module docs).
pub static I2C_ANA_MST_NAMES: RegNames = RegNames {
    block: "i2c_ana_mst",
    entries: &[
        (0x000, "host0"),
        (0x004, "host1"),
        (0x008, "host2"),
        (0x00c, "host3"),
        (0x010, "host4_bbpll"),
        (0x014, "host5"),
        (0x018, "host6"),
        (0x01c, "host7"),
        (0x044, "ana_config"),
        (0x048, "ana_config2"),
    ],
    resets: &[],
    access: &[],
};

const SLAVE_ADDR: u32 = 0xff;
const SLAVE_REG_ADDR_SHIFT: u32 = 8;
const DATA_SHIFT: u32 = 16;
const DATA_MASK: u32 = 0xff << DATA_SHIFT;
const READ_WRITE: u32 = 1 << 24;
const BUSY: u32 = 1 << 25;

/// The analog registers this machine answers with something other than 0
/// before anyone writes them, and the evidence for each.
///
/// `(block, register, value, why)`. **Empty**, deliberately: see the module
/// docs. The C6's equivalent has one entry and a ROM disassembly line behind
/// it; an entry here without one would be an invention.
pub const ANALOG_SEED: &[(u8, u8, u8, &str)] = &[];

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
        let mut regs = RegFile::new("I2C_ANA_MST", I2C_ANA_MST_LEN).with_names(I2C_ANA_MST_NAMES);
        for host in 0..I2C_ANA_MST_HOSTS {
            // `busy` is the ROM's spin and the guest never sets it.
            regs.set_read_override(4 * host, BUSY, 0);
        }
        let regs = (0..I2C_ANA_MST_LEN / 4).fold(regs.with_pac_grades(), |rf, i| {
            // `with_pac_grades` calls a register with no access entry
            // read-write and therefore *documented*; nothing documents these,
            // so every word is demoted by hand to what it is.
            rf.with_grade(4 * i, RegGrade::Modeled)
        });
        Self { regs, analog }
    }

    /// The byte at `{block, register}`, or 0 where nothing has been written.
    pub fn analog(&self, block: u8, reg: u8) -> u8 {
        self.analog.get(&(block, reg)).copied().unwrap_or(0)
    }

    /// Run the transaction a write to a host word just asked for, and leave
    /// the `data` field holding what the guest will read back.
    fn transact(&mut self, host: u32) {
        let word = self.regs.stored(host);
        let block = (word & SLAVE_ADDR) as u8;
        let reg = ((word >> SLAVE_REG_ADDR_SHIFT) & 0xff) as u8;
        let data = ((word >> DATA_SHIFT) & 0xff) as u8;
        if word & READ_WRITE != 0 {
            self.analog.insert((block, reg), data);
            return;
        }
        let answer = self.analog(block, reg);
        self.regs.poke(
            host,
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
        //
        // Only the eight host words are transactions. Everything above them
        // in the aperture — `ana_config`, `ana_config2`, and the unnamed
        // words up to `+0x8c` — is remembered and does nothing.
        let word = off & !3;
        if word < I2C_ANA_MST_HOSTS * 4 {
            self.transact(word);
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        I2C_ANA_MST_NAMES.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
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
        let window = I2C_ANA_MST_LEN as usize;
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

    /// The BBPLL's port: `0x6000_E000 + 4·4`.
    const HOST4: u32 = 0x010;

    fn read_request(block: u8, reg: u8) -> u32 {
        u32::from(block) | (u32::from(reg) << SLAVE_REG_ADDR_SHIFT)
    }

    fn write_request(block: u8, reg: u8, data: u8) -> u32 {
        read_request(block, reg) | READ_WRITE | (u32::from(data) << DATA_SHIFT)
    }

    /// `rom_chip_i2c_writeReg(block, host, reg, data)`.
    fn rom_write(sb: &mut Sandbox, m: &mut I2cAnaMst, host: u32, block: u8, reg: u8, data: u8) {
        sb.write(m, host, write_request(block, reg, data));
        assert_eq!(sb.read(m, host) & BUSY, 0, "the ROM's spin ends at once");
    }

    /// `rom_chip_i2c_readReg(block, host, reg)`.
    fn rom_read(sb: &mut Sandbox, m: &mut I2cAnaMst, host: u32, block: u8, reg: u8) -> u8 {
        sb.write(m, host, read_request(block, reg));
        assert_eq!(sb.read(m, host) & BUSY, 0);
        ((sb.read(m, host) >> DATA_SHIFT) & 0xff) as u8
    }

    #[test]
    fn a_read_answers_the_register_it_asked_for_and_not_the_last_one_written() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        assert_eq!(m.reg_name(HOST4), Some("host4_bbpll"));
        rom_write(&mut sb, &mut m, HOST4, 0x66, 3, 0xaa);
        rom_write(&mut sb, &mut m, HOST4, 0x66, 4, 0xbb);
        rom_write(&mut sb, &mut m, HOST4, 0x6a, 3, 0xcc);
        assert_eq!(rom_read(&mut sb, &mut m, HOST4, 0x66, 3), 0xaa);
        assert_eq!(rom_read(&mut sb, &mut m, HOST4, 0x66, 4), 0xbb);
        assert_eq!(rom_read(&mut sb, &mut m, HOST4, 0x6a, 3), 0xcc);
        // The store's whole point: a pair nobody has touched is 0, whatever
        // was written to its neighbours.
        assert_eq!(rom_read(&mut sb, &mut m, HOST4, 0x66, 5), 0);
        assert_eq!(rom_read(&mut sb, &mut m, HOST4, 0x6d, 3), 0);
    }

    /// What the accept block did, stated as the thing that no longer
    /// happens: `data` used to read back the last byte written to the host
    /// word, whatever analog register it belonged to.
    #[test]
    fn the_one_shared_data_byte_is_gone() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        rom_write(&mut sb, &mut m, HOST4, 0x66, 3, 0xaa);
        assert_ne!(
            rom_read(&mut sb, &mut m, HOST4, 0x69, 0),
            0xaa,
            "a SAR ADC register answering with a BBPLL byte"
        );
    }

    /// The eight ports are eight doors onto one analog world — the same
    /// store, whichever `host_id` a caller uses.
    #[test]
    fn every_host_port_reaches_the_same_store() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        rom_write(&mut sb, &mut m, 0x000, 0x66, 8, 0x5a);
        assert_eq!(rom_read(&mut sb, &mut m, 0x01c, 0x66, 8), 0x5a);
        for host in 0..8u32 {
            assert_eq!(rom_read(&mut sb, &mut m, 4 * host, 0x66, 8), 0x5a);
        }
    }

    /// The word the direct load's fifth strict stop actually wrote
    /// (`clocks.rs` programming the BBPLL through `rom_i2c_writeReg`).
    #[test]
    fn the_busy_bit_answers_the_roms_spin_and_the_word_is_remembered() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        let word = (1 << 24) | (0x1c << 16) | (3 << 8) | 0x66;
        sb.write(&mut m, HOST4, word);
        assert_eq!(sb.read(&mut m, HOST4) & BUSY, 0, "bit 25 never sets");
        assert_eq!(sb.read(&mut m, HOST4), word);
        assert_eq!(m.analog(0x66, 3), 0x1c);
        // The PAC does not know this block: every word is `Modeled`.
        assert_eq!(m.reg_grade(HOST4), Some(RegGrade::Modeled));
    }

    #[test]
    fn the_seed_list_is_empty_and_any_entry_would_have_to_carry_a_reason() {
        for (block, reg, value, why) in ANALOG_SEED {
            assert_ne!(*value, 0, "a seed of 0 is not a seed ({block:#04x}/{reg})");
            assert!(why.len() > 40, "{block:#04x}/{reg} has no reason");
        }
    }

    #[test]
    fn the_analog_store_round_trips_through_save_state() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        rom_write(&mut sb, &mut m, HOST4, 0x6d, 13, 0x0c);
        rom_write(&mut sb, &mut m, 0x008, 0x61, 3, 0x09);
        let blob = m.save_state();
        let mut other = I2cAnaMst::new();
        other.load_state(&blob);
        assert_eq!(other.analog(0x6d, 13), 0x0c);
        assert_eq!(other.analog(0x61, 3), 0x09);
        assert_eq!(other.save_state(), blob);
    }
}
