//! `LP_I2C_ANA_MST` at `0x600B_2400` — the **LP** analog I2C master, the one
//! the ESP-IDF second-stage bootloader drives, modelled with the gate that
//! wedges it.
//!
//! This is the same kind of block as [`super::i2c_ana_mst`] (the HP aperture
//! at `0x600A_F800`) one power domain over: a transaction port onto the
//! chip's analog register space. It is modelled here rather than accepted
//! because of one measured behaviour the accept block could not have — it is
//! **clocked and reset from `LP_PERI`**, and a transaction started with its
//! clock gated latches `busy` for ever.
//!
//! # What the bench measured, and the hang it explains
//!
//! A factory ESP-IDF app gates the LP peripheral clocks it does not use,
//! `LPPERI_CLK_EN` bit 29 (`LP_ANA_I2C_CK_EN`) among them — an ESP-IDF app
//! drives the analog bus through the HP aperture and never misses it. Our
//! merged image's bootloader (espflash 3.3.0's bundled ESP-IDF
//! v5.1-beta1-378) drives it through **this** one. Its first `regi2c` write
//! in `rtc_clk_init` (slave `0x6d`, register `0x0e`) latches the master busy
//! with no clock to finish it, and the bootloader spins before
//! `bootloader_console_init()` — no output, a fixed `Saved PC`, and TG0's
//! flash-boot-protection watchdog (reset-armed, bootloader-disarmed — see
//! [`super::timg`]) as the only thing that moves. Disassembled from
//! the wedged board's own bootloader segment
//! (`docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md`):
//!
//! ```asm
//! 756: lui  a5, 0x600b2
//! 75a: sw   a6, 0x400(a5)    ; the command word into i2c0_ctrl
//! 75e: lui  a4, 0x600b2
//! 762: lui  a3, 0x2000       ; BIT(25)
//! 766: addi a0, a4, 0x400
//! 76a: lw   a5, 0x0(a0)      ; <-- Saved PC: re-read i2c0_ctrl
//! 76c: and  a5, a5, a3
//! 76e: bnez a5, 0x766        ; spin while BUSY is set
//! ```
//!
//! The wedged board read `i2c0_ctrl = 0x0200_0e6d`
//! (`lp_analog_i2c.rs::tests::STUCK_CTRL`): busy set over the command word
//! for `{block 0x6d, register 0x0e}`, a read. So the command word **stays
//! visible** in the register while busy is latched, and the three states
//! this block models are:
//!
//! | `clk_en` bit 29 | `reset_en` bit 29 | a write to `i2c0_ctrl` |
//! |---|---|---|
//! | 1 | 0 | the transaction runs; `busy` reads 0 — what every clean boot has always seen |
//! | 0 | 0 | the word is stored, the transaction does **not** run, and `busy` **latches**: bit 25 reads 1 on every subsequent read |
//! | — | 1 | the block is held in reset: the write is ignored |
//!
//! and the only two things that clear a latched busy are a **rising edge of
//! `reset_en` bit 29** — which also returns the window to its reset values —
//! and a power cycle. Turning the clock back on does not
//! (`lp_analog_i2c.rs`: "Setting the clock alone does NOT clear the latched
//! busy", bench-proven 2026-09-06 both directions on XIAO
//! `A0:F2:62:85:A8:7C`), which is why the flashers' cure is `clk_en |=
//! bit29`, `reset_en |= bit29`, `reset_en &= !bit29`.
//!
//! **`modeled`, and where**: the bench only ever pulsed the reset line with
//! the clock already restored, so "a pulse clears it" is asserted here
//! without that condition and the unclocked pulse is unmeasured. That the
//! whole window — not only `i2c0_ctrl` — returns to reset on the pulse, and
//! that a held-high reset line makes the block ignore writes, are readings
//! of what a reset line is, not measurements.
//!
//! # The transaction, and where the answer comes back
//!
//! Bits 0:24 of `i2c0_ctrl` are one opaque field in the PAC
//! (`lp_i2c_ana_mst/i2c0_ctrl.rs`: `LP_I2C_ANA_MAST_I2C0_CTRL`, bits 0:24,
//! with bit 25 `LP_I2C_ANA_MAST_I2C0_BUSY` above it), but `STUCK_CTRL` shows
//! the encoding is the HP master's: `slave_addr` low, `slave_reg_addr` at
//! bit 8, `data` at bit 16, `read_write` at bit 24. The one structural
//! difference from [`super::i2c_ana_mst`] is the **answer**: the HP master
//! puts it back in the `data` field of `i2c_ctrl`, while this block has a
//! register for it — `i2c0_data` (`+0x008`), whose bits 0:7 are
//! `LP_I2C_ANA_MAST_I2C0_RDATA` (bits 8:10 `i2c0_clk_sel`, bit 11
//! `i2c_mst_sel`; reset `0x0000_0900`). So a read transaction leaves the
//! byte in `i2c0_data.rdata`, and a write stores it.
//!
//! There is also no second port here: the PAC gives the LP block one
//! `i2c0_ctrl`, where the HP block has `i2c_ctrl(0)` and `i2c_ctrl(1)`.
//!
//! # The store
//!
//! `{slave_addr, slave_reg_addr}` → the byte last written there, exactly as
//! the HP master keeps it, and for the same reason
//! (`docs/defects/2026-09-08-regi2c-is-one-data-register-not-a-register-file.md`:
//! one shared `data` byte answers a read of any analog register with the
//! last value written to any other). The store is **this block's own**, not
//! shared with the HP master: the two apertures reach the same analog slaves
//! on silicon, but nothing here has measured that they do, and sharing would
//! let the bootloader's LP writes change what the mask ROM and esp-hal read
//! back through the HP aperture — a coupling with no evidence behind it.
//!
//! There is no seed list either. [`super::i2c_ana_mst::ANALOG_SEED`]'s one
//! entry exists because the mask ROM polls the RF PLL's calibration flag
//! through the **HP** aperture and prints an error when it reads 0; nothing
//! has been observed polling anything through this one, and 0 is the honest
//! answer for an analog register nobody has evidence about.
//!
//! Everything else in the window is accept-and-remember with the PAC's names
//! and reset values.
//!
//! # Grades
//!
//! | Register | Grade | Why |
//! |---|---|---|
//! | `i2c0_ctrl` (`+0x000`) | `modeled` | this block answers `busy` itself, and the latch is a reading of the bench rather than a measurement of the bit |
//! | `i2c0_data` (`+0x008`) | `modeled` | `rdata` is answered from the store |
//! | everything else | the PAC's (`with_pac_grades`) | accept-and-remember |

