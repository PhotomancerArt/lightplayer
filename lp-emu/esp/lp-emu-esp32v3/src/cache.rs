//! The classic's flash cache: the per-core MMU page tables, the enable and
//! flush bits, and **D4's cache-off fetch stop**.
//!
//! # Provenance: the mask ROM, not a datasheet
//!
//! Every number here is read off the vendored ROM ELF
//! (`lp-emu/esp/roms/esp32_rev300_rom.elf`) with
//! `xtensa-esp32-elf-objdump -d`. Five routines say the whole thing.
//!
//! **`mmu_init`** (`0x4000_95A4`) — where the tables are and how big they
//! are:
//!
//! ```text
//! 400095a7:  l32r  a9, (0x3ff12000)     ; the APP core's table
//! 400095aa:  l32r  a8, (0x3ff10000)     ; the PRO core's table
//! 400095ad:  sext  a10, a2, 15
//! 400095b0:  movnez a8, a9, a10         ; cpu_no != 0 ? APP : PRO
//! 400095b3:  l32r  a12, (0x2000)        ; 8 KiB …
//! 400095bc:  call8 <memset>             ; … cleared to zero
//! ```
//!
//! so each core's table is **`0x2000` bytes = 2048 `u32` entries**, and reset
//! leaves whatever was there — the ROM clears it, and a direct load has to do
//! the clearing itself (that is P7's, with the fill).
//!
//! **`cache_flash_mmu_set(cpu_no, pid, vaddr, paddr, psize, num)`**
//! (`0x4000_95E0`) — the entry format and the index arithmetic:
//!
//! ```text
//! 400095e3:  movi.n a8, 64
//! 400095e8:  quos  a8, a8, a6          ; 64 / psize(KiB)
//! 400095eb:  addi.n a8, a8, -1
//! 400095f0:  sra   a8, a9              ; 0xffff >> (64/psize - 1)  = the
//! 400095f6:  and   a8, a8, a9          ;   alignment mask on vaddr|paddr
//! 400095f9:  bnez  a8, +0x1fc          ; → return 1, misaligned
//! 400095fc:  beqi  a6, 64, …           ; page mode 0, shift 16
//! 400095ff:  beqi  a6, 32, …           ; page mode 1, shift 15
//! 40009606:  beqi  a6, 16, …           ; page mode 2, shift 14
//! …
//! 4000978c:  add.n  a5, a6, a4         ; index = start + i
//! 4000978e:  add.n  a13, a6, a10       ; content = (paddr >> shift) + i
//! 40009790:  addx4  a5, a5, a3         ; &table[index]
//! 40009796:  s32i.n a13, a5, 0
//! ```
//!
//! ⚠️ **The entry is a bare physical page number.** The ROM sets **no valid
//! bit** and tests none: `cache_flash_mmu_set` stores `(paddr >> shift) + i`
//! and nothing else, `mmu_init` clears the table to zero, and
//! `ets_unpack_flash_code` (`0x4000_7018`) calls the two of them and never
//! writes a marker of its own. So on this chip a zeroed entry means *flash
//! page 0*, not "unmapped" — the opposite of the C6, whose
//! `Cache_MSPI_MMU_Set` ORs in bit 9 (`lp-emu-esp32c6/src/cache.rs:24-27`).
//! [`FlashMmu::translate`] therefore answers `None` only for an address
//! outside the window, and P7 — which owns the fill and is the first code
//! that can tell a mapped page from an unwritten one — decides what an
//! unwritten entry serves. Anything else would be a datasheet claim, and the
//! rule for this file is the ROM or nothing.
//!
//! The **windows and their index bases** come from the same routine's
//! `pid < 2` arm, which walks four `0x400000`-sized windows in order:
//!
//! | virtual window | index base | ROM |
//! |---|---:|---|
//! | `0x3F40_0000..0x3F80_0000` (DROM) | 0 | `40009627..40009644` |
//! | `0x400D_0000..0x4040_0000` (IROM) | 64 | `40009648..4000966e` (`addi a4, a4, 64`) |
//! | `0x4040_0000..0x4080_0000` | 128 | `4000967a..40009697` (`movi a3, 128`) |
//! | `0x4080_0000..0x40C0_0000` | 192 | `400096a5..400096c5` (`movi a3, 192`) |
//!
//! `index = ((vaddr & (0x3F_FFFF >> page_mode)) >> shift) + base`, four
//! windows of 64 entries each: **256 flash entries** out of the 2048-entry
//! table. The rest belong to `cache_sram_mmu_set` (`0x4000_97F4`), whose
//! bases start at `0x400` — the internal-SRAM MMU, which this machine does
//! not model.
//!
//! **The page mode lives in `*_cache_ctrl1` bits 10:9**, and two independent
//! sources say so: `cache_flash_mmu_set`'s tail
//!
//! ```text
//! 400097ad:  slli  a9, a9, 9           ; page mode << 9
//! 400097b0:  movi  a5, 0xfffff9ff      ; clearing bits 10:9
//! 400097c3:  s32i.n a9, a4, 0          ; → DPORT+0x44 (pro) / +0x5c (app)
//! ```
//!
//! and the PAC's own field, `pro_cmmu_flash_page_mode`, "Bits 9:10". Bits 5:0
//! of the same register are the six window masks
//! (`pro_cache_mask_iram0`…`_opsdram`), which the ROM sets to all-ones while
//! it rewrites the table and restores afterwards
//! (`40009760: movi.n a13, 63`).
//!
//! **`Cache_Read_Enable`** (`0x4000_9A84`) and **`Cache_Read_Disable`**
//! (`0x4000_9AB8`) — the bit D4 is defined against:
//!
//! ```text
//! Cache_Read_Enable:   SPI0+0x50 |= 1 ; DPORT+0x40 (pro) / +0x58 (app) |= 8
//! Cache_Read_Disable:  DPORT+0x40 / +0x58 &= ~8   (movi.n a10, -9)
//!                      then, if the OTHER core's bit 3 is clear,
//!                      spin on SPI0+0xf8 and clear SPI0+0x50 bit 0
//! ```
//!
//! Bit 3 is `pro_cache_enable` in the PAC — the same bit, from the other
//! side.
//!
//! **`Cache_Flush`** (`0x4000_9A14`) — the flush handshake the guest polls:
//!
//! ```text
//! 40009a1b:  ctrl &= ~0x10             ; flush_ena low first
//! 40009a30:  ctrl |=  0x10             ; then high
//! 40009a3a:  movi.n a9, 32
//! 40009a3c:  l32i / bnone a10, a9, →   ; spin until flush_done (bit 5) is 1
//! 40009a74:  ctrl &= ~0x10
//! ```
//!
//! so `flush_ena` (bit 4) setting is what raises `flush_done` (bit 5). This
//! model raises it **in the same store**, which is the honest shape for a
//! machine with no cache array to walk: there is nothing to flush, and a
//! `flush_done` that took N cycles would be a number nobody measured.
//!
//! # D4 — the cache-off fetch stop
//!
//! On silicon a core that fetches through the flash window with its cache
//! disabled **stalls until the cache comes back**, and nothing observes it
//! except a crash or a watchdog. This emulator refuses instead: the window is
//! ordinary RAM here, so the guest would sail through a bug that hangs a
//! board. That asymmetry is the whole point (plan D4) — it is the milestone's
//! one finding that only a product could have produced.
//!
//! **How the check is armed, and why it costs nothing when it is not.** The
//! trigger is a *guest access address*, which only the bus sees, so the check
//! rides [`lp_emu_core::cycle_model::MemoryCost`] — the one per-access seam
//! the chip crate already has. It is installed **only while some core's cache
//! is off**, which on this chip is a handful of microseconds inside a flash
//! write: [`ClassicCache::watch_wanted`] says when, the machine installs and
//! removes it, and a run whose guest never disables the cache pays one
//! `Option` test per access, exactly as before. Every cycle it charges is
//! **0**, so no cycle count moves.
//!
//! `MemoryCost` sees the address and not the pc, so while the watch is armed
//! the machine hands the hart **one instruction at a time** and reads the pc
//! and cycle from the hart itself. That is precise rather than approximate,
//! and it is affordable precisely because the armed window is short.
//!
//! **`--cache-off-fetch permit` turns the check off**, rather than running it
//! and swallowing the answer: with nothing to report there is nothing to arm,
//! the machine says once at `info` that the cache went off and who did it,
//! and the run keeps its full-speed slices. That is what "claims nothing"
//! means.
//!
//! **Two things that are not exceptions and must not arm it.** The IDF
//! bootloader executes from `0x4007_8000`, which is SRAM0 and not a flash
//! window, so it never triggers (P7 confirms on the ROM-up path). And a
//! **host-side** fill — P7 copying flash bytes into the window — is not a
//! guest access: it goes through `SocBus::load_image`, which never reaches
//! `MemoryCost`, and the machine's own `peek_word`/`poke_word` mark
//! themselves with [`ClassicCache::set_host_access`].

