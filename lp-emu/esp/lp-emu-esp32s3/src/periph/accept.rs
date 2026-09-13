//! The accept-and-remember blocks: a [`RegFile`] each, seeded from the PAC's
//! reset values, with the exceptions the boot path needs written down as
//! overrides — **each carrying its evidence beside it.**
//!
//! # The two rules this file exists to hold
//!
//! **A reset value comes from the PAC's `Resettable`.** [`RegFile::with_names`]
//! seeds every register of a block from the generated `regs/<block>.rs`
//! table, so a block reads what the part reads before anyone writes it. A
//! `with_reset` in this file is a **deviation from the PAC**, and the test
//! `the_only_deviations_from_the_pacs_resets_are_the_listed_ones` is the
//! list. The sweep that produced the rule is
//! `docs/defects/2026-09-07-accept-blocks-carry-only-the-reset-values-a-boot-needed.md`:
//! an accept block seeded with the value a spin happened to want looks
//! exactly like one seeded from the PAC, right up to the moment a different
//! image spins on a different value.
//!
//! **An exception carries its evidence beside it.** Not "the boot spins
//! here", but *why the value is what it is*: a ROM disassembly line, a PAC
//! field doc, an esp-hal source line. A spin no pin table can justify is an
//! **E-premise stop** — reported in the phase report, not answered with an
//! invented value. Every read override in this file is listed in
//! [`READ_OVERRIDES`] with its citation, and a test holds that the list and
//! the blocks agree.
//!
//! # Which phase owns which block
//!
//! Every block here is a probe. The phase that gives it behaviour is named
//! on the block; `SPI0`/`SPI1` and `EXTMEM` are **P06**'s, the four RF
//! blocks are nobody's (nothing on this chip's radio is modelled and nothing
//! here suggests it is).

use lp_emu_esp_common::RegFile;

use crate::regs;

/// `SENSITIVE`'s aperture: the generated table runs to `+0xffc` (`date`),
/// and the next block (`INTERRUPT_CORE0`) is at `+0x1000`.
pub const SENSITIVE_LEN: u32 = 0x1000;

/// `SENSITIVE` at `0x600C_1000` — the permission-management block, and
/// **the mask ROM's first MMIO access on a direct load**.
///
/// P03's first strict stop, thirty-six cycles in: `Cache_Occupy_ICache_MEMORY
/// +0xc` (`0x4004_F670`) reads `+0x04` (`cache_dataarray_connect_1`) on
/// `esp_hal::init`'s `rom_config_instruction_cache_mode` path, which the
/// application's own literal census could not see. The routine
/// read-modify-writes two words and spins on nothing:
///
/// ```text
/// 4004f667:  l32r   a8, (0x600c1004)     ; cache_dataarray_connect_1
/// 4004f670:  l32i.n a10, a8, 0
/// 4004f677:  and    a10, a10, -16        ; low nibble := which SRAM banks the icache occupies
/// 4004f686:  s32i.n a9, a8, 0
/// 4004f680:  l32r   a10, (0x600c1014)    ; internal_sram_usage_1
/// 4004f68b:  l32i.n a9, a10, 0
/// 4004f69a:  and    a9, a9, -4           ; low two bits := the banks the CPU may still use
/// 4004f6a3:  s32i.n a8, a10, 0
/// ```
///
/// (`Cache_Occupy_DCache_MEMORY`, `0x4004_F6A8`, does the same to bits 4:7
/// and 2:3.) Accept-and-remember with the PAC's resets — `+0x04` reads
/// `0xff`, `+0x14` `0x7ff` — is the whole model: the words say which SRAM
/// banks are cache and which are memory, and on this machine the memory map
/// is [`crate::memmap`]'s whatever they hold. Nothing else in the boot
/// reaches the block; the trace names what does.
pub fn sensitive() -> RegFile {
    RegFile::new("SENSITIVE", SENSITIVE_LEN)
        .with_names(regs::SENSITIVE)
        .with_pac_grades()
}

/// `EXTMEM`'s aperture: the generated table runs to `+0x3fc` (`date`).
pub const EXTMEM_LEN: u32 = 0x400;

