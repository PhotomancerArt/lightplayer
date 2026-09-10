//! `EFUSE` at `0x3FF5_A000` — the fuse array, the read-data registers it is
//! loaded into, and **the read command that is a completion**.
//!
//! # Why this block is a view and not an accept block
//!
//! Seven instructions after the reset vector the mask ROM runs its own
//! anti-glitch check on its fuses, and the check reads `cmd` (`+0x104`) on
//! **both** edges — first expecting the command still in flight, then
//! expecting it finished:
//!
//! ```text
//! 4000fc90 <_reload_efuses_and_check>:
//! 4000fc9b:  s32i.n  a3, a1, 0     ; EFUSE.conf  = 0x5aa5   (the read opcode)
//! 4000fc9d:  s32i.n  a4, a2, 0     ; EFUSE.cmd   = 1        (read_cmd)
//! 4000fc9f:  memw
//! 4000fca2:  l32i.n  a1, a2, 0     ; cmd, first read
//! 4000fca4:  add.n   a13, a13, a1  ; ...ACCUMULATED into the checksum
//! 4000fca6:  beqz    a1, _rtc_trigger_sw_system_reset
//! 4000fca9:  memw
//! 4000fcac:  l32i.n  a1, a2, 0
//! 4000fcae:  bnez    a1, 4000fca9  ; ...then spin until it clears
//! ```
//!
//! and the caller checks the accumulator afterwards:
//!
//! ```text
//! 4000fdef:  l32r    a13, (ee101017)      ; the seed
//! 4000fdf2:  call0   _reload_efuses_and_check   ; three times
//! 4000fdfe:  bne     a13, (ee10101a), _rtc_trigger_sw_system_reset
//! ```
//!
//! `0xee10101a − 0xee101017 = 3`, over three calls: **each call's first read
//! of `cmd` must be exactly 1.** A constant cannot be 1 and 0 at once, a
//! `with_write_one_pulse` reads 0 on that first read, and there is nothing
//! for a mirror to mirror — which is why P3 left the ROM path standing here
//! and named this phase (`docs/reports/2026-09-10-esp32v3-strict-boot-inventory.md`
//! §1.2, §3.3). The command is a *completion*: set by the guest, cleared by
//! the block when the read is done.
//!
//! # The fuse array and the read-data registers are two different things
//!
//! `blk0_rdata0..6` are not storage — they are the **latched result** of the
//! last read command. The ROM's check leans on exactly that distinction: it
//! parks the three words it read at `0x3FFE_1320`, re-runs the read command,
//! and requires the reloaded registers to compare equal three times over
//! (`4000fcba..4000fd84`). So this block holds an [`Efuse::fuses`] array —
//! the burned state, from [`EfuseIdentity`] — and the read command copies it
//! into the register file. Both are seeded at reset, because on silicon the
//! controller loads the read registers before the CPU starts.
//!
//! # What is burned, and what is not
//!
//! Only the fields something on the boot path reads are seeded; every other
//! word is the PAC's zero, which is what an unburned fuse reads. The layout
//! is esp-hal 1.1.1's (`src/efuse/esp32/fields.rs`, bit offsets inside
//! block 0):
//!
//! ```text
//! MAC0                 bits  32..64   → blk0_rdata1 (+0x04), all four bytes
//! MAC1                 bits  64..80   → blk0_rdata2 (+0x08) bits 15:0
//! MAC_CRC              bits  80..88   → blk0_rdata2 (+0x08) bits 23:16   (not modelled, see below)
//! CHIP_VER_REV1        bit  111       → blk0_rdata3 (+0x0c) bit 15
//! CHIP_VER_REV2        bit  180       → blk0_rdata5 (+0x14) bit 20
//! WAFER_VERSION_MINOR  bits 184..186  → blk0_rdata5 (+0x14) bits 25:24
//! ```
//!
//! "MAC address is stored in big endian, so load the bytes in reverse" —
//! `esp-hal-1.1.1/src/efuse/mod.rs:210`, and [`identity`] applies that
//! extraction to the words [`words`] produces, both ways, in a test.
//!
//! **`MAC_CRC` is left at 0** and is a deliberate gap: it is a CRC-8 whose
//! polynomial this phase has no source for in reach, nothing on either boot
//! path reads it (esp-hal's `base_mac_address` does not; the ROM's check
//! compares words against themselves), and inventing a checksum byte would
//! be exactly the "seeded with the value a spin happened to want" mistake
//! `crate::periph::accept`'s module docs exist to prevent. Recorded for
//! whoever first meets a reader that does check it.
//!
//! ⚠️ **The major revision is not an eFuse field alone.** esp-hal's
//! `major_chip_version` (`src/efuse/esp32/mod.rs`) is
//! `(eco2 << 2) | (eco1 << 1) | eco0` mapped `1 → 1, 3 → 2, 7 → 3, _ → 0`,
//! and `eco2` is **bit 31 of `APB_CTRL.date`**. v3 therefore needs a bit
//! outside this block; [`crate::periph::accept::apb_ctrl`] carries it as a
//! listed deviation, with the same citation.