use std::sync::{Arc, Mutex};

use lp_emu_core::cycle_model::MemoryCost;
use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::SocBus;

use crate::memmap;

/// Entries in one core's MMU table: `0x2000` bytes, which `mmu_init`
/// clears in one `memset`.
pub const MMU_ENTRIES: usize = 2048;

/// The bytes one core's table occupies. `FLASH_MMU_APP - FLASH_MMU_PRO`.
pub const MMU_TABLE_LEN: u32 = (MMU_ENTRIES * 4) as u32;

/// What an **unmapped** flash-MMU entry holds: `0x100`.
///
/// ⚠️ **The classic's entry has no valid bit, so zero is a mapping.** P7
/// found the first half of that — a write that does not change an entry
/// still maps the page, because `0` over `0` *is* a mapping of flash page 0 —
/// and P8's direct-vs-ROM-up cross-check found the other half: after the
/// mask ROM's own `mmu_init`, every entry the boot did not map reads
/// **`0x100`**, not `0`. The direct loader left them at `0`, which points a
/// stray read in an unmapped DROM page at the first 64 KiB of the chip
/// instead of at nothing.
///
/// The value is **not** transcribed from a datasheet. It is what the real
/// ROM wrote, read back out of the ROM-up walk's own table by
/// `rom_up_boot.rs::rom_up_and_direct_load_agree_on_what_the_app_sees` —
/// entry 5 of the PRO table, ROM-up `0x100` against the direct load's `0x0`,
/// which is the assertion that produced this constant. It is also 256 pages
/// of 64 KiB = 16 MiB, past the end of any part this machine models, which
/// is how an entry with no valid bit says "nothing here".
pub const MMU_UNMAPPED: u32 = 0x100;

/// How many of [`MMU_ENTRIES`] the **flash** MMU uses: four windows of 64.
/// Above them, from index `0x400`, is `cache_sram_mmu_set`'s territory.
pub const FLASH_MMU_ENTRIES: u32 = 256;

/// The two cores.
pub const CORES: usize = 2;

/// `pro_cache_ctrl.pro_cache_enable` / `app_cache_ctrl.app_cache_enable`,
/// **bit 3** — the PAC's field, and the bit `Cache_Read_Enable` ORs in and
/// `Cache_Read_Disable` masks out.
pub const CACHE_ENABLE: u32 = 1 << 3;

/// `*_cache_flush_ena`, bit 4. Writable.
pub const CACHE_FLUSH_ENA: u32 = 1 << 4;

/// `*_cache_flush_done`, bit 5. Read-only in the PAC, and this model raises
/// it the moment `flush_ena` is set. See the module docs.
pub const CACHE_FLUSH_DONE: u32 = 1 << 5;

/// `*_cmmu_flash_page_mode`, bits 10:9 of `*_cache_ctrl1`.
pub const FLASH_PAGE_MODE_SHIFT: u32 = 9;