/// `EXTMEM` at `0x600C_4000` — the cache controller, **accept-with-the-
/// operation-done-bits-answered**; the cache *model* is P06's.
///
/// # What the ROM's cache routines need, and no more
///
/// The mask ROM's `rom_config_instruction_cache_mode` / `_data_cache_mode`
/// (called from esp-hal's `configure_cpu_caches`, `soc/esp32s3/mod.rs`)
/// set the mode bits, invalidate, and enable. Every one of those is a
/// read-modify-write that hardware would answer with "done" at once, and
/// the P03 scouting run stood at the one place that is not:
/// `Cache_Invalidate_ICache_Items+0x41` polling a done bit an unmodelled
/// block never sets. The idiom is the classic's `spi_flash`/cache-enable
/// one — **write the operation bit, read the done bit** — and this block
/// answers it for every operation register the PAC declares one for:
///
/// | register | operation bit(s), write-one-pulse | done bit, reads 1 | who polls it |
/// |---|---|---|---|
/// | `icache_sync_ctrl` `+0x88` | 0 `invalidate_ena` | 1 `invalidate_done` | `Cache_Invalidate_ICache_Items+0x3c` (`0x4004_E4FC`: `bnone a9, 2`) |
/// | `dcache_sync_ctrl` `+0x28` | 0 `invalidate_ena`, 1 `writeback_ena`, 2 `clean_ena` | 3 `sync_done` | `Cache_Invalidate_DCache_Items+0x3c` (`0x4004_E550`: `bnone a9, 8`) |
/// | `icache_preload_ctrl` `+0x94` | 0 `preload_ena` | 1 `preload_done` | `Cache_Disable_ICache+0x29` (`0x4004_F2E1`: `bnone a9, 2`) |
/// | `dcache_preload_ctrl` `+0x40` | 0 `preload_ena` | 1 `preload_done` | `Cache_Suspend_DCache+0x38` (`0x4004_F464`: `bnone a9, 2`) |
/// | `icache_lock_ctrl` `+0x7c` | 0 `lock_ena`, 1 `unlock_ena` | 2 `lock_done` | the lock routines (same shape) |
/// | `dcache_lock_ctrl` `+0x1c` | 0 `lock_ena`, 1 `unlock_ena` | 2 `lock_done` | the lock routines (same shape) |
/// | `icache_autoload_ctrl` `+0xa0` | — | 3 `autoload_done` | the autoload suspend/resume routines |
/// | `dcache_autoload_ctrl` `+0x4c` | — | 3 `autoload_done` | the autoload suspend/resume routines |
/// | `cache_state` `+0x130` | — | 0 `icache_state = 1`, 12 `dcache_state = 1` (idle) | `Cache_Disable_ICache+0x40` (`0x4004_F2F8`: `extui 0,12; bnei 1`) |
/// | `icache_freeze` `+0x154`, `dcache_freeze` `+0x150` | 0 `freeze_ena` — **a mirror, not a pulse** | 2 `freeze_done` **mirrors bit 0** | `Cache_Freeze_*_Enable` (`0x4004_E8A8`/`E910`) sets bit 0 and polls `bany 4` until done is **1**; `Cache_Freeze_*_Disable` (`0x4004_E8E8`/`E950`) clears bit 0 and polls until done is **0** |
///
/// The freeze pair is the one place no constant will do: the ROM waits for
/// `done` to go **both ways** — and P04's first traced run spun for two
/// emulated seconds at `Cache_Freeze_DCache_Disable+0x1b` on the PAC's
/// reset `0x04` before this row existed. [`RegFile::with_read_mirror`] was
/// built for exactly this shape on the C6's `l1_cache_freeze_ctrl`, and it
/// says the true thing: the freeze finished, in whichever direction it was
/// asked for. It is listed in [`READ_MIRRORS`].
///
/// The bit positions are the PAC's own field docs
/// (`esp32s3-0.35.2/src/extmem/*.rs`: "It will be cleared by hardware after
/// … operation done" / "The bit is used to indicate … operation is
/// finished" / "1: in idle state"); the pulse form is that first sentence,
/// the override is the second. `cache_state` is the one register the PAC
/// gives **no** reset for, and a strict accept block would spin
/// `Cache_Disable_ICache` forever on it; a cache with no operation in
/// flight is idle, which is the value the field doc names, and it is listed
/// in [`READ_OVERRIDES`] as the deviation it is.
///
/// # ⚠️ The cache-enable polarity is INVERTED relative to the C6's
///
/// `icache_ctrl.icache_enable` (`+0x60` bit 0) and `dcache_ctrl.
/// dcache_enable` (`+0x00` bit 0) are *"0: disable, 1: enable"*, where the
/// C6's `l1_icache_ctrl.l1_icache_shut_ibus0` is *"0: enable, 1: disable"*.
/// A cache-off watch (P06's D4) copied from the C6's predicate arms
/// backwards. Written here, at the register, because this is where P06 will
/// read the bit; both are plain accept-and-remember in this phase, and
/// `Cache_Set_ICache_Mode` (`0x4004_E290`) read-modify-writes bits 1..3 of
/// `+0x60` (way mode, size, line size) around them.
///
/// **Not here**: the flash MMU table, which is not in this block at all
/// (`regs/mod.rs`), and any fill of the IROM/DROM windows — P06.
pub fn extmem() -> RegFile {
    const ICACHE_SYNC_CTRL: u32 = 0x088;
    const DCACHE_SYNC_CTRL: u32 = 0x028;
    const ICACHE_PRELOAD_CTRL: u32 = 0x094;
    const DCACHE_PRELOAD_CTRL: u32 = 0x040;
    const ICACHE_LOCK_CTRL: u32 = 0x07c;
    const DCACHE_LOCK_CTRL: u32 = 0x01c;
    const ICACHE_AUTOLOAD_CTRL: u32 = 0x0a0;
    const DCACHE_AUTOLOAD_CTRL: u32 = 0x04c;
    const CACHE_STATE: u32 = 0x130;
    const DCACHE_FREEZE: u32 = 0x150;
    const ICACHE_FREEZE: u32 = 0x154;
    RegFile::new("EXTMEM", EXTMEM_LEN)
        .with_names(regs::EXTMEM)
        .with_read_mirror(ICACHE_FREEZE, 1 << 0, 1 << 2)
        .with_read_mirror(DCACHE_FREEZE, 1 << 0, 1 << 2)
        .with_write_one_pulse(ICACHE_SYNC_CTRL, 1 << 0)
        .with_read_override(ICACHE_SYNC_CTRL, 1 << 1, 1 << 1)
        .with_write_one_pulse(DCACHE_SYNC_CTRL, 0b111)
        .with_read_override(DCACHE_SYNC_CTRL, 1 << 3, 1 << 3)
        .with_write_one_pulse(ICACHE_PRELOAD_CTRL, 1 << 0)
        .with_read_override(ICACHE_PRELOAD_CTRL, 1 << 1, 1 << 1)
        .with_write_one_pulse(DCACHE_PRELOAD_CTRL, 1 << 0)
        .with_read_override(DCACHE_PRELOAD_CTRL, 1 << 1, 1 << 1)
        .with_write_one_pulse(ICACHE_LOCK_CTRL, 0b11)
        .with_read_override(ICACHE_LOCK_CTRL, 1 << 2, 1 << 2)
        .with_write_one_pulse(DCACHE_LOCK_CTRL, 0b11)
        .with_read_override(DCACHE_LOCK_CTRL, 1 << 2, 1 << 2)
        .with_read_override(ICACHE_AUTOLOAD_CTRL, 1 << 3, 1 << 3)
        .with_read_override(DCACHE_AUTOLOAD_CTRL, 1 << 3, 1 << 3)
        .with_read_override(CACHE_STATE, (1 << 12) | 1, (1 << 12) | 1)
        .with_pac_grades()
}