use lp_emu_core::sched::EventId;
use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width, event_id, event_local};

use crate::loader::EfuseIdentity;
use crate::memmap;
use crate::regs;

/// The block's aperture: the generated table runs to `+0x1fc` (`date`).
pub const EFUSE_LEN: u32 = 0x200;

/// Block 0's seven read-data words, `blk0_rdata0..6` at `+0x00..+0x18`.
pub const BLK0_WORDS: usize = 7;

/// `conf` — the operation opcode register (`+0xfc`).
pub const CONF: u32 = 0x0fc;
/// `status` (`+0x100`), read-only.
pub const STATUS: u32 = 0x100;
/// `cmd` (`+0x104`): `read_cmd` is bit 0, `pgm_cmd` bit 1.
pub const CMD: u32 = 0x104;

const READ_CMD: u32 = 1 << 0;
const PGM_CMD: u32 = 1 << 1;

/// The read opcode the mask ROM writes to `conf` before every read command
/// (`4000fc96: l32r a3, (0x5aa5)`). Remembered, not enforced — see
/// [`Efuse::write_word`].
pub const CONF_READ_OPCODE: u32 = 0x5aa5;

/// How long a read command stays set before the block clears it.
///
/// ⚠️ **This is the one number in this block that no source pins.** What the
/// ROM's check requires is only an *ordering*: the command must still read
/// set at the load two instructions after the store (`4000fc9d` → `4000fca2`)
/// and must clear without software help. Nothing in the PAC, the ROM or
/// esp-hal states how many cycles silicon takes — IDF and the ROM both spin
/// on the bit with no timeout and no delay — so the model states a duration
/// rather than pretending to have measured one: **one emulated microsecond**,
/// [`memmap::CYCLES_PER_US`], the smallest unit this machine's time grade
/// expresses. Every register of this block is graded `Modeled` accordingly.
pub const READ_CMD_CYCLES: u64 = memmap::CYCLES_PER_US;

const EV_READ_DONE: u16 = 0;

/// The words an identity burns into block 0, `(word index, value)`.
///
/// Only the words a reader on the boot path looks at; see the module docs
/// for the field table and its citation.
pub fn words(id: &EfuseIdentity) -> [(usize, u32); 4] {
    let m = id.mac;
    // MAC0, bits 32..64 → word 1, read back as four little-endian bytes and
    // reversed into MAC[2..6] (`efuse/mod.rs:206-212`).
    let w1 = (u32::from(m[2]) << 24)
        | (u32::from(m[3]) << 16)
        | (u32::from(m[4]) << 8)
        | u32::from(m[5]);
    // MAC1, bits 64..80 → word 2's low half, reversed into MAC[0..2].
    // MAC_CRC (bits 80..88) stays 0; see the module docs.
    let w2 = (u32::from(m[0]) << 8) | u32::from(m[1]);
    // CHIP_VER_REV1 (word 3 bit 15) is the low bit of esp-hal's three-bit
    // combination; the middle one is word 5's bit 20 and the top one is not
    // an eFuse at all.
    let (eco0, eco1, _eco2) = eco_bits(id.chip_major);
    let w3 = u32::from(eco0) << 15;
    // Word 5 carries CHIP_VER_REV2 and WAFER_VERSION_MINOR together.
    let w5 = (u32::from(eco1) << 20) | (u32::from(id.chip_minor & 0b11) << 24);
    [(1, w1), (2, w2), (3, w3), (5, w5)]
}