/// The mask of [`FLASH_PAGE_MODE_SHIFT`]'s two bits.
pub const FLASH_PAGE_MODE_MASK: u32 = 0b11 << FLASH_PAGE_MODE_SHIFT;

/// The four windows `cache_flash_mmu_set` maps, in the order it tests them:
/// `(base, index base)`. Each is `0x40_0000` long at page mode 0.
///
/// The second window's *index* arithmetic is `0x4000_0000`-relative, but the
/// ROM **rejects anything at or below `0x400C_FFFF`** before it reaches that
/// arithmetic (`40009648: l32r a3, (0x400cffff)`; `bltu a3, a4`; else return
/// 5), so the reachable part starts at `0x400D_0000` — [`memmap::IROM_BASE`],
/// which is the low bound written here. That is why `0x400D_0000` is entry
/// **77** and not entry 64, and why an SRAM0 address inside
/// `0x4000_0000..0x400D_0000` resolves to nothing rather than to a plausible
/// entry.
pub const FLASH_WINDOWS: [(u32, u32, u32); 4] = [
    (memmap::DROM_BASE, memmap::DROM_BASE + 0x0040_0000, 0),
    (memmap::IROM_BASE, 0x4040_0000, 64),
    (0x4040_0000, 0x4080_0000, 128),
    (0x4080_0000, 0x40C0_0000, 192),
];

/// Which flash window an address falls in, for the stop's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// `0x400D_0000..`, instruction.
    Irom,
    /// `0x3F40_0000..`, data.
    Drom,
}

impl std::fmt::Display for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Window::Irom => "IROM",
            Window::Drom => "DROM",
        })
    }
}

/// What `--cache-off-fetch` chooses. **`Stop` is the default and is what
/// every gate run uses** (M3's acceptance criterion 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CacheOffPolicy {
    #[default]
    Stop,
    /// Do not check at all. See the module docs: with nothing to report
    /// there is nothing to arm, and the run keeps its full-speed slices.
    Permit,
}

impl CacheOffPolicy {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "stop" => Ok(CacheOffPolicy::Stop),
            "permit" => Ok(CacheOffPolicy::Permit),
            other => Err(format!(
                "unknown --cache-off-fetch `{other}` (expected stop or permit)"
            )),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            CacheOffPolicy::Stop => "stop",
            CacheOffPolicy::Permit => "permit",
        }
    }
}

/// One guest access through a flash window by a core whose cache is off.
///
/// `pc`, `cycle` and `symbol` are filled in by the machine, which is the only
/// thing that can see the hart; the watch records the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheOffAccess {
    pub core: usize,
    pub addr: u32,
    pub window: Window,
    /// `true` for an instruction fetch, `false` for a data read.
    pub fetch: bool,
    /// When this core's cache was last turned off, and the pc of the store
    /// that did it — `None` when nothing ever turned it on: the PAC's reset
    /// leaves `*_cache_enable` clear, and on silicon the bootloader's
    /// `Cache_Read_Enable` is what sets it.
    pub disabled_at: Cycles,
    pub disabled_by: Option<u32>,
}

/// The virtual base each of [`FLASH_WINDOWS`]' index bases counts from.
///
/// The ROM's index arithmetic is `(vaddr & mask) >> shift`, which is
/// `0x40_0000`-relative — so window 1's entry 64 is virtual `0x4000_0000`
/// even though the routine refuses anything below `0x400D_0000`. These are
/// the four bases that arithmetic implies, and [`FlashMmu::entry_vaddr`]
/// inverts through them and then **checks the round trip**, so an index with
/// no reachable virtual address (64..=76, which would otherwise land inside
/// SRAM0) answers `None` rather than a plausible address.
pub const WINDOW_VBASE: [u32; 4] = [memmap::DROM_BASE, 0x4000_0000, 0x4040_0000, 0x4080_0000];

/// The classic's flash MMU page tables — one per core, 2048 entries.
#[derive(Clone)]
pub struct FlashMmu {
    tables: [[u32; MMU_ENTRIES]; CORES],
    /// `(core, index)` pairs whose backing bytes have not been copied into
    /// the window since the entry last moved. The fill drains it.
    dirty: Vec<(usize, u32)>,
}

impl Default for FlashMmu {
    fn default() -> Self {
        Self::new()
    }
}

impl FlashMmu {
    /// Both tables zeroed — which is what `mmu_init`'s `memset` leaves, not
    /// what reset leaves. Reset's contents are undefined and the ROM clears
    /// them; a direct load has to do the same, and that is P7's job with the
    /// fill.
    pub fn new() -> Self {
        Self {
            tables: [[0; MMU_ENTRIES]; CORES],
            dirty: Vec::new(),
        }
    }

    pub fn entry(&self, core: usize, index: usize) -> u32 {
        self.tables
            .get(core)
            .and_then(|t| t.get(index))
            .copied()
            .unwrap_or(0)
    }

    pub fn set_entry(&mut self, core: usize, index: usize, value: u32) {
        if let Some(slot) = self.tables.get_mut(core).and_then(|t| t.get_mut(index)) {
            *slot = value;
        }
        self.mark_dirty(core, index as u32);
    }

    /// This entry's page has to be copied out of the chip again.
    pub fn mark_dirty(&mut self, core: usize, index: u32) {
        if index >= FLASH_MMU_ENTRIES {
            // Above the flash half is `cache_sram_mmu_set`'s territory,
            // which this machine does not model and does not fill.
            return;
        }
        if !self.dirty.contains(&(core, index)) {
            self.dirty.push((core, index));
        }
    }

    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// The entries needing a refill, and clear the list.
    pub fn take_dirty(&mut self) -> Vec<(usize, u32)> {
        core::mem::take(&mut self.dirty)
    }