/// `APB_CTRL`'s aperture: the generated table runs to `+0x3fc` (`date`).
pub const APB_CTRL_LEN: u32 = 0x400;

/// `APB_CTRL` at `0x6002_6000` — the census's three registers
/// (`front_end_mem_pd` `+0x9c`, `clkgate_force_on` `+0xa8`,
/// `mem_power_up` `+0xb0`), each a read-modify-write on `esp_hal::init`'s
/// inlined path with nothing read back that hardware would have changed.
/// Accept-and-remember at the PAC's resets is the whole model.
///
/// Unlike the classic's, this chip's `date` carries no revision bit: the
/// S3 keeps its wafer version in the eFuse block ([`super::efuse`]), so no
/// deviation is needed here.
pub fn apb_ctrl() -> RegFile {
    RegFile::new("APB_CTRL", APB_CTRL_LEN)
        .with_names(regs::APB_CTRL)
        .with_pac_grades()
}

/// An SPI controller's aperture: both generated tables run to `+0x3fc`
/// (`date`).
pub const SPI_LEN: u32 = 0x400;

/// `SPI0` at `0x6000_3000` — the cache's own flash port; **P06's block**,
/// accept-and-remember here. `esp_hal::init` writes `clock_gate` (`+0xe8`)
/// once and reads nothing back.
///
/// ⚠️ **SPI0 and SPI1 are separate PAC modules on this chip** with separate
/// tables, and their bases are the *other way round* from the C6's (SPI1
/// `0x6000_2000`, SPI0 `0x6000_3000` — `m6/notes.md` §3.0 row 13).
pub fn spi0() -> RegFile {
    RegFile::new("SPI0", SPI_LEN)
        .with_names(regs::SPI0)
        .with_pac_grades()
}