use std::collections::BTreeMap;

use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::{BusCx, Domain, Peripheral, RegFile, Width};

use super::lp_peri::LpPeriLines;
use crate::regs;

/// The window, as `boot_set` registers it. The PAC's names stop at `date`
/// (`+0x3fc`); the aperture is the whole `0x400`.
pub const LEN: u32 = 0x400;

/// `i2c0_ctrl` — the transaction port, and the register the bootloader spins
/// on.
pub const I2C0_CTRL: u32 = 0x000;
/// `i2c0_data` — `rdata` in bits 0:7, `i2c0_clk_sel`/`i2c_mst_sel` above.
pub const I2C0_DATA: u32 = 0x008;

const SLAVE_ADDR: u32 = 0xff;
const SLAVE_REG_ADDR_SHIFT: u32 = 8;
const DATA_SHIFT: u32 = 16;
const READ_WRITE: u32 = 1 << 24;
/// `LP_I2C_ANA_MAST_I2C0_BUSY`.
pub const BUSY: u32 = 1 << 25;
/// `LP_I2C_ANA_MAST_I2C0_RDATA`.
const RDATA_MASK: u32 = 0xff;

/// The LP analog I2C master.
#[derive(Debug)]
pub struct LpI2cAnaMst {
    regs: RegFile,
    /// `LPPERI_CLK_EN` / `LPPERI_RESET_EN` bit 29, from
    /// [`super::lp_peri::LpPeri`].
    lines: LpPeriLines,
    /// A transaction was started with the clock gated, so `busy` reads 1
    /// until the reset line pulses.
    busy_latched: bool,
    /// `{slave_addr, slave_reg_addr}` → the byte last written there. A
    /// `BTreeMap` so `save_state` is a run-to-run identical blob.
    analog: BTreeMap<(u8, u8), u8>,
}

impl LpI2cAnaMst {
    pub fn new(lines: LpPeriLines) -> Self {
        Self {
            regs: RegFile::new("LP_I2C_ANA_MST", LEN)
                .with_names(regs::LP_I2C_ANA_MST)
                // The guest's spin bit is this block's to answer; the
                // override is re-set on every latch and every clear.
                .with_read_override(I2C0_CTRL, BUSY, 0)
                .with_pac_grades()
                .with_grade(I2C0_DATA, RegGrade::Modeled),
            lines,
            busy_latched: false,
            analog: BTreeMap::new(),
        }
    }

