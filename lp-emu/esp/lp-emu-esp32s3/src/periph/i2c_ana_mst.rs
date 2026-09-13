//! `I2C_ANA_MST` at `0x6000_E000` — the analog I2C master, modelled as the
//! **register file the guest is talking through it to**, plus the three
//! registers the PAC names at `+0x40..+0x48`.
//!
//! The classic's view (`lp-emu-esp32v3/src/periph/i2c_ana_mst.rs`), on this
//! chip's own ROM: the brief said to start with an accept block and promote
//! it only if a spin appeared, and one does before the first PLL is up —
//! esp-hal's `enable_pll_clk_impl` spins on **`ana_conf0.bbpll_cal_done`**
//! (§ below). The store rather than a plain accept block is the classic's
//! lesson carried over: an accept block answers a read of *any* analog
//! register with the last byte written to *any other*, which was a real
//! defect with a real symptom on the C6
//! (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`),
//! and on this chip esp-hal's `write_field` *does* read back — every
//! `I2C_DIG_REG_EXT_*_DREG.write_field` in `ensure_voltage_raised` is a
//! `rom_i2c_readReg` followed by a `rom_i2c_writeReg` (`rom_i2c_writeReg_Mask`,
//! `0x4003_58D8`).
//!
//! # The transaction, from this chip's mask ROM
//!
//! The command-word protocol is the classic's, one chip over
//! (`rom_chip_i2c_writeReg`, `0x4003_5818`):
//!
//! ```text
//! 4003583d:  l32r   a2, (0x18003800)
//! 40035840:  add.n  a10, a10, a2             ; + host_id
//! 40035845:  slli   a10, a10, 2              ; ×4 → 0x6000E000 + 4·host_id
//! 40035848:  l32i   a8, a10, 0
//! 4003584e:  bany   a8, a2(0x02000000), -3   ; spin while bit 25 (busy) — before
//! 40035851:  l32r   a8, (0x05000000)         ; bits 24 (write) and 26
//! 40035854:  slli   a4, a4, 8                ; reg  << 8
//! 4003585d:  slli   a5, a5, 16               ; data << 16
//! 40035866:  s32i   a2, a10, 0               ; write the command word
//! 4003586f:  l32i.n a3, a10, 0
//! 40035871:  bany   a3, a2(0x02000000), -3   ; spin while bit 25 (busy) — after
//! ```
//!
//! One command word per `host_id`: `[7:0]` slave address, `[15:8]`
//! register, `[23:16]` data, `[24]` write, `[25]` busy, and — new on this
//! chip — **`[26]` set on every write**, whose meaning the PAC does not say
//! and this model remembers without interpreting. A write with bit 24 set
//! stores the byte at `{block, register}`; a read (bit 24 clear) looks the
//! pair up and leaves the answer in the word's `data` field for the guest's
//! next read; a pair nobody has written answers **0**. `busy` reads 0: the
//! guest never sets it, and a transaction with no duration has always
//! finished.
//!
//! # The three PAC registers, and the one spin
//!
//! `esp32s3-0.35.2/src/lib.rs:812` places the PAC's `I2C_ANA_MST` at
//! `0x6000_E040` with `ana_conf0 +0x00`, `ana_config +0x04`, `ana_config2
//! +0x08` — i.e. `+0x40..+0x48` of this window. `ana_config.bbpll_pd` (bit
//! 17, "Clear to enable BBPLL") and `ana_conf0.bbpll_stop_force_high/low`
//! (bits 2/3) are written by `enable_pll_clk_impl`
//! (`soc/esp32s3/clocks.rs:143-146, 226-247`); and then it waits:
//!
//! ```text
//! // WAIT CALIBRATION DONE
//! while I2C_ANA_MST::regs().ana_conf0().read().bbpll_cal_done().bit_is_clear() {}
//! ```
//!
//! (`clocks.rs:232-238`). `bbpll_cal_done` is **bit 24** of `ana_conf0`
//! (`esp32s3-0.35.2/src/i2c_ana_mst/ana_conf0.rs:30`), the PAC gives the
//! register no reset, and a block that answered 0 would spin there
//! forever. It reads **1** here — *modeled*: the calibration a PLL with no
//! analog behind it would have finished — and it is the block's one read
//! override, listed in [`READ_OVERRIDES`].
//!
//! # No seed
//!
//! The C6's view carries one `ANALOG_SEED` entry for its ROM's
//! `wait_rfpll_cal_end`; the classic's carries none. **This block carries
//! none**: no run on this chip has met a poll on an analog register yet,
//! and a seed is a claim about silicon that needs a spin to justify it.