/// `SPI1` at `0x6000_2000` — the flash controller `esp_storage` drives;
/// **P06's block**, accept-and-remember here. `esp_hal::init` touches
/// `cmd` (`+0x00`), `w0` (`+0x58`) and `clock_gate` (`+0xe8`).
///
/// No exception is carried, and none could be: a flash read sets `cmd.usr`
/// (`+0x00`, bit 18) and spins until hardware clears it when the transfer
/// is done, and a block that remembers holds it set forever. That spin is
/// **P06**'s stop, on `engine::spi_flash`, and the classic's P3 ledger
/// carries the same sentence for the same reason.
pub fn spi1() -> RegFile {
    RegFile::new("SPI1", SPI_LEN)
        .with_names(regs::SPI1)
        .with_pac_grades()
}

/// The four RF-adjacent blocks' aperture: one register each, the highest at
/// `+0xf0` (`FE2.tx_interp_ctrl`).
pub const RF_LEN: u32 = 0x100;

/// `BB` at `0x6001_D000` — one register, `bbpd_ctrl` (`+0x54`), which
/// `esp_hal::init`'s inlined path read-modify-writes to power the baseband
/// down. Accept-and-remember; nothing on this chip's radio is modelled.
pub fn bb() -> RegFile {
    RegFile::new("BB", RF_LEN)
        .with_names(regs::BB)
        .with_pac_grades()
}

/// `NRX` at `0x6001_CC00` — one register, `nrxpd_ctrl` (`+0xd4`). See
/// [`bb`].
pub fn nrx() -> RegFile {
    RegFile::new("NRX", RF_LEN)
        .with_names(regs::NRX)
        .with_pac_grades()
}

/// `FE` at `0x6000_6000` — one register, `gen_ctrl` (`+0x90`). See [`bb`].
pub fn fe() -> RegFile {
    RegFile::new("FE", RF_LEN)
        .with_names(regs::FE)
        .with_pac_grades()
}

/// `FE2` at `0x6000_5000` — one register, `tx_interp_ctrl` (`+0xf0`). See
/// [`bb`].
pub fn fe2() -> RegFile {
    RegFile::new("FE2", RF_LEN)
        .with_names(regs::FE2)
        .with_pac_grades()
}