    /// Mark every entry that maps the flash page `flash_offset` falls in —
    /// what a flash write under the window means.
    ///
    /// Returns the entry indices it marked, so the caller can forget what
    /// those window pages were holding — the bytes under them moved.
    pub fn invalidate_page_at(&mut self, flash_offset: u32, page_mode: u8) -> Vec<u32> {
        let page = flash_offset >> Self::shift(page_mode);
        let mut hits = Vec::new();
        for core in 0..CORES {
            for index in 0..FLASH_MMU_ENTRIES {
                if self.entry(core, index as usize) == page {
                    hits.push((core, index));
                }
            }
        }
        for (core, index) in &hits {
            self.mark_dirty(*core, *index);
        }
        hits.into_iter().map(|(_, index)| index).collect()
    }

    /// Every entry in the flash half of both tables, marked for a refill.
    /// What a snapshot restore and a page-mode change both mean: whatever
    /// the window holds now, the table just changed under it.
    pub fn mark_all_dirty(&mut self) {
        for core in 0..CORES {
            for index in 0..FLASH_MMU_ENTRIES {
                self.mark_dirty(core, index);
            }
        }
    }

    /// `cache_flash_mmu_set`'s shift for a page mode: 16 at mode 0 (64 KiB),
    /// one less per mode.
    pub const fn shift(page_mode: u8) -> u32 {
        16 - (page_mode & 3) as u32
    }

    /// Bytes per page: 64 KiB >> the page mode.
    pub const fn page_len(page_mode: u8) -> u32 {
        0x1_0000 >> (page_mode & 3)
    }

    /// The table index a virtual address falls in, or `None` for an address
    /// outside all four windows. The ROM's own arithmetic — see the module
    /// docs' table.
    pub fn entry_index(vaddr: u32, page_mode: u8) -> Option<u32> {
        let shift = Self::shift(page_mode);
        let mask = 0x003F_FFFFu32 >> (page_mode & 3);
        for (lo, hi, base) in FLASH_WINDOWS {
            if vaddr >= lo && vaddr < hi {
                return Some(((vaddr & mask) >> shift) + base);
            }
        }
        None
    }

    /// The inverse of [`entry_index`](Self::entry_index): the virtual base
    /// address an entry serves, or `None` when it serves none.
    ///
    /// The round trip is checked rather than assumed. Entries **64..=76**
    /// have no reachable virtual address — the ROM refuses anything at or
    /// below `0x400C_FFFF` before it reaches the second window's arithmetic
    /// — and the addresses they would otherwise name (`0x4000_0000`…
    /// `0x400C_0000`) run straight through SRAM0, which is real, executable
    /// guest memory holding the vectors and the IDF bootloader. A fill that
    /// wrote there would overwrite the code it was running.
    pub fn entry_vaddr(index: u32, page_mode: u8) -> Option<u32> {
        let page_len = Self::page_len(page_mode);
        for (k, (_, _, base)) in FLASH_WINDOWS.iter().enumerate() {
            if index < *base || index >= base + 64 {
                continue;
            }
            let vaddr = WINDOW_VBASE[k].checked_add((index - base).checked_mul(page_len)?)?;
            return (Self::entry_index(vaddr, page_mode) == Some(index)).then_some(vaddr);
        }
        None
    }

    /// **The address path.** A virtual address in a flash window to the flash
    /// byte offset its entry names, or `None` for an address outside the
    /// windows.
    ///
    /// One function on purpose (the C6's rule): a later `t2` rung hangs
    /// cache-miss wait states off exactly this lookup. And it answers `None`
    /// only for an out-of-window address — the classic's entry carries no
    /// valid bit, so "unmapped" is not a thing this table can say. See the
    /// module docs.
    pub fn translate(&self, core: usize, vaddr: u32, page_mode: u8) -> Option<u32> {
        let index = Self::entry_index(vaddr, page_mode)?;
        let page = self.entry(core, index as usize);
        let shift = Self::shift(page_mode);
        Some((page << shift) | (vaddr & (Self::page_len(page_mode) - 1)))
    }
}

/// The cache-control state DPORT holds, plus the MMU tables and D4's watch
/// slot. Shared between the DPORT view, the MMU-table view and the machine.
pub struct ClassicCache {
    pub mmu: FlashMmu,
    /// `pro_cache_ctrl` @`+0x40` / `app_cache_ctrl` @`+0x58`. PAC reset
    /// `0x0000_0010`: the cache is **off** at reset and `flush_ena` is set.
    ctrl: [u32; CORES],
    /// `pro_cache_ctrl1` @`+0x44` / `app_cache_ctrl1` @`+0x5C`. PAC reset
    /// `0x0000_08ff`.
    ctrl1: [u32; CORES],
    /// When each core's cache was last turned off, and by which pc. Both
    /// start at the reset that turned it off in the first place.
    disabled_at: [Cycles; CORES],
    disabled_by: [Option<u32>; CORES],
    policy: CacheOffPolicy,
    /// Which flash page each **PRO-core** window page already holds, or
    /// `None` when it has never been filled. The dirty mark is unconditional
    /// — a write that does not change an entry still maps the page, because
    /// the classic's entry has no valid bit — and this is what keeps the
    /// *copy* from being unconditional too: `mmu_init` clears 2048 entries,
    /// and re-copying a hundred-odd 64 KiB pages that already hold the right
    /// bytes would be seven megabytes of memcpy per call for no change.
    filled: [Option<u32>; FLASH_MMU_ENTRIES as usize],
    /// The first offence since the machine last took one.
    offence: Option<CacheOffAccess>,
    /// Set around a host-side peek or poke, which goes through the bus's
    /// decode and would otherwise look like a guest access.
    host_access: bool,
}

/// The tables and the cache state, shared between the register views and the
/// machine. The C6's `CacheHandle` precedent.
pub type CacheHandle = Arc<Mutex<ClassicCache>>;

/// `pro_cache_ctrl`'s PAC reset (`regs::DPORT`'s `resets`: `(0x040,
/// 0x00000010)`), and `app_cache_ctrl`'s (`(0x058, 0x00000010)`).
pub const CACHE_CTRL_RESET: u32 = 0x0000_0010;