    /// The byte at `{block, register}`, or 0 where nothing has been written.
    pub fn analog(&self, block: u8, reg: u8) -> u8 {
        self.analog.get(&(block, reg)).copied().unwrap_or(0)
    }

    /// Is `busy` latched?
    pub fn busy_latched(&self) -> bool {
        self.busy_latched
    }

    /// Consume a rising edge of the reset line, if `LP_PERI` has raised one
    /// since the last access. Called before every read and every write,
    /// which is the only way this block can be observed — the pulse is two
    /// writes to `LP_PERI` with no access here in between, so the edge is
    /// applied when the guest next looks.
    fn sync_reset(&mut self) {
        if self.lines.take_ana_i2c_reset_pulse() {
            self.regs.reset();
            self.busy_latched = false;
            self.apply_busy();
        }
    }

    fn apply_busy(&mut self) {
        let value = if self.busy_latched { BUSY } else { 0 };
        self.regs.set_read_override(I2C0_CTRL, BUSY, value);
    }

    /// Run the transaction a write to `i2c0_ctrl` just asked for, leaving a
    /// read's answer in `i2c0_data.rdata`.
    fn transact(&mut self) {
        let word = self.regs.stored(I2C0_CTRL);
        let block = (word & SLAVE_ADDR) as u8;
        let reg = ((word >> SLAVE_REG_ADDR_SHIFT) & 0xff) as u8;
        let data = ((word >> DATA_SHIFT) & 0xff) as u8;
        if word & READ_WRITE != 0 {
            self.analog.insert((block, reg), data);
            return;
        }
        let answer = self.analog(block, reg);
        let held = self.regs.stored(I2C0_DATA);
        self.regs
            .poke(I2C0_DATA, (held & !RDATA_MASK) | u32::from(answer));
    }
}

impl Peripheral for LpI2cAnaMst {
    fn name(&self) -> &'static str {
        "LP_I2C_ANA_MST"
    }

    /// The **low-power island**, with the `LPPERI` block that gates it. The
    /// latched busy this block holds is the *other* half of the wedge: on the
    /// bench, setting the clock alone did not clear it, which is why the
    /// flasher's cure pulses `LPPERI_RESET_EN` bit 29 as well — a latch a
    /// reset did not clear is a latch in a domain the reset did not reach.
    fn domain(&self) -> Domain {
        Domain::Lp
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        self.sync_reset();
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        self.sync_reset();
        // Held in reset: the block is not there to be written.
        if self.lines.ana_i2c_reset() {
            return;
        }
        self.regs.write(off, width, value, cx);
        // The transaction runs off the *stored* word, so a byte-lane write
        // that completes a `{block, register}` pair works the way a word
        // write does — the same reason [`super::i2c_ana_mst`] gives.
        if off & !3 != I2C0_CTRL {
            return;
        }
        if self.lines.ana_i2c_clk() {
            self.transact();
        } else {
            // The whole defect, in one line: a transaction with no clock
            // to finish it never finishes.
            self.busy_latched = true;
            self.apply_busy();
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::LP_I2C_ANA_MST.name(off)
    }

    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = self.regs.save_state();
        out.extend_from_slice(&u32::from(self.busy_latched).to_le_bytes());
        out.extend_from_slice(&(self.analog.len() as u32).to_le_bytes());
        for ((block, reg), value) in &self.analog {
            out.extend_from_slice(&[*block, *reg, *value]);
        }
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let window = LEN as usize;
        if bytes.len() < window + 8 {
            log::warn!("LP_I2C_ANA_MST: load_state blob too short, ignored");
            return;
        }
        self.regs.load_state(&bytes[..window]);
        let word = |at: usize| {
            u32::from_le_bytes(
                bytes[at..at + 4]
                    .try_into()
                    .expect("four bytes inside the blob"),
            )
        };
        self.busy_latched = word(window) != 0;
        let count = word(window + 4) as usize;
        let rows = &bytes[window + 8..];
        if rows.len() != count * 3 {
            log::warn!(
                "LP_I2C_ANA_MST: load_state has {count} pairs but {} bytes",
                rows.len()
            );
            return;
        }
        self.analog.clear();
        for row in rows.chunks_exact(3) {
            self.analog.insert((row[0], row[1]), row[2]);
        }
        self.apply_busy();
    }
}