/// Every bit an accept block in this file answers with something other
/// than what was written, and the evidence for each.
///
/// `(block, offset, mask, value, why)`. A read override is a claim about
/// silicon; this is where each one is made citable, and the test below
/// holds that the blocks and the list agree both ways.
pub const READ_OVERRIDES: &[(&str, u32, u32, u32, &str)] = &[
    (
        "EXTMEM",
        0x088,
        1 << 1,
        1 << 1,
        "icache_sync_ctrl.icache_invalidate_done — 'the bit is used to indicate invalidate \
         operation is finished' (PAC); Cache_Invalidate_ICache_Items+0x3c (0x4004_E4FC) \
         spins `bnone a9, 2` on it after setting bit 0, and P03's scouting run stood there",
    ),
    (
        "EXTMEM",
        0x028,
        1 << 3,
        1 << 3,
        "dcache_sync_ctrl.dcache_sync_done — 'indicate clean/writeback/invalidate operation is \
         finished' (PAC); Cache_Invalidate_DCache_Items+0x3c (0x4004_E550) spins `bnone a9, 8`",
    ),
    (
        "EXTMEM",
        0x094,
        1 << 1,
        1 << 1,
        "icache_preload_ctrl.icache_preload_done (PAC, reset 0x02 already has it); \
         Cache_Disable_ICache+0x29 (0x4004_F2E1) spins `bnone a9, 2`",
    ),
    (
        "EXTMEM",
        0x040,
        1 << 1,
        1 << 1,
        "dcache_preload_ctrl.dcache_preload_done (PAC, reset 0x02 already has it); \
         Cache_Suspend_DCache+0x38 (0x4004_F464) spins `bnone a9, 2`",
    ),
    (
        "EXTMEM",
        0x07c,
        1 << 2,
        1 << 2,
        "icache_lock_ctrl.icache_lock_done — 'indicate unlock/lock operation is finished' \
         (PAC, reset 0x04 already has it); the same write-then-poll shape as the sync registers",
    ),
    (
        "EXTMEM",
        0x01c,
        1 << 2,
        1 << 2,
        "dcache_lock_ctrl.dcache_lock_done (PAC, reset 0x04 already has it); the same shape",
    ),
    (
        "EXTMEM",
        0x0a0,
        1 << 3,
        1 << 3,
        "icache_autoload_ctrl.icache_autoload_done — 'indicate autoload operation is finished' \
         (PAC, reset 0x08 already has it); the ROM's autoload suspend/resume pair reads it",
    ),
    (
        "EXTMEM",
        0x04c,
        1 << 3,
        1 << 3,
        "dcache_autoload_ctrl.dcache_autoload_done (PAC, reset 0x08 already has it)",
    ),
    (
        "EXTMEM",
        0x130,
        (1 << 12) | 1,
        (1 << 12) | 1,
        "cache_state.icache_state / dcache_state — '1: in idle state, 0: not in idle state' \
         (PAC); the PAC gives the register NO reset, and Cache_Disable_ICache+0x40 \
         (0x4004_F2F8) spins `extui a8, a8, 0, 12; bnei a8, 1` until the icache fsm is idle. A \
         cache with no operation in flight is idle; this is the one deviation from a PAC reset \
         in this file",
    ),
];