/// `pro_cache_ctrl1`'s and `app_cache_ctrl1`'s PAC reset (`(0x044,
/// 0x000008ff)` / `(0x05c, 0x000008ff)`).
pub const CACHE_CTRL1_RESET: u32 = 0x0000_08ff;

impl Default for ClassicCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassicCache {
    pub fn new() -> Self {
        Self {
            mmu: FlashMmu::new(),
            ctrl: [CACHE_CTRL_RESET; CORES],
            ctrl1: [CACHE_CTRL1_RESET; CORES],
            disabled_at: [0; CORES],
            disabled_by: [None; CORES],
            policy: CacheOffPolicy::default(),
            filled: [None; FLASH_MMU_ENTRIES as usize],
            offence: None,
            host_access: false,
        }
    }

    pub fn handle() -> CacheHandle {
        Arc::new(Mutex::new(Self::new()))
    }

    pub fn policy(&self) -> CacheOffPolicy {
        self.policy
    }

    pub fn set_policy(&mut self, policy: CacheOffPolicy) {
        self.policy = policy;
    }

    /// `*_cache_ctrl` as the guest reads it.
    pub fn ctrl(&self, core: usize) -> u32 {
        self.ctrl.get(core).copied().unwrap_or(0)
    }

    /// `*_cache_ctrl1` as the guest reads it.
    pub fn ctrl1(&self, core: usize) -> u32 {
        self.ctrl1.get(core).copied().unwrap_or(0)
    }

    /// Write `*_cache_ctrl`. `at` and `pc` are the cycle and the pc of the
    /// store, which the stop's message needs when the write clears
    /// [`CACHE_ENABLE`].
    ///
    /// `flush_done` (bit 5) follows `flush_ena` (bit 4) in the same store —
    /// see the module docs — and is otherwise not writable.
    pub fn write_ctrl(&mut self, core: usize, value: u32, at: Cycles, pc: u32) {
        let Some(slot) = self.ctrl.get_mut(core) else {
            return;
        };
        let was_on = *slot & CACHE_ENABLE != 0;
        let mut next = value & !CACHE_FLUSH_DONE;
        if next & CACHE_FLUSH_ENA != 0 {
            next |= CACHE_FLUSH_DONE;
        }
        *slot = next;
        let now_on = next & CACHE_ENABLE != 0;
        if was_on && !now_on {
            self.disabled_at[core] = at;
            self.disabled_by[core] = Some(pc);
            log::debug!("cache: core {core}'s read cache disabled at cycle {at} by pc {pc:#010x}");
        }
    }

    /// Write `*_cache_ctrl1`. A change to the page-mode field
    /// ([`FLASH_PAGE_MODE_MASK`]) re-points every entry, so it marks the
    /// whole flash half of that core's table for a refill.
    pub fn write_ctrl1(&mut self, core: usize, value: u32) {
        let was = self.ctrl1(core);
        if let Some(slot) = self.ctrl1.get_mut(core) {
            *slot = value;
        }
        if (was ^ value) & FLASH_PAGE_MODE_MASK != 0 {
            // Every entry means something else now, and every window page
            // holds bytes from the old page size.
            self.forget_fills();
            self.mmu.mark_all_dirty();
        }
    }

    /// Is `core`'s read cache enabled?
    pub fn enabled(&self, core: usize) -> bool {
        self.ctrl(core) & CACHE_ENABLE != 0
    }

    /// `*_cmmu_flash_page_mode`, bits 10:9 of `*_cache_ctrl1`.
    pub fn page_mode(&self, core: usize) -> u8 {
        ((self.ctrl1(core) & FLASH_PAGE_MODE_MASK) >> FLASH_PAGE_MODE_SHIFT) as u8
    }

    /// [`FlashMmu::translate`] with this core's own page mode.
    pub fn translate(&self, core: usize, vaddr: u32) -> Option<u32> {
        self.mmu.translate(core, vaddr, self.page_mode(core))
    }

    /// When `core`'s cache was last turned off, and the pc that did it.
    pub fn disabled(&self, core: usize) -> (Cycles, Option<u32>) {
        (
            self.disabled_at.get(core).copied().unwrap_or(0),
            self.disabled_by.get(core).copied().flatten(),
        )
    }

    /// Should the machine arm D4's watch for `core`? Only under
    /// [`CacheOffPolicy::Stop`], and only while **that core's** cache is off.
    ///
    /// Per core, not "any core": core 1's cache is off for the whole of M3
    /// because core 1 never runs, and arming on that would single-step every
    /// run for a core that issues no accesses.
    pub fn watch_wanted(&self, core: usize) -> bool {
        self.policy == CacheOffPolicy::Stop && !self.enabled(core)
    }

    /// See [`ClassicCache`]'s `host_access`.
    pub fn set_host_access(&mut self, on: bool) {
        self.host_access = on;
    }

    /// Does the PRO core's window page `index` already hold flash page
    /// `entry`? See [`ClassicCache`]'s `filled`.
    pub fn already_filled(&self, index: u32, entry: u32) -> bool {
        self.filled
            .get(index as usize)
            .copied()
            .flatten()
            .is_some_and(|held| held == entry)
    }

    /// Record that it does now.
    pub fn note_filled(&mut self, index: u32, entry: u32) {
        if let Some(slot) = self.filled.get_mut(index as usize) {
            *slot = Some(entry);
        }
    }

    /// Forget what one window page holds — the bytes under it moved.
    pub fn forget_fill(&mut self, index: u32) {
        if let Some(slot) = self.filled.get_mut(index as usize) {
            *slot = None;
        }
    }

    /// Forget what every window page holds — a snapshot restore, or anything
    /// that changed the bytes under the whole table.
    pub fn forget_fills(&mut self) {
        self.filled = [None; FLASH_MMU_ENTRIES as usize];
    }