/// The three bits esp-hal combines into a major revision, for `major`.
///
/// `major_chip_version` (`esp-hal-1.1.1/src/efuse/esp32/mod.rs`) maps
/// `(eco2 << 2) | (eco1 << 1) | eco0` with `1 → 1`, `3 → 2`, `7 → 3` and
/// everything else to 0, so the encoding is a thermometer: revision *n* has
/// the low *n* bits set. `eco2` is not an eFuse — it is `APB_CTRL.date`
/// bit 31.
pub const fn eco_bits(major: u8) -> (u8, u8, u8) {
    match major {
        1 => (1, 0, 0),
        2 => (1, 1, 0),
        3 => (1, 1, 1),
        _ => (0, 0, 0),
    }
}

/// esp-hal's extraction, applied to block 0's words — the other direction of
/// [`words`], so a test can assert the round trip rather than a constant.
///
/// `eco2` is [`eco_bits`]'s third bit, which the caller reads out of
/// `APB_CTRL.date` bit 31; it is a parameter here because this block cannot
/// see that register.
pub fn identity(fuses: &[u32; BLK0_WORDS], eco2: bool) -> EfuseIdentity {
    let mac0 = fuses[1].to_le_bytes();
    let mac1 = (fuses[2] & 0xffff).to_le_bytes();
    let eco0 = (fuses[3] >> 15) & 1;
    let eco1 = (fuses[5] >> 20) & 1;
    let combined = (u32::from(eco2) << 2) | (eco1 << 1) | eco0;
    EfuseIdentity {
        mac: [mac1[1], mac1[0], mac0[3], mac0[2], mac0[1], mac0[0]],
        chip_major: match combined {
            1 => 1,
            3 => 2,
            7 => 3,
            _ => 0,
        },
        chip_minor: ((fuses[5] >> 24) & 0b11) as u8,
    }
}

/// The classic's eFuse controller.
#[derive(Debug)]
pub struct Efuse {
    index: usize,
    regs: RegFile,
    /// Block 0's burned words — the array the read command loads from.
    fuses: [u32; BLK0_WORDS],
}

impl Efuse {
    /// The block, with `id` burned into block 0.
    pub fn new(id: EfuseIdentity) -> Self {
        let mut fuses = [0u32; BLK0_WORDS];
        for (word, value) in words(&id) {
            fuses[word] = value;
        }

        let mut regs = RegFile::new("EFUSE", EFUSE_LEN)
            .with_names(regs::EFUSE)
            .with_pac_grades();
        // Silicon loads the read-data registers before the CPU runs; a
        // machine whose first `l32i` on `blk0_rdata0` read 0 and only
        // answered after a read command would be modelling a controller
        // nobody has.
        for (i, word) in fuses.iter().enumerate() {
            regs.poke(4 * i as u32, *word);
        }
        Self {
            index: 0,
            regs,
            fuses,
        }
    }

    /// Block 0's burned words.
    pub fn fuses(&self) -> &[u32; BLK0_WORDS] {
        &self.fuses
    }

    /// Load the fuse array into the read-data registers — what the read
    /// command completing does, and the only thing that writes them.
    fn reload(&mut self) {
        for (i, word) in self.fuses.iter().enumerate() {
            self.regs.poke(4 * i as u32, *word);
        }
    }

    fn read_word(&self, off: u32) -> u32 {
        self.regs.stored(off)
    }