/// Every bit an accept block in this file answers as a **mirror** of
/// another bit of the same register — the `done` half of an operation the
/// guest waits for in both directions — with its evidence.
///
/// `(block, offset, from, to, why)`.
pub const READ_MIRRORS: &[(&str, u32, u32, u32, &str)] = &[
    (
        "EXTMEM",
        0x154,
        1 << 0,
        1 << 2,
        "icache_freeze.icache_freeze_done follows icache_freeze_ena: Cache_Freeze_ICache_Enable \
         (0x4004_E8A8) sets bit 0 then spins `bany a9, 4` until done is 1; \
         Cache_Freeze_ICache_Disable (0x4004_E8E8) clears bit 0 then spins until done is 0. No \
         constant satisfies both; the PAC's reset 0x04 (done set, ena clear) is what the \
         first traced run spun on",
    ),
    (
        "EXTMEM",
        0x150,
        1 << 0,
        1 << 2,
        "dcache_freeze.dcache_freeze_done follows dcache_freeze_ena: Cache_Freeze_DCache_Enable \
         (0x4004_E910) / _Disable (0x4004_E950), the same two polls; the run stood at \
         Cache_Freeze_DCache_Disable+0x1b (0x4004_E96B) for two emulated seconds on the PAC's \
         reset",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::periph::RegGrade;
    use lp_emu_esp_common::{Peripheral, Sandbox};

    /// The accept blocks, each with the generated table it is built from.
    fn every_block() -> Vec<(RegFile, lp_emu_esp_common::regnames::RegNames)> {
        vec![
            (sensitive(), regs::SENSITIVE),
            (extmem(), regs::EXTMEM),
            (apb_ctrl(), regs::APB_CTRL),
            (spi0(), regs::SPI0),
            (spi1(), regs::SPI1),
            (bb(), regs::BB),
            (nrx(), regs::NRX),
            (fe(), regs::FE),
            (fe2(), regs::FE2),
        ]
    }

    /// No accept block in this file carries a `with_reset`: every stored
    /// power-on value is the PAC's. (The read overrides are the other list,
    /// and they do not touch the stored word.)
    #[test]
    fn the_stored_resets_are_the_pacs_everywhere() {
        let mut unlisted = Vec::new();
        for (block, names) in every_block() {
            let name = Peripheral::name(&block);
            for (off, _) in names.entries {
                if *off >= block.len_bytes() {
                    continue;
                }
                let want = names.reset(*off).unwrap_or(0);
                let got = block.stored(*off);
                if got != want {
                    unlisted.push(format!(
                        "  {name}+{off:#05x} {}: reads {got:#010x}, the PAC says {want:#010x}",
                        names.name(*off).unwrap_or("?")
                    ));
                }
            }
        }
        assert!(
            unlisted.is_empty(),
            "these accept-block registers do not hold what the PAC says:\n{}",
            unlisted.join("\n")
        );
    }

    /// Every listed override is real — the block really answers it — and
    /// every override a block answers is listed with a reason.
    #[test]
    fn the_read_overrides_and_the_list_agree_both_ways() {
        for (name, off, mask, value, why) in READ_OVERRIDES {
            let (block, names) = every_block()
                .into_iter()
                .find(|(b, _)| Peripheral::name(b) == *name)
                .unwrap_or_else(|| panic!("READ_OVERRIDES names `{name}`, which is not a block"));
            assert!(why.len() > 40, "{name}+{off:#05x} has no reason");
            assert!(
                names.name(*off).is_some(),
                "{name}+{off:#05x} names no register"
            );
            assert_eq!(
                block.effective(*off) & mask,
                value & mask,
                "{name}+{off:#05x} does not read what the list says"
            );
            // A listed override is a pretence, and the grade table says so.
            assert_eq!(block.reg_grade(*off), Some(RegGrade::Modeled));
        }
        // The mirrors: `done` reads as `ena`, both ways.
        for (name, off, from, to, why) in READ_MIRRORS {
            let (mut block, names) = every_block()
                .into_iter()
                .find(|(b, _)| Peripheral::name(b) == *name)
                .unwrap_or_else(|| panic!("READ_MIRRORS names `{name}`, which is not a block"));
            assert!(why.len() > 40, "{name}+{off:#05x} has no reason");
            assert!(names.name(*off).is_some());
            let mut sb = Sandbox::new();
            sb.write(&mut block, *off, *from);
            assert_ne!(
                sb.read(&mut block, *off) & to,
                0,
                "{name}+{off:#05x}: enable → done"
            );
            sb.write(&mut block, *off, 0);
            assert_eq!(
                sb.read(&mut block, *off) & to,
                0,
                "{name}+{off:#05x}: disable → not done"
            );
            assert_eq!(block.reg_grade(*off), Some(RegGrade::Modeled));
        }
        // The other direction: a register that reads something other than
        // its stored word is on one of the two lists.
        for (block, names) in every_block() {
            let name = Peripheral::name(&block);
            for (off, _) in names.entries {
                if *off >= block.len_bytes() || block.effective(*off) == block.stored(*off) {
                    continue;
                }
                assert!(
                    READ_OVERRIDES
                        .iter()
                        .any(|(b, o, _, _, _)| *b == name && o == off)
                        || READ_MIRRORS
                            .iter()
                            .any(|(b, o, _, _, _)| *b == name && o == off),
                    "{name}+{off:#05x} reads {:#010x} over a stored {:#010x} and is on neither \
                     READ_OVERRIDES nor READ_MIRRORS",
                    block.effective(*off),
                    block.stored(*off)
                );
            }
        }
    }

    /// `Cache_Freeze_DCache_Enable` then `_Disable`, poll for poll — the
    /// sequence the first traced run never got out of.
    #[test]
    fn the_roms_freeze_enable_and_disable_both_complete() {
        let mut sb = Sandbox::new();
        let mut e = extmem();
        assert_eq!(e.reg_name(0x150), Some("dcache_freeze"));
        // Enable(mode=1): mode bit 1 set, then ena bit 0 set, poll done=1.
        let v = sb.read(&mut e, 0x150);
        sb.write(&mut e, 0x150, v | 2);
        let v = sb.read(&mut e, 0x150);
        sb.write(&mut e, 0x150, v | 1);
        assert_ne!(sb.read(&mut e, 0x150) & 4, 0, "`bnone a9, 4` falls through");
        // Disable: ena clear, poll done=0.
        let v = sb.read(&mut e, 0x150);
        sb.write(&mut e, 0x150, v & !1);
        assert_eq!(sb.read(&mut e, 0x150) & 4, 0, "`bany a10, 4` falls through");
        assert_eq!(sb.read(&mut e, 0x150) & 2, 2, "the mode bit is remembered");
    }

    /// Every block carries its generated names, so a trace line is readable
    /// without a second lookup, and every block is graded.
    #[test]
    fn the_blocks_carry_their_names_and_grades() {
        for (block, names) in every_block() {
            let (off, expected) = names.entries[0];
            assert_eq!(block.reg_name(off), Some(expected));
            assert!(
                block.reg_grade(off).is_some(),
                "`{}` publishes no grade table",
                Peripheral::name(&block)
            );
        }
    }

    /// `Cache_Invalidate_ICache_Items`, register for register: the address,
    /// the size, the operation bit, then the poll — which ends at once.
    #[test]
    fn the_roms_icache_invalidate_completes_against_the_done_bit() {
        let mut sb = Sandbox::new();
        let mut e = extmem();
        assert_eq!(e.reg_name(0x088), Some("icache_sync_ctrl"));
        assert_eq!(sb.read(&mut e, 0x088), 0x03, "PAC reset 0x01 plus done");
        sb.write(&mut e, 0x08c, 0x4200_0000); // icache_sync_addr
        sb.write(&mut e, 0x090, 0x0000_1000); // icache_sync_size
        let ctrl = sb.read(&mut e, 0x088);
        sb.write(&mut e, 0x088, ctrl | 1); // invalidate_ena
        let after = sb.read(&mut e, 0x088);
        assert_eq!(after & 1, 0, "the operation bit is consumed");
        assert_ne!(
            after & 2,
            0,
            "and done reads 1: `bnone a9, 2` falls through"
        );
        // The dcache's three operation bits, the same way.
        sb.write(&mut e, 0x028, 0b111);
        assert_eq!(sb.read(&mut e, 0x028), 1 << 3);
        // `Cache_Disable_ICache`'s last poll: the icache fsm is idle.
        assert_eq!(sb.read(&mut e, 0x130) & 0xfff, 1);
        assert_eq!((sb.read(&mut e, 0x130) >> 12) & 0xfff, 1);
    }

    /// The polarity note, held as an assertion: `icache_ctrl` bit 0 is an
    /// *enable*, and the PAC's reset for it is 0 (disabled), so
    /// `Cache_Enable_ICache`'s `or 1` is what turns it on.
    #[test]
    fn icache_enable_is_a_set_bit_not_a_shut_bit() {
        let mut sb = Sandbox::new();
        let mut e = extmem();
        assert_eq!(e.reg_name(0x060), Some("icache_ctrl"));
        assert_eq!(sb.read(&mut e, 0x060) & 1, 0, "disabled at reset");
        let v = sb.read(&mut e, 0x060);
        sb.write(&mut e, 0x060, v | 1);
        assert_eq!(sb.read(&mut e, 0x060) & 1, 1, "enabled — 1 means on here");
    }

    #[test]
    fn sensitive_remembers_what_cache_occupy_writes() {
        let mut sb = Sandbox::new();
        let mut s = sensitive();
        assert_eq!(sb.read(&mut s, 0x004), 0xff, "the PAC's reset");
        let v = sb.read(&mut s, 0x004);
        sb.write(&mut s, 0x004, (v & !0xf) | 0x1); // one bank to the icache
        assert_eq!(sb.read(&mut s, 0x004), 0xf1);
        assert_eq!(sb.read(&mut s, 0x014), 0x7ff);
    }
}