    /// The watch's entry point: one guest access, from the core whose slices
    /// the machine is running.
    fn note_access(&mut self, core: usize, addr: u32, fetch: bool) {
        if self.host_access || self.offence.is_some() || self.enabled(core) {
            return;
        }
        let in_span = |base: u32, len: u32| addr >= base && addr - base < len;
        let window = if in_span(memmap::IROM_BASE, memmap::IROM_LEN) {
            Window::Irom
        } else if in_span(memmap::DROM_BASE, memmap::DROM_LEN) {
            Window::Drom
        } else {
            return;
        };
        // A fetch from DROM or a data read from IROM is still an access
        // through a flash window with the cache off, and is reported as
        // whichever window the address is in.
        let (disabled_at, disabled_by) = self.disabled(core);
        self.offence = Some(CacheOffAccess {
            core,
            addr,
            window,
            fetch,
            disabled_at,
            disabled_by,
        });
    }

    /// The offence, if there is one, cleared.
    pub fn take_offence(&mut self) -> Option<CacheOffAccess> {
        self.offence.take()
    }

    /// Snapshot: the control words and the tables. Cycle/pc provenance rides
    /// along so a restored machine's stop message is still true.
    pub fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(CORES * (MMU_ENTRIES * 4 + 8 + 8 + 4));
        for core in 0..CORES {
            out.extend_from_slice(&self.ctrl[core].to_le_bytes());
            out.extend_from_slice(&self.ctrl1[core].to_le_bytes());
            out.extend_from_slice(&self.disabled_at[core].to_le_bytes());
            out.push(u8::from(self.disabled_by[core].is_some()));
            out.extend_from_slice(&self.disabled_by[core].unwrap_or(0).to_le_bytes());
            for entry in &self.mmu.tables[core] {
                out.extend_from_slice(&entry.to_le_bytes());
            }
        }
        out
    }

    pub fn load_state(&mut self, bytes: &[u8]) {
        let per_core = 4 + 4 + 8 + 1 + 4 + MMU_ENTRIES * 4;
        if bytes.len() != CORES * per_core {
            log::warn!(
                "ClassicCache::load_state: {} bytes, expected {}; ignored",
                bytes.len(),
                CORES * per_core
            );
            return;
        }
        for (core, chunk) in bytes.chunks_exact(per_core).enumerate() {
            let w = |at: usize| u32::from_le_bytes(chunk[at..at + 4].try_into().expect("4 bytes"));
            self.ctrl[core] = w(0);
            self.ctrl1[core] = w(4);
            self.disabled_at[core] = u64::from_le_bytes(chunk[8..16].try_into().expect("8 bytes"));
            self.disabled_by[core] = (chunk[16] != 0).then(|| w(17));
            for (i, entry) in chunk[21..].chunks_exact(4).enumerate() {
                self.mmu.tables[core][i] = u32::from_le_bytes(entry.try_into().expect("4 bytes"));
            }
        }
        // Whatever the window holds now, the table just changed under it.
        self.forget_fills();
        self.mmu.mark_all_dirty();
    }
}

/// **The fill.** Copy every page the table marked dirty out of the flash
/// chip and into the RAM region behind its window. Returns how many pages
/// were copied.
///
/// Called once by the builder after a direct load programs the table, and
/// from the machine's slice loop whenever the table, the page mode or the
/// flash under a mapped page moved.
///
/// # Why the window is a fill and not a per-access translation
///
/// Instruction fetch stays a plain RAM read, which is what keeps this
/// machine fast enough to be used. The consequence is that the model is
/// **stricter than silicon about staleness**: a real cache serves stale
/// lines until it is flushed, and this one never does. Stated, not hidden.
///
/// # What an entry with no valid bit means here
///
/// The classic's entry is a bare physical page number — `cache_flash_mmu_set`
/// sets no marker and tests none, and `mmu_init` clears the table to zero
/// (the module docs carry both disassemblies). So a zeroed entry means
/// **flash page 0**, and this fill serves it: refusing would be inventing a
/// valid bit the silicon does not have. What keeps `mmu_init`'s 8 KiB
/// `memset` from costing 2048 page copies is that
/// [`crate::periph::flash_mmu::FlashMmuView`] only marks an entry dirty when
/// the write **changes** it, so zeros over zeros mark nothing.
///
/// A dirty entry whose window is not backed by a RAM region on this machine
/// — the two windows above `0x4040_0000`, which `memmap` does not map — is
/// logged and skipped, as is one naming a flash page past the end of the
/// chip.
pub fn fill(bus: &mut SocBus, flash: &crate::flash::FlashHandle, cache: &CacheHandle) -> usize {
    // `(core, entry index, the page number stored there, that core's page
    // mode)`, gathered before the flash lock is taken so the two are never
    // held at once.
    let work: Vec<(usize, u32, u32, u8)> = {
        let mut c = cache.lock().expect("cache poisoned");
        let dirty = c.mmu.take_dirty();
        if dirty.is_empty() {
            return 0;
        }
        dirty
            .into_iter()
            .map(|(core, index)| {
                (
                    core,
                    index,
                    c.mmu.entry(core, index as usize),
                    c.page_mode(core),
                )
            })
            .collect()
    };

    let flash = flash.lock().expect("flash poisoned");
    let mut filled = 0;
    for (core, index, entry, page_mode) in work {
        // ⚠️ **One window, one memory.** `SocBus` holds the flash windows in
        // a single arena, so there is no per-core copy for the APP core's
        // table to fill: a fill from core 1 would overwrite core 0's view of
        // the same address. M3 runs core 0 only (Q5), so the fill is core
        // 0's — and a core-1 entry that *disagrees* with core 0's is said
        // out loud, because that is the case a per-core window would be
        // needed for and M4 is where it would land.
        if core != 0 {
            let pro = cache
                .lock()
                .expect("cache poisoned")
                .mmu
                .entry(0, index as usize);
            if pro != entry {
                log::warn!(
                    "cache: the APP core's entry {index} maps flash page {entry:#x} where the \
                     PRO core's maps {pro:#x}; this machine has one window behind both tables \
                     and serves the PRO core's. See crate::cache::fill."
                );
            }
            continue;
        }
        let page_len = FlashMmu::page_len(page_mode);
        let Some(vaddr) = FlashMmu::entry_vaddr(index, page_mode) else {
            log::debug!(
                "cache: core {core} entry {index} names no virtual address on this chip                  (the ROM refuses the second window below 0x400d0000); not filled"
            );
            continue;
        };
        if cache
            .lock()
            .expect("cache poisoned")
            .already_filled(index, entry)
        {
            continue;
        }
        let paddr = entry << FlashMmu::shift(page_mode);
        let Some(bytes) = flash.peek(paddr, page_len) else {
            log::warn!(
                "cache: core {core} entry {index} ({vaddr:#010x}) maps flash {paddr:#010x},                  past the {:#x}-byte chip; the page is left as it was",
                flash.len()
            );
            continue;
        };
        if let Err(e) = bus.load_image(vaddr, bytes) {
            log::debug!(
                "cache: core {core} entry {index} ({vaddr:#010x}) has no RAM region behind it                  on this machine; not filled ({e:?})"
            );
            continue;
        }
        cache
            .lock()
            .expect("cache poisoned")
            .note_filled(index, entry);
        filled += 1;
    }
    filled
}