#[cfg(test)]
mod tests {
    use super::super::lp_peri::{CLK_EN_RESET, LP_ANA_I2C_BIT, LpPeri, LpPeriLines};
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// The word `regi2c_ctrl_write_reg_mask` writes for a read, as the
    /// wedged board's `STUCK_CTRL` shows it: `{block, register}` and nothing
    /// else.
    fn read_request(block: u8, reg: u8) -> u32 {
        u32::from(block) | (u32::from(reg) << SLAVE_REG_ADDR_SHIFT)
    }

    fn write_request(block: u8, reg: u8, data: u8) -> u32 {
        read_request(block, reg) | READ_WRITE | (u32::from(data) << DATA_SHIFT)
    }

    /// A clean board: clocked, not reset.
    fn clean() -> (LpI2cAnaMst, LpPeriLines) {
        let lines = LpPeriLines::new(CLK_EN_RESET);
        (LpI2cAnaMst::new(lines.clone()), lines)
    }

    /// The induced board the bench made: `LPPERI_CLK_EN 0x5f00_0000`.
    fn induced() -> (LpI2cAnaMst, LpPeriLines) {
        let lines = LpPeriLines::new(0x5f00_0000);
        (LpI2cAnaMst::new(lines.clone()), lines)
    }

    fn regi2c_write(sb: &mut Sandbox, m: &mut LpI2cAnaMst, block: u8, reg: u8, data: u8) {
        sb.write(m, I2C0_CTRL, write_request(block, reg, data));
    }

    fn regi2c_read(sb: &mut Sandbox, m: &mut LpI2cAnaMst, block: u8, reg: u8) -> u8 {
        sb.write(m, I2C0_CTRL, read_request(block, reg));
        (sb.read(m, I2C0_DATA) & RDATA_MASK) as u8
    }