    fn write_word(&mut self, off: u32, value: u32, cx: &mut BusCx<'_>) {
        match off {
            // The read-data registers are the controller's output. The PAC
            // marks `blk0_rdata0`/`rdata1` read-only and leaves the rest
            // plain read-write, which is an SVD gap rather than a writable
            // fuse latch: a store into one is dropped and said out loud.
            0x000..=0x018 => {
                log::debug!(
                    "EFUSE: write {value:#010x} to +{off:#05x} dropped; the read-data registers \
                     are loaded by the read command, not by the guest"
                );
            }
            CMD => {
                if value & PGM_CMD != 0 {
                    let line = format!(
                        "cyc={} pc=0x{:08x} EFUSE PGM_CMD (burning a fuse is not modelled)",
                        cx.now, cx.pc
                    );
                    cx.trace.note(&line);
                    log::warn!("EFUSE: pgm_cmd written; burning a fuse is not modelled");
                }
                if value & READ_CMD == 0 {
                    return;
                }
                // A completion: set now, cleared by the block when the read
                // lands. `conf` is remembered but does not gate it — whether
                // silicon refuses a wrong opcode is not something any source
                // in reach says, and nothing on either boot path writes one.
                self.regs.poke(CMD, READ_CMD);
                cx.sched
                    .schedule_in(cx.now, READ_CMD_CYCLES, event_id(self.index, EV_READ_DONE));
            }
            STATUS => {}
            other => self.regs.poke(other, value),
        }
    }
}