/// D4's watch: a [`MemoryCost`] that charges nothing and reports the first
/// guest access through a flash window by a core whose cache is off.
///
/// Installed by the machine only while [`ClassicCache::watch_wanted`], and
/// removed the moment the cache comes back. See the module docs.
pub struct CacheOffWatch {
    cache: CacheHandle,
    /// Which core's slices are running. The machine sets it; M3 runs one.
    core: usize,
}

impl CacheOffWatch {
    pub fn new(cache: CacheHandle, core: usize) -> Self {
        Self { cache, core }
    }
}

impl MemoryCost for CacheOffWatch {
    #[inline]
    fn fetch(&mut self, addr: u32) -> u32 {
        if let Ok(mut c) = self.cache.lock() {
            c.note_access(self.core, addr, true);
        }
        0
    }

    #[inline]
    fn load(&mut self, addr: u32, _width: u8) -> u32 {
        if let Ok(mut c) = self.cache.lock() {
            c.note_access(self.core, addr, false);
        }
        0
    }

    #[inline]
    fn store(&mut self, _addr: u32, _width: u8) -> u32 {
        // The flash windows are read-only regions on this bus, so a guest
        // store into one is already a fault with a better message than this
        // one would be.
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tables_are_the_two_bases_the_rom_selects_between() {
        assert_eq!(memmap::FLASH_MMU_PRO, 0x3FF1_0000);
        assert_eq!(memmap::FLASH_MMU_APP, 0x3FF1_2000);
        // `mmu_init`'s memset length is the distance between them.
        assert_eq!(memmap::FLASH_MMU_APP - memmap::FLASH_MMU_PRO, MMU_TABLE_LEN);
        assert_eq!(MMU_TABLE_LEN, 0x2000);
    }

    #[test]
    fn the_index_arithmetic_is_the_roms() {
        // 64 KiB pages, page mode 0.
        assert_eq!(FlashMmu::shift(0), 16);
        assert_eq!(FlashMmu::page_len(0), 0x1_0000);
        // DROM: the window base is entry 0.
        assert_eq!(FlashMmu::entry_index(0x3F40_0000, 0), Some(0));
        assert_eq!(FlashMmu::entry_index(0x3F41_0000, 0), Some(1));
        assert_eq!(FlashMmu::entry_index(0x3F7F_FFFF, 0), Some(63));
        // IROM: the index is computed from the 0x4000_0000-relative offset,
        // so the linker's `0x400D_0000` is entry 64 + 13.
        assert_eq!(FlashMmu::entry_index(memmap::IROM_BASE, 0), Some(77));
        assert_eq!(FlashMmu::entry_index(0x403F_FFFF, 0), Some(127));
        assert_eq!(FlashMmu::entry_index(0x4040_0000, 0), Some(128));
        assert_eq!(FlashMmu::entry_index(0x4080_0000, 0), Some(192));
        assert_eq!(FlashMmu::entry_index(0x40BF_FFFF, 0), Some(255));
        // Everything the four windows cover fits in the flash half of the
        // table, and nothing outside them resolves.
        assert!(FlashMmu::entry_index(0x40C0_0000, 0).is_none());
        assert!(FlashMmu::entry_index(0x3F3F_FFFF, 0).is_none());
        assert!(FlashMmu::entry_index(0x3FFB_0000, 0).is_none());
        for v in [0x3F40_0000u32, 0x400D_0000, 0x4040_0000, 0x40BF_FFFF] {
            assert!(FlashMmu::entry_index(v, 0).unwrap() < FLASH_MMU_ENTRIES);
        }
    }

    #[test]
    fn a_page_number_plus_an_offset_is_the_flash_byte() {
        let mut c = ClassicCache::new();
        // `cache_flash_mmu_set(0, 0, 0x400D_0000, 0x0001_0000, 64, 1)` writes
        // `paddr >> 16` = 1 at entry 77.
        c.mmu.set_entry(0, 77, 1);
        assert_eq!(c.translate(0, memmap::IROM_BASE), Some(0x0001_0000));
        assert_eq!(c.translate(0, memmap::IROM_BASE + 0x20), Some(0x0001_0020));
        // The APP core's table is a different table.
        assert_eq!(c.translate(1, memmap::IROM_BASE), Some(0));
        // Out of the windows entirely.
        assert_eq!(c.translate(0, 0x3FFB_0000), None);
    }

    #[test]
    fn the_page_mode_is_ctrl1_bits_ten_nine() {
        let mut c = ClassicCache::new();
        assert_eq!(c.page_mode(0), 0, "the PAC reset 0x8ff has bits 10:9 clear");
        // `cache_flash_mmu_set`'s tail: (mode << 9) into a word masked with
        // 0xfffff9ff.
        c.write_ctrl1(0, (CACHE_CTRL1_RESET & !FLASH_PAGE_MODE_MASK) | (1 << 9));
        assert_eq!(c.page_mode(0), 1);
        assert_eq!(FlashMmu::page_len(1), 0x8000);
        assert_eq!(FlashMmu::shift(1), 15);
        // 32 KiB pages: twice as many entries cover the same window.
        assert_eq!(FlashMmu::entry_index(0x3F40_8000, 1), Some(1));
    }

    #[test]
    fn the_cache_is_off_at_reset_and_the_roms_two_routines_flip_it() {
        let mut c = ClassicCache::new();
        assert_eq!(c.ctrl(0), CACHE_CTRL_RESET);
        assert!(!c.enabled(0), "PAC reset 0x10 has bit 3 clear");

        // `Cache_Read_Enable`: ctrl |= 8.
        c.write_ctrl(0, c.ctrl(0) | CACHE_ENABLE, 100, 0x4000_9a97);
        assert!(c.enabled(0));
        assert!(!c.enabled(1), "the APP core's bit is its own");

        // `Cache_Read_Disable`: ctrl &= ~8, and the cycle and pc are kept.
        c.write_ctrl(0, c.ctrl(0) & !CACHE_ENABLE, 4_242, 0x4008_1c04);
        assert!(!c.enabled(0));
        assert_eq!(c.disabled(0), (4_242, Some(0x4008_1c04)));
    }

    #[test]
    fn setting_flush_ena_raises_flush_done_so_the_roms_spin_ends() {
        let mut c = ClassicCache::new();
        // `Cache_Flush`: clear flush_ena, set it, spin on bit 5, clear it.
        c.write_ctrl(0, c.ctrl(0) & !CACHE_FLUSH_ENA, 0, 0);
        assert_eq!(c.ctrl(0) & CACHE_FLUSH_DONE, 0);
        c.write_ctrl(0, c.ctrl(0) | CACHE_FLUSH_ENA, 0, 0);
        assert_ne!(
            c.ctrl(0) & CACHE_FLUSH_DONE,
            0,
            "the ROM spins on bit 5 until it is set"
        );
        c.write_ctrl(0, c.ctrl(0) & !CACHE_FLUSH_ENA, 0, 0);
        assert_eq!(c.ctrl(0) & CACHE_FLUSH_DONE, 0);
    }

    #[test]
    fn the_watch_reports_the_first_flash_window_access_with_the_cache_off() {
        let handle = ClassicCache::handle();
        handle
            .lock()
            .unwrap()
            .write_ctrl(0, CACHE_ENABLE, 10, 0x4000_9a97);
        handle
            .lock()
            .unwrap()
            .write_ctrl(0, 0, 1_284_610, 0x4008_1c04);

        let mut watch = CacheOffWatch::new(handle.clone(), 0);
        // SRAM0 is not a flash window: the bootloader runs there and must
        // not trigger.
        assert_eq!(watch.fetch(0x4007_8000), 0);
        assert!(handle.lock().unwrap().offence.is_none());

        assert_eq!(watch.fetch(0x400D_1A2C), 0);
        let hit = handle
            .lock()
            .unwrap()
            .take_offence()
            .expect("a fetch through IROM");
        assert_eq!(hit.core, 0);
        assert_eq!(hit.addr, 0x400D_1A2C);
        assert_eq!(hit.window, Window::Irom);
        assert!(hit.fetch);
        assert_eq!(hit.disabled_at, 1_284_610);
        assert_eq!(hit.disabled_by, Some(0x4008_1c04));

        // A data read through DROM is the same finding.
        watch.load(0x3F40_0100, 4);
        let hit = handle
            .lock()
            .unwrap()
            .take_offence()
            .expect("a read through DROM");
        assert_eq!(hit.window, Window::Drom);
        assert!(!hit.fetch);
    }

    #[test]
    fn a_host_side_access_never_arms_the_watch() {
        let handle = ClassicCache::handle();
        handle.lock().unwrap().set_host_access(true);
        let mut watch = CacheOffWatch::new(handle.clone(), 0);
        watch.fetch(0x400D_1A2C);
        watch.load(0x3F40_0100, 4);
        assert!(handle.lock().unwrap().offence.is_none());
    }

    #[test]
    fn the_watch_is_wanted_only_with_the_cache_off_and_the_stop_on() {
        let mut c = ClassicCache::new();
        assert!(c.watch_wanted(0), "reset leaves both caches off");
        c.write_ctrl(0, CACHE_ENABLE, 0, 0);
        assert!(!c.watch_wanted(0));
        assert!(
            c.watch_wanted(1),
            "per core: core 1's cache is off for the whole of M3, and arming on \
             that would single-step every run for a core that issues no accesses"
        );
        c.write_ctrl(0, 0, 1, 0);
        assert!(c.watch_wanted(0));
        c.set_policy(CacheOffPolicy::Permit);
        assert!(!c.watch_wanted(0), "`permit` does not check at all");
        assert!(!c.watch_wanted(1));
    }

    #[test]
    fn the_cache_state_round_trips_through_a_snapshot() {
        let mut c = ClassicCache::new();
        c.write_ctrl(0, CACHE_ENABLE, 5, 0x4000_9a97);
        c.write_ctrl(0, 0, 9, 0x4008_1c04);
        c.write_ctrl1(1, 0x0000_0aff);
        c.mmu.set_entry(0, 77, 0x31);
        c.mmu.set_entry(1, 255, 0x0d);
        let bytes = c.save_state();
        let mut back = ClassicCache::new();
        back.load_state(&bytes);
        assert_eq!(back.ctrl(0), c.ctrl(0));
        assert_eq!(back.ctrl1(1), c.ctrl1(1));
        assert_eq!(back.disabled(0), (9, Some(0x4008_1c04)));
        assert_eq!(back.mmu.entry(0, 77), 0x31);
        assert_eq!(back.mmu.entry(1, 255), 0x0d);
    }
}