    #[test]
    fn a_clocked_transaction_finishes_and_answers_the_register_it_asked_for() {
        let mut sb = Sandbox::new();
        let (mut m, _) = clean();
        regi2c_write(&mut sb, &mut m, 0x6d, 0x0e, 0xaa);
        regi2c_write(&mut sb, &mut m, 0x6d, 0x0f, 0xbb);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x6d, 0x0e), 0xaa);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x6d, 0x0f), 0xbb);
        // A pair nobody has written is 0, whatever its neighbours hold.
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x6d, 0x10), 0);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x62, 0x07), 0);
        // …and `busy` reads 0 throughout, which is what the spin needs.
        assert_eq!(sb.read(&mut m, I2C0_CTRL) & BUSY, 0);
        assert!(!m.busy_latched());
        // `i2c0_data`'s other fields are untouched by an answer.
        assert_eq!(sb.read(&mut m, I2C0_DATA) & !RDATA_MASK, 0x0000_0900);
        assert_eq!(m.reg_name(I2C0_CTRL), Some("i2c0_ctrl"));
        assert_eq!(m.reg_name(I2C0_DATA), Some("i2c0_data"));
        assert_eq!(m.name(), "LP_I2C_ANA_MST");
    }

    /// The hang: the bootloader's first `regi2c` write on an induced board.
    #[test]
    fn a_write_with_the_clock_gated_latches_busy_for_ever() {
        let mut sb = Sandbox::new();
        let (mut m, lines) = induced();
        assert!(!lines.ana_i2c_clk());
        // Before the write there is nothing wrong with the block.
        assert_eq!(sb.read(&mut m, I2C0_CTRL) & BUSY, 0);

        sb.write(&mut m, I2C0_CTRL, read_request(0x6d, 0x0e));
        // What the wedged board read back, bit for bit.
        assert_eq!(sb.read(&mut m, I2C0_CTRL), 0x0200_0e6d, "STUCK_CTRL");
        assert!(m.busy_latched());
        // Every subsequent read, which is the spin.
        for _ in 0..8 {
            assert_ne!(sb.read(&mut m, I2C0_CTRL) & BUSY, 0);
        }
        // The transaction did not run: nothing reached the analog store.
        assert_eq!(m.analog(0x6d, 0x0e), 0);
    }

    /// "Setting the clock alone does NOT clear the latched busy" — the
    /// bench line the flashers' ordering exists for.
    #[test]
    fn turning_the_clock_back_on_does_not_clear_a_latched_busy() {
        let mut sb = Sandbox::new();
        let lines = LpPeriLines::new(0x5f00_0000);
        let mut peri = LpPeri::new(0, 0x5f00_0000, lines.clone());
        let mut m = LpI2cAnaMst::new(lines.clone());
        sb.write(&mut m, I2C0_CTRL, read_request(0x6d, 0x0e));
        assert!(m.busy_latched());

        // The flasher's first write, through the block that owns the bit.
        sb.write(&mut peri, super::super::lp_peri::CLK_EN, 0x7f00_0000);
        assert!(lines.ana_i2c_clk());
        assert_ne!(sb.read(&mut m, I2C0_CTRL) & BUSY, 0, "still latched");

        // …and the rest of the cure: pulse the master's reset line.
        sb.write(&mut peri, super::super::lp_peri::RESET_EN, LP_ANA_I2C_BIT);
        sb.write(&mut peri, super::super::lp_peri::RESET_EN, 0);
        assert_eq!(sb.read(&mut m, I2C0_CTRL) & BUSY, 0, "busy cleared");
        assert!(!m.busy_latched());
        // The window is back at its reset values.
        assert_eq!(sb.read(&mut m, I2C0_CTRL), 0);
        assert_eq!(sb.read(&mut m, I2C0_DATA), 0x0000_0900);
        // And the block works again.
        regi2c_write(&mut sb, &mut m, 0x6d, 0x0e, 0x5a);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x6d, 0x0e), 0x5a);
    }

    /// A reset line held high is a block that is not there.
    #[test]
    fn a_held_reset_line_swallows_writes() {
        let mut sb = Sandbox::new();
        let (mut m, lines) = clean();
        let mut peri = LpPeri::new(0, CLK_EN_RESET, lines.clone());
        sb.write(&mut peri, super::super::lp_peri::RESET_EN, LP_ANA_I2C_BIT);
        regi2c_write(&mut sb, &mut m, 0x6d, 0x0e, 0x5a);
        assert_eq!(m.analog(0x6d, 0x0e), 0, "nothing ran");
        assert_eq!(sb.read(&mut m, I2C0_CTRL), 0, "and nothing was stored");
        assert!(!m.busy_latched(), "a reset is not a latch");
    }

    /// The analog store survives a reset pulse: it models the slaves, not
    /// the master.
    #[test]
    fn a_reset_pulse_resets_the_master_and_not_the_analog_world() {
        let mut sb = Sandbox::new();
        let (mut m, lines) = clean();
        let mut peri = LpPeri::new(0, CLK_EN_RESET, lines.clone());
        regi2c_write(&mut sb, &mut m, 0x6d, 0x0e, 0x5a);
        sb.write(&mut peri, super::super::lp_peri::RESET_EN, LP_ANA_I2C_BIT);
        sb.write(&mut peri, super::super::lp_peri::RESET_EN, 0);
        assert_eq!(regi2c_read(&mut sb, &mut m, 0x6d, 0x0e), 0x5a);
    }

    #[test]
    fn the_latch_and_the_store_round_trip_through_save_state() {
        let mut sb = Sandbox::new();
        let (mut m, _) = clean();
        regi2c_write(&mut sb, &mut m, 0x6d, 0x0e, 0x0c);
        regi2c_write(&mut sb, &mut m, 0x61, 0x03, 0x09);
        let blob = m.save_state();

        let (mut other, _) = clean();
        other.load_state(&blob);
        assert_eq!(other.analog(0x6d, 0x0e), 0x0c);
        assert_eq!(other.analog(0x61, 0x03), 0x09);
        assert_eq!(other.save_state(), blob);

        // The latch travels too, and comes back as the read override.
        let (mut wedged, _) = induced();
        sb.write(&mut wedged, I2C0_CTRL, read_request(0x6d, 0x0e));
        let blob = wedged.save_state();
        let (mut restored, _) = clean();
        restored.load_state(&blob);
        assert!(restored.busy_latched());
        assert_ne!(sb.read(&mut restored, I2C0_CTRL) & BUSY, 0);
        assert_eq!(restored.save_state(), blob);
    }

    /// The grade table the module header publishes.
    #[test]
    fn the_two_registers_this_block_answers_for_are_graded_modeled() {
        let (m, _) = clean();
        assert_eq!(m.reg_grade(I2C0_CTRL), Some(RegGrade::Modeled));
        assert_eq!(m.reg_grade(I2C0_DATA), Some(RegGrade::Modeled));
        assert_eq!(m.reg_grade(0x014), Some(RegGrade::Documented), "device_en");
    }
}