impl Peripheral for Efuse {
    fn name(&self) -> &'static str {
        "EFUSE"
    }

    fn attached(&mut self, index: usize) {
        self.index = index;
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        lane_of(self.read_word(off & !3), off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        let merged = merge_lane(self.read_word(word), off, width, value);
        self.write_word(word, merged, cx);
    }

    fn on_event(&mut self, id: EventId, _cx: &mut BusCx<'_>) {
        if event_local(id) == EV_READ_DONE {
            self.reload();
            self.regs.poke(CMD, 0);
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::EFUSE.name(off)
    }

    /// The PAC's grades, with the identity words demoted to what they are.
    ///
    /// `with_pac_grades` calls a read-only register `Modeled` already, which
    /// is the right answer here for a different reason than the PAC's: the
    /// bytes are this run's own `--efuse-mac` / `--efuse-rev`, not something
    /// silicon told us. Nothing in this block is `Measured`.
    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(EFUSE_LEN as usize + 8 + 4 * BLK0_WORDS);
        out.extend_from_slice(&(self.index as u64).to_le_bytes());
        for word in &self.fuses {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let head = 8 + 4 * BLK0_WORDS;
        if bytes.len() < head {
            log::warn!("EFUSE: load_state blob too short, ignored");
            return;
        }
        let mut index = [0u8; 8];
        index.copy_from_slice(&bytes[..8]);
        self.index = u64::from_le_bytes(index) as usize;
        for (i, chunk) in bytes[8..head].chunks_exact(4).enumerate() {
            self.fuses[i] = u32::from_le_bytes(chunk.try_into().expect("four bytes"));
        }
        self.regs.load_state(&bytes[head..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::DESK_MAC;
    use lp_emu_esp_common::Sandbox;

    #[test]
    fn the_desk_boards_identity_round_trips_through_the_words() {
        let id = EfuseIdentity::default();
        assert_eq!(id.mac, DESK_MAC);
        assert_eq!((id.chip_major, id.chip_minor), (3, 1));
        let e = Efuse::new(id);
        let f = e.fuses();
        // MAC[2..6] big-endian in word 1, MAC[0..2] in word 2's low half.
        assert_eq!(f[1], 0xf5ec_f634);
        assert_eq!(f[2], 0x0000_3076);
        // v3: CHIP_VER_REV1 (word 3 bit 15) and CHIP_VER_REV2 (word 5 bit
        // 20) both set; the third bit is APB_CTRL.date's.
        assert_eq!(f[3], 1 << 15);
        assert_eq!(f[5], (1 << 20) | (1 << 24), "REV2 set, minor = 1");
        assert_eq!(identity(f, true), id, "with APB_CTRL.date bit 31 set");
        assert_eq!(
            identity(f, false).chip_major,
            2,
            "without APB_CTRL's bit the same fuses read as v2: esp-hal's combination is 0b011"
        );
    }

    #[test]
    fn every_major_revision_round_trips() {
        for major in 0..=3u8 {
            for minor in 0..=3u8 {
                let id = EfuseIdentity {
                    mac: DESK_MAC,
                    chip_major: major,
                    chip_minor: minor,
                };
                let e = Efuse::new(id);
                let (_, _, eco2) = eco_bits(major);
                assert_eq!(identity(e.fuses(), eco2 != 0), id);
            }
        }
    }

    /// The ROM's check, instruction for instruction: `conf`, `cmd`, one read
    /// that must be 1, then a spin that must end.
    #[test]
    fn the_read_command_reads_set_once_and_then_clears_itself() {
        let mut sb = Sandbox::new();
        let mut e = Efuse::new(EfuseIdentity::default());
        e.attached(11);
        assert_eq!(e.reg_name(CMD), Some("cmd"));
        assert_eq!(e.reg_name(CONF), Some("conf"));

        sb.now = 1_000;
        sb.write(&mut e, CONF, CONF_READ_OPCODE);
        sb.write(&mut e, CMD, READ_CMD);
        // `4000fca2: l32i.n a1, a2, 0` — two instructions after the store,
        // and the ROM adds this value to a checksum it later compares.
        assert_eq!(sb.read(&mut e, CMD), 1, "the first read is the completion");
        assert_eq!(sb.sched.next_deadline(), Some(1_000 + READ_CMD_CYCLES));
        // The spin: still set right up to the completion.
        sb.run_to(&mut e, 1_000 + READ_CMD_CYCLES - 1);
        assert_eq!(sb.read(&mut e, CMD), 1);
        sb.run_to(&mut e, 1_000 + READ_CMD_CYCLES);
        assert_eq!(sb.read(&mut e, CMD), 0, "cleared by the block");
        assert_eq!(sb.read(&mut e, CONF), CONF_READ_OPCODE, "remembered");
    }

    /// What `_reload_efuses_and_check` does after the spin: read the three
    /// words again and require them unchanged.
    #[test]
    fn a_reload_answers_the_same_words_every_time() {
        let mut sb = Sandbox::new();
        let mut e = Efuse::new(EfuseIdentity::default());
        e.attached(11);
        let before = [
            sb.read(&mut e, 0x000),
            sb.read(&mut e, 0x014),
            sb.read(&mut e, 0x018),
        ];
        for round in 0..3 {
            sb.now = 10_000 * (round + 1);
            sb.write(&mut e, CONF, CONF_READ_OPCODE);
            sb.write(&mut e, CMD, READ_CMD);
            assert_eq!(sb.read(&mut e, CMD), 1);
            sb.run_to(&mut e, sb.now + READ_CMD_CYCLES);
            assert_eq!(sb.read(&mut e, CMD), 0);
            assert_eq!(
                [
                    sb.read(&mut e, 0x000),
                    sb.read(&mut e, 0x014),
                    sb.read(&mut e, 0x018)
                ],
                before,
                "round {round}: the reload must compare equal"
            );
        }
    }

    #[test]
    fn the_read_data_registers_are_the_controllers_output_not_guest_storage() {
        let mut sb = Sandbox::new();
        let mut e = Efuse::new(EfuseIdentity::default());
        sb.write(&mut e, 0x004, 0xdead_beef);
        assert_eq!(sb.read(&mut e, 0x004), 0xf5ec_f634, "the write was dropped");
        // A word nobody burned reads the PAC's zero.
        assert_eq!(sb.read(&mut e, 0x010), 0);
        // And the rest of the block is accept-and-remember at the PAC's
        // resets: `clk` = 0x4052, `dac_conf` = 0x28.
        assert_eq!(sb.read(&mut e, 0x0f8), 0x0000_4052);
        assert_eq!(sb.read(&mut e, 0x118), 0x0000_0028);
    }

    #[test]
    fn the_state_round_trips() {
        let mut sb = Sandbox::new();
        let mut e = Efuse::new(EfuseIdentity {
            mac: [1, 2, 3, 4, 5, 6],
            chip_major: 2,
            chip_minor: 3,
        });
        e.attached(7);
        sb.write(&mut e, CONF, CONF_READ_OPCODE);
        let blob = e.save_state();
        let mut other = Efuse::new(EfuseIdentity::default());
        other.load_state(&blob);
        assert_eq!(other.fuses(), e.fuses());
        assert_eq!(other.index, 7);
        assert_eq!(sb.read(&mut other, CONF), CONF_READ_OPCODE);
        assert_eq!(other.save_state(), blob);
    }
}