use std::collections::BTreeMap;

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regnames::RegNames;
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

/// How many command/status words the block has, one per `host_id`
/// (`0x6000_E000 + 4·host_id`).
pub const I2C_ANA_MST_HOSTS: u32 = 8;

/// The block's aperture, tight: the eight host words, the gap, and the
/// three PAC registers, the last at `+0x48`.
pub const I2C_ANA_MST_LEN: u32 = 0x4c;

/// Where the PAC's block starts inside this window.
pub const PAC_BASE: u32 = 0x040;
/// `ana_conf0` — the BBPLL calibration control and its `cal_done`.
pub const ANA_CONF0: u32 = PAC_BASE;
/// `ana_config` — `bbpll_pd` bit 17.
pub const ANA_CONFIG: u32 = PAC_BASE + 0x04;
/// `ana_config2`.
pub const ANA_CONFIG2: u32 = PAC_BASE + 0x08;

/// `ana_conf0.bbpll_cal_done`, bit 24.
pub const BBPLL_CAL_DONE: u32 = 1 << 24;

/// The block's register names: the eight host words are the ROM's
/// (hand-named, since no PAC names them), the three above are the PAC's,
/// re-based from `0x6000_E040` to this window.
pub static I2C_ANA_MST_NAMES: RegNames = RegNames {
    block: "i2c_ana_mst",
    entries: &[
        (0x000, "host0"),
        (0x004, "host1"),
        (0x008, "host2"),
        (0x00c, "host3"),
        (0x010, "host4"),
        (0x014, "host5"),
        (0x018, "host6"),
        (0x01c, "host7"),
        (0x040, "ana_conf0"),
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

/// Every bit this block answers with something other than what was
/// written, with its evidence. `(offset, mask, value, why)`.
pub const READ_OVERRIDES: &[(u32, u32, u32, &str)] = &[(
    ANA_CONF0,
    BBPLL_CAL_DONE,
    BBPLL_CAL_DONE,
    "ana_conf0.bbpll_cal_done (bit 24, esp32s3-0.35.2/src/i2c_ana_mst/ana_conf0.rs:30): \
         esp-hal's enable_pll_clk_impl spins `while … bbpll_cal_done().bit_is_clear() {}` \
         (soc/esp32s3/clocks.rs:232-238) after starting the BBPLL self-calibration; the PAC \
         gives the register no reset. Modeled: a PLL with no analog behind it has finished",
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
        let mut regs = RegFile::new("I2C_ANA_MST", I2C_ANA_MST_LEN).with_names(I2C_ANA_MST_NAMES);
        for host in 0..I2C_ANA_MST_HOSTS {
            // `busy` is the ROM's spin and the guest never sets it.
            regs.set_read_override(4 * host, BUSY, 0);
        }
        for (off, mask, value, _) in READ_OVERRIDES {
            regs.set_read_override(*off, *mask, *value);
        }
        let regs = (0..I2C_ANA_MST_LEN / 4).fold(regs.with_pac_grades(), |rf, i| {
            // `with_pac_grades` calls a register with no access entry
            // read-write and therefore *documented*; the host words are the
            // ROM's and the PAC registers are pretended about, so every word
            // is what it is: modeled.
            rf.with_grade(4 * i, RegGrade::Modeled)
        });
        Self {
            regs,
            analog: BTreeMap::new(),
        }
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
        // Only the eight host words are transactions. The gap and the three
        // PAC registers are remembered and do nothing.
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
        let window = self.regs.len_bytes() as usize;
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
    use crate::regs;
    use lp_emu_esp_common::Sandbox;

    /// `REGI2C_BBPLL` is master `0x66` on host 1 (`soc/esp32s3/regi2c.rs:4`).
    const HOST1: u32 = 0x004;
    const BBPLL: u8 = 0x66;

    fn read_request(block: u8, reg: u8) -> u32 {
        u32::from(block) | (u32::from(reg) << SLAVE_REG_ADDR_SHIFT)
    }

    /// The ROM's write word: bits 24 and 26 (`l32r a8, (0x05000000)`).
    fn write_request(block: u8, reg: u8, data: u8) -> u32 {
        read_request(block, reg) | 0x0500_0000 | (u32::from(data) << DATA_SHIFT)
    }

    fn rom_write(sb: &mut Sandbox, m: &mut I2cAnaMst, host: u32, block: u8, reg: u8, data: u8) {
        assert_eq!(
            sb.read(m, host) & BUSY,
            0,
            "the ROM's spin before the write"
        );
        sb.write(m, host, write_request(block, reg, data));
        assert_eq!(sb.read(m, host) & BUSY, 0, "and after");
    }

    fn rom_read(sb: &mut Sandbox, m: &mut I2cAnaMst, host: u32, block: u8, reg: u8) -> u8 {
        sb.write(m, host, read_request(block, reg));
        assert_eq!(sb.read(m, host) & BUSY, 0);
        ((sb.read(m, host) >> DATA_SHIFT) & 0xff) as u8
    }

    #[test]
    fn a_read_answers_the_register_it_asked_for_and_not_the_last_one_written() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        assert_eq!(m.reg_name(HOST1), Some("host1"));
        rom_write(&mut sb, &mut m, HOST1, BBPLL, 3, 0xaa);
        rom_write(&mut sb, &mut m, HOST1, BBPLL, 4, 0x6b);
        rom_write(&mut sb, &mut m, HOST1, 0x6d, 3, 0xcc);
        assert_eq!(rom_read(&mut sb, &mut m, HOST1, BBPLL, 3), 0xaa);
        assert_eq!(rom_read(&mut sb, &mut m, HOST1, BBPLL, 4), 0x6b);
        assert_eq!(rom_read(&mut sb, &mut m, HOST1, 0x6d, 3), 0xcc);
        assert_eq!(rom_read(&mut sb, &mut m, HOST1, BBPLL, 5), 0);
        assert_eq!(m.analog(BBPLL, 4), 0x6b);
        // Bit 26 is remembered on the word, not interpreted.
        rom_write(&mut sb, &mut m, HOST1, BBPLL, 6, 0x11);
        assert_ne!(sb.read(&mut m, HOST1) & (1 << 26), 0);
    }

    /// The spin `enable_pll_clk_impl` ends on, and the writes around it.
    #[test]
    fn bbpll_cal_done_reads_set_and_the_pac_registers_remember() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        assert_eq!(m.reg_name(ANA_CONF0), Some("ana_conf0"));
        assert_ne!(sb.read(&mut m, ANA_CONF0) & BBPLL_CAL_DONE, 0);
        // Start calibration: force_high clear, force_low set.
        let v = sb.read(&mut m, ANA_CONF0);
        sb.write(&mut m, ANA_CONF0, (v & !(1 << 2)) | (1 << 3));
        assert_eq!(sb.read(&mut m, ANA_CONF0), (1 << 3) | BBPLL_CAL_DONE);
        // `bbpll_pd` clear to enable.
        sb.write(&mut m, ANA_CONFIG, 0);
        assert_eq!(sb.read(&mut m, ANA_CONFIG), 0);
        assert_eq!(m.reg_grade(ANA_CONF0), Some(RegGrade::Modeled));
        assert_eq!(m.reg_grade(0), Some(RegGrade::Modeled));
    }

    /// The PAC's three names, re-based: the table above agrees with the
    /// generated one at `+0x40`.
    #[test]
    fn the_pac_registers_are_the_generated_tables_at_plus_0x40() {
        for (off, name) in regs::I2C_ANA_MST.entries {
            assert_eq!(I2C_ANA_MST_NAMES.name(PAC_BASE + off), Some(*name));
        }
        assert_eq!(regs::I2C_ANA_MST.entries.len(), 3);
    }

    #[test]
    fn the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut m = I2cAnaMst::new();
        rom_write(&mut sb, &mut m, HOST1, BBPLL, 3, 0xaa);
        sb.write(&mut m, ANA_CONFIG, 0x2_0000);
        let blob = m.save_state();
        let mut other = I2cAnaMst::new();
        other.load_state(&blob);
        assert_eq!(other.save_state(), blob);
        assert_eq!(rom_read(&mut sb, &mut other, HOST1, BBPLL, 3), 0xaa);
        assert_eq!(sb.read(&mut other, ANA_CONFIG), 0x2_0000);
    }
}
