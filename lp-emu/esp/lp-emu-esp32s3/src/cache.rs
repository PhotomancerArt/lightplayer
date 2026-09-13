//! The S3's flash cache: the **one** MMU page table both buses read through,
//! the two enable bits, the fill, and **D4's cache-off fetch stop**.
//!
//! # Provenance: the mask ROM, not a datasheet
//!
//! Every number here is read off the vendored ROM ELF
//! (`lp-emu/esp/roms/esp32s3_rev0_rom.elf`) with
//! `xtensa-esp32s3-elf-objdump -d`. Nothing comes from a datasheet and
//! nothing comes from analogy with either sibling — the S3 PAC names only
//! `cache_mmu_fault_*`, `cache_mmu_power_ctrl` and `cache_mmu_owner`, its
//! `SPI0` has none of the C6's `mmu_item_*` path, and there is no table
//! array in its `EXTMEM` as the classic's `DPORT` has (`m6/notes.md` §3.4).
//! Four routines say the whole format; M6 P01's report §8 read them first,
//! and this file re-read every line it cites.
//!
//! **`Cache_MMU_Init`** (`0x4004_f6f4`) — where the table is, how big it is,
//! and what an *unmapped* entry holds:
//!
//! ```text
//! 4004f6f4:  entry  a1, 32
//! 4004f6f7:  l32r   a9, (0x600c5000)      ; THE TABLE BASE
//! 4004f6fa:  l32r   a10, (0x4000)         ; the fill value
//! 4004f6fd:  movi   a8, 0x200             ; 512 ENTRIES
//! 4004f702:  loop   a8, 4004f709
//! 4004f705:    s32i.n a10, a9, 0
//! 4004f707:    addi.n a9, a9, 4           ; 4 BYTES PER ENTRY
//! 4004f709:  retw.n
//! ```
//!
//! so the table is **512 × 4 bytes at `0x600C_5000..0x600C_5800`**, and the
//! ROM's own initialiser writes **`0x4000`** into every slot. The same base
//! literal is loaded at `0x4004_f75a` (`Cache_Ibus_MMU_Set`), `0x4004_f7e2`
//! (`Cache_Dbus_MMU_Set`), `0x4004_f589` (`Cache_Set_IDROM_MMU_Size`) and
//! `0x4004_f835` (`Cache_Count_Flash_Pages`); `0x600c_57fc` at `0x4004_f86c`
//! is the last entry, `base + 511 × 4`.
//!
//! **`Cache_Ibus_MMU_Set(ext_ram, vaddr, paddr, psize, num, fixed)`**
//! (`0x4004_f710`) and **`Cache_Dbus_MMU_Set`** (`0x4004_f798`) — the same
//! code twice, differing in one literal — the index arithmetic, the entry
//! format, the page size, and the windows:
//!
//! ```text
//! 4004f713:  mov.n  a12, a2               ; the caller's target select …
//! 4004f715:  movi.n a2, 64
//! 4004f717:  quou   a2, a2, a5            ; 64 / psize
//! 4004f71a:  l32r   a8, (0xffff)
//! 4004f722:  sra    a8, a8                ; 0xffff >> (64/psize − 1) = the
//! 4004f728:  and    a8, a8, (vaddr|paddr) ;   alignment mask → return 2
//! 4004f732:  bnei   a5, 64, → return 3    ; ONLY 64 KiB PAGES
//! 4004f738:  l32r   a2, (0x2000000)       ; the 32 MiB region …
//! 4004f73b:  l32r   a5, (0xfe000000)      ; … and its boundary mask
//! 4004f743:  l32r   a5, (0xbe000000)      ; Ibus: vaddr + 0xbe000000
//! 4004f757:  bltu   0x1ffffff, a2, → return 4   ;   must fit 25 bits, i.e.
//!                                        ;   vaddr ∈ 0x4200_0000..0x43FF_FFFF
//!   (Dbus, 4004f7cb:  l32r a5, (0xc4000000)   ; vaddr ∈ 0x3C00_0000..0x3DFF_FFFF)
//! 4004f75a:  l32r   a2, (0x600c5000)
//! 4004f75d:  extui  a3, a3, 16, 9         ; INDEX = (vaddr >> 16) & 0x1ff
//! 4004f760:  slli   a3, a3, 2
//! 4004f763:  add.n  a3, a3, a2            ; &table[index]
//! 4004f754:  extui  a15, a4, 16, 16       ; PAGE = paddr >> 16
//! 4004f76f:  add.n  a13, a9, a15          ; page + i
//! 4004f771:  or     a13, a13, a12         ; … | the target select
//! 4004f774:  s32i.n a13, a3, 0
//! ```
//!
//! **`Cache_Count_Flash_Pages`** (`0x4004_f820`) — which bits are flags and
//! which are the page number:
//!
//! ```text
//! 4004f838:  l32r   a12, (0xc000)
//! 4004f845:  bany   a10, a12, → skip      ; either of bits 15:14 set: NOT a flash page
//! 4004f848:  extui  a10, a10, 0, 14       ; the page number is bits 13:0
//! ```
//!
//! # The three answers, and the one that is the opposite of the classic's
//!
//! - **One table serves both buses.** `Cache_Ibus_MMU_Set` and
//!   `Cache_Dbus_MMU_Set` write the same `0x600C_5000` table with the same
//!   `(vaddr >> 16) & 0x1ff`, so entry *n* is `0x3C00_0000 + n × 64 KiB` on
//!   the data bus **and** `0x4200_0000 + n × 64 KiB` on the instruction bus.
//!   The classic has two index bases in one table and the C6 has one window;
//!   the S3 has two windows on one index. The fill therefore writes a page
//!   into **both** RAM regions behind the windows, because the guest may
//!   read it through either.
//! - **The page size is 64 KiB and only 64 KiB** (`bnei a5, 64`), and there
//!   is no page-mode field: 512 entries × 64 KiB = 32 MiB, exactly the window
//!   `memory.x` declares ([`crate::memmap::DROM_LEN`]).
//! - **The entry carries a valid bit, and it is INVERTED relative to the
//!   C6's.** Bit 14 set (`0x4000`) means *invalid*; `Cache_MMU_Init` writes
//!   it into every slot, and `Cache_Count_Flash_Pages` skips any entry with a
//!   flag bit set. So an **unwritten entry is unmapped** —
//!   [`FlashMmu::translate`] answers `None` for it — which is the **opposite
//!   of the classic**, whose bare page number makes a zeroed entry *flash
//!   page 0*. Copying the classic's semantics here would be a machine that
//!   silently maps flash page 0 wherever the guest has not mapped anything:
//!   it boots and lies, and no test in this milestone would catch it. (The
//!   other flag bit, 15, is the `ext_ram` select the callers OR in; every
//!   caller on this machine's boot path passes 0 — `ets_loader_map_range`
//!   `40045547: movi.n a15, 0` / `40045553: mov.n a10, a15` — and this
//!   file names no meaning for it beyond "not a flash page", because the
//!   ROM names none.)
//!
//! **What reset leaves in the table is not known**, and this file does not
//! pretend: a ROM-up boot runs `Cache_MMU_Init` before anything reads the
//! table (`ROM_Boot_Cache_Init` `4004e406: call8 Cache_MMU_Init`, from
//! `ets_run_flash_bootloader`), and a direct load writes the table the
//! bootloader would have. [`FlashMmu::new`] therefore starts every entry at
//! [`MMU_INVALID`] — the one initial state the ROM is known to produce — and
//! says so.
//!
//! # The enable bits, and the polarity
//!
//! **`Cache_Enable_ICache`** (`0x4004_f308`) / **`Cache_Disable_ICache`**
//! (`0x4004_f2b8`) and the DCache pair (`0x4004_f37c` / `0x4004_f32c`):
//!
//! ```text
//! Cache_Enable_ICache:   EXTMEM+0x60 |= 1      (4004f30b: l32r 600c4060; 4004f315: or a8, a8, 1)
//! Cache_Disable_ICache:  EXTMEM+0x60 &= ~1     (4004f2be: movi.n a2, -2; 4004f2c5: and)
//! Cache_Enable_DCache:   EXTMEM+0x00 |= 1      (4004f37f: l32r 600c4000)
//! Cache_Disable_DCache:  EXTMEM+0x00 &= ~1
//! ```
//!
//! ⚠️ **`1` means ON.** The PAC agrees (`icache_ctrl.icache_enable`, "0:
//! disable, 1: enable"). The C6's `l1_icache_ctrl.l1_icache_shut_ibus0` is
//! "0: enable, 1: disable", so a watch that copied the C6's predicate would
//! arm **backwards** and report a stop on every access while the cache is
//! on. The predicate below is written from the S3 ROM's `or …, 1` and
//! tested in both directions. `Cache_Suspend_*` / `Cache_Resume_*`
//! (`0x4004_f3a0` … `0x4004_f480`) also set and clear the `*_ctrl1` shut
//! bits around the same enable bit; the shut bits are remembered by the
//! `EXTMEM` view and not read by this model, which arms on the enable bit
//! alone and says so.
//!
//! # Which cache serves which window
//!
//! `Cache_Ibus_MMU_Set` accepts only `0x4200_0000..` and `Cache_Dbus_MMU_Set`
//! only `0x3C00_0000..`, and the two enable bits are two registers: the
//! instruction window is the ICache's and the data window is the DCache's.
//! D4 on this chip is therefore **two** predicates — an access in the IROM
//! window while `icache_enable` is 0, an access in the DROM window while
//! `dcache_enable` is 0 — reported with the cache that was off. A *load*
//! from the IROM window (an `l32r` literal in `.text`) is counted against
//! the ICache with the window it is in, which is what the address says and
//! all this model can say.
//!
//! # D4 — the cache-off fetch stop
//!
//! On silicon a core that reaches through a flash window with that window's
//! cache disabled **stalls until the cache comes back**, and nothing
//! observes it except a crash or a watchdog. This emulator refuses instead:
//! the window is ordinary RAM here, so the guest would sail through a bug
//! that hangs a board. That asymmetry is the whole point (plan D4). The
//! mechanics are the classic's: the check rides
//! [`lp_emu_core::cycle_model::MemoryCost`], the one per-access seam the
//! chip crate has; it is installed **only while a cache is off**
//! ([`S3Cache::watch_wanted`]), the machine hands the hart one instruction
//! at a time while it is armed so the reported pc is the instruction that
//! made the access, every cycle it charges is 0, and
//! `--cache-off-fetch permit` turns the check off rather than swallowing its
//! answer. A host-side fill or peek marks itself with
//! [`S3Cache::set_host_access`] and never arms it.
//!
//! **One core runs**, so there is one watch and no per-core divergence
//! question: the classic's `--app-mmu-divergence` (ruling R4) exists because
//! two cores program two tables, and this chip has one table. There is no
//! S3 counterpart and no exit code 7 here — said in the README rather than
//! reserved as a flag.

use std::sync::{Arc, Mutex};

use lp_emu_core::cycle_model::MemoryCost;
use lp_emu_core::sched::Cycles;
use lp_emu_esp_common::SocBus;

use crate::memmap;

/// The MMU table's base, `0x600C_5000` — `Cache_MMU_Init` `4004f6f7`.
pub const MMU_TABLE_BASE: u32 = 0x600C_5000;

/// Entries in the table: 512 — `Cache_MMU_Init` `4004f6fd: movi a8, 0x200`.
pub const MMU_ENTRIES: usize = 512;

/// Bytes per entry: 4 — `Cache_MMU_Init` `4004f707: addi.n a9, a9, 4`.
pub const MMU_ENTRY_LEN: u32 = 4;

/// The table's length, `0x800` — and `0x600c_57fc` at `4004f86c` is
/// `MMU_TABLE_BASE + MMU_TABLE_LEN - 4`, the last entry.
pub const MMU_TABLE_LEN: u32 = MMU_ENTRIES as u32 * MMU_ENTRY_LEN;

/// What an **unmapped** entry holds: `0x4000`, bit 14 — the value
/// `Cache_MMU_Init` writes into every slot (`4004f6fa: l32r a10, (0x4000)`)
/// and one of the two bits `Cache_Count_Flash_Pages` tests for "not a flash
/// page" (`4004f838: l32r a12, (0xc000)` / `4004f845: bany`).
pub const MMU_INVALID: u32 = 0x4000;

/// The entry's flag bits, 15:14 — `Cache_Count_Flash_Pages` `4004f845:
/// bany a10, 0xc000`. Either set means the entry is not a flash page.
pub const MMU_FLAG_MASK: u32 = 0xC000;

/// The entry's page number, bits 13:0 — `Cache_Count_Flash_Pages`
/// `4004f848: extui a10, a10, 0, 14`.
pub const MMU_PAGE_MASK: u32 = 0x3FFF;

/// The page size: 64 KiB, the only one the ROM accepts —
/// `Cache_Ibus_MMU_Set` `4004f732: bnei a5, 64, → return 3`.
pub const PAGE_LEN: u32 = 0x1_0000;

/// `log2(PAGE_LEN)`: the shift in `extui a3, a3, 16, 9` (the index) and
/// `extui a15, a4, 16, 16` (the page).
pub const PAGE_SHIFT: u32 = 16;

/// The index mask: nine bits — `4004f75d: extui a3, a3, 16, 9`.
pub const INDEX_MASK: u32 = 0x1FF;

/// The two windows the table serves, and their lengths: the ROM's range
/// checks (`4004f743: l32r a5, (0xbe000000)` for the instruction bus,
/// `4004f7cb: l32r a5, (0xc4000000)` for the data bus, each tested against
/// `0x1ffffff`) are 32 MiB from [`memmap::IROM_BASE`] and
/// [`memmap::DROM_BASE`], which is 512 pages — the whole table.
pub const WINDOW_LEN: u32 = MMU_ENTRIES as u32 * PAGE_LEN;

/// Which flash window an address falls in, for the stop's message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// `0x4200_0000..`, the instruction bus — the ICache's.
    Irom,
    /// `0x3C00_0000..`, the data bus — the DCache's.
    Drom,
}

impl Window {
    /// The window `addr` is in, if any.
    pub fn of(addr: u32) -> Option<Self> {
        let in_span = |base: u32| addr >= base && addr - base < WINDOW_LEN;
        if in_span(memmap::IROM_BASE) {
            Some(Window::Irom)
        } else if in_span(memmap::DROM_BASE) {
            Some(Window::Drom)
        } else {
            None
        }
    }

    /// The window's base.
    pub const fn base(self) -> u32 {
        match self {
            Window::Irom => memmap::IROM_BASE,
            Window::Drom => memmap::DROM_BASE,
        }
    }

    /// The cache that serves it.
    pub const fn cache(self) -> Which {
        match self {
            Window::Irom => Which::ICache,
            Window::Drom => Which::DCache,
        }
    }
}

impl std::fmt::Display for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Window::Irom => "IROM",
            Window::Drom => "DROM",
        })
    }
}

/// The two caches, each with its own enable bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// `EXTMEM.icache_ctrl.icache_enable`, `+0x60` bit 0.
    ICache,
    /// `EXTMEM.dcache_ctrl.dcache_enable`, `+0x00` bit 0.
    DCache,
}

impl Which {
    pub const fn index(self) -> usize {
        match self {
            Which::ICache => 0,
            Which::DCache => 1,
        }
    }

    /// The `EXTMEM` register that holds this cache's enable bit.
    pub const fn ctrl_offset(self) -> u32 {
        match self {
            Which::ICache => 0x060,
            Which::DCache => 0x000,
        }
    }

    /// The PAC's name for the register and its bit 0.
    pub const fn ctrl_name(self) -> &'static str {
        match self {
            Which::ICache => "icache_ctrl.icache_enable",
            Which::DCache => "dcache_ctrl.dcache_enable",
        }
    }
}

impl std::fmt::Display for Which {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Which::ICache => "ICache",
            Which::DCache => "DCache",
        })
    }
}

/// Bit 0 of `icache_ctrl` / `dcache_ctrl`: **1 = enabled**. The ROM's own
/// `or a8, a8, 1` (`Cache_Enable_ICache` `4004f315`), and the PAC's "0:
/// disable, 1: enable".
pub const CACHE_ENABLE: u32 = 1 << 0;

/// What `--cache-off-fetch` chooses. **`Stop` is the default and is what
/// every gate run uses.**
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

/// One guest access through a flash window while the cache serving that
/// window was off.
///
/// `pc`, `cycle` and `symbol` are filled in by the machine, which is the only
/// thing that can see the hart; the watch records the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheOffAccess {
    pub addr: u32,
    pub window: Window,
    /// The cache that was off — the window's.
    pub cache: Which,
    /// `true` for an instruction fetch, `false` for a data read.
    pub fetch: bool,
    /// When that cache was last turned off, and the pc of the store that did
    /// it — `None` when nothing ever turned it on: the PAC's reset leaves
    /// both enable bits clear, and on silicon it is the ROM's
    /// `ROM_Boot_Cache_Init` / `Cache_Enable_Defalut_ICache_Mode` and the
    /// bootloader's `Cache_Resume_*` that set them.
    pub disabled_at: Cycles,
    pub disabled_by: Option<u32>,
}

/// The S3's flash MMU page table — one table, 512 entries, both buses.
#[derive(Clone)]
pub struct FlashMmu {
    table: [u32; MMU_ENTRIES],
    /// Entry indices whose backing bytes have not been copied into the
    /// windows since the entry last moved. The fill drains it.
    dirty: Vec<u32>,
}

impl Default for FlashMmu {
    fn default() -> Self {
        Self::new()
    }
}

impl FlashMmu {
    /// Every entry [`MMU_INVALID`] — what `Cache_MMU_Init` leaves, which is
    /// the only initial state the ROM is known to produce (module docs).
    pub fn new() -> Self {
        Self {
            table: [MMU_INVALID; MMU_ENTRIES],
            dirty: Vec::new(),
        }
    }

    pub fn entry(&self, index: usize) -> u32 {
        self.table.get(index).copied().unwrap_or(MMU_INVALID)
    }

    pub fn entries(&self) -> &[u32; MMU_ENTRIES] {
        &self.table
    }

    /// Write an entry and mark its page for the fill. The mark is
    /// unconditional; [`fill`] is what makes that cheap, by remembering
    /// which flash page each window page already holds.
    pub fn set_entry(&mut self, index: usize, value: u32) {
        if let Some(slot) = self.table.get_mut(index) {
            *slot = value;
        }
        self.mark_dirty(index as u32);
    }

    /// This entry's pages have to be copied out of the chip again.
    pub fn mark_dirty(&mut self, index: u32) {
        if index as usize >= MMU_ENTRIES {
            return;
        }
        if !self.dirty.contains(&index) {
            self.dirty.push(index);
        }
    }

    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// The entries needing a refill, and clear the list.
    pub fn take_dirty(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.dirty)
    }

    /// Mark every entry that maps the flash page `flash_offset` falls in —
    /// what a flash write under the window means. Returns the indices it
    /// marked, so the caller can forget what those window pages held.
    pub fn invalidate_page_at(&mut self, flash_offset: u32) -> Vec<u32> {
        let page = flash_offset >> PAGE_SHIFT;
        let hits: Vec<u32> = (0..MMU_ENTRIES as u32)
            .filter(|i| {
                let e = self.entry(*i as usize);
                e & MMU_FLAG_MASK == 0 && e & MMU_PAGE_MASK == page
            })
            .collect();
        for index in &hits {
            self.mark_dirty(*index);
        }
        hits
    }

    /// Every entry marked for a refill — a snapshot restore: whatever the
    /// windows hold now, the table just changed under them.
    pub fn mark_all_dirty(&mut self) {
        for index in 0..MMU_ENTRIES as u32 {
            self.mark_dirty(index);
        }
    }

    /// The table index a virtual address falls in — `(vaddr >> 16) & 0x1ff`
    /// — or `None` for an address outside both windows.
    pub fn entry_index(vaddr: u32) -> Option<u32> {
        Window::of(vaddr)?;
        Some((vaddr >> PAGE_SHIFT) & INDEX_MASK)
    }

    /// The two virtual pages an entry serves: `(DROM, IROM)`.
    pub const fn entry_vaddrs(index: u32) -> (u32, u32) {
        (
            memmap::DROM_BASE + index * PAGE_LEN,
            memmap::IROM_BASE + index * PAGE_LEN,
        )
    }

    /// Is this entry a mapped flash page?
    pub const fn is_mapped(entry: u32) -> bool {
        entry & MMU_FLAG_MASK == 0
    }

    /// **The address path.** A virtual address in either window to the flash
    /// byte offset its entry names, or `None` for an address outside the
    /// windows **or in an unmapped page** — an entry with a flag bit set,
    /// which is what an unwritten entry is on this chip (module docs).
    ///
    /// One function on purpose (the C6's rule): a later `t2` rung hangs
    /// cache-miss wait states off exactly this lookup.
    pub fn translate(&self, vaddr: u32) -> Option<u32> {
        let index = Self::entry_index(vaddr)?;
        let entry = self.entry(index as usize);
        if !Self::is_mapped(entry) {
            return None;
        }
        Some(((entry & MMU_PAGE_MASK) << PAGE_SHIFT) | (vaddr & (PAGE_LEN - 1)))
    }
}

/// The cache-enable state, the MMU table and D4's watch slot. Shared between
/// the `EXTMEM` view, the `FLASH_MMU` view and the machine.
pub struct S3Cache {
    pub mmu: FlashMmu,
    /// `icache_ctrl.icache_enable` and `dcache_ctrl.dcache_enable`, as the
    /// `EXTMEM` view last stored them. **Both clear at reset** — the PAC
    /// gives `icache_ctrl` and `dcache_ctrl` no reset value.
    enabled: [bool; 2],
    /// When each cache was last turned off, and by which pc. Both start at
    /// the reset that left them off in the first place.
    disabled_at: [Cycles; 2],
    disabled_by: [Option<u32>; 2],
    policy: CacheOffPolicy,
    /// Which flash page each entry's window pages already hold, or `None`
    /// when never filled. What keeps `Cache_MMU_Init`'s 512 stores from
    /// costing 512 page copies.
    filled: [Option<u32>; MMU_ENTRIES],
    /// The first offence since the machine last took one.
    offence: Option<CacheOffAccess>,
    /// Set around a host-side peek, poke or fill, which goes through the
    /// bus's decode and would otherwise look like a guest access.
    host_access: bool,
}

/// The table and the cache state, shared between the register views and the
/// machine. The C6's and the classic's `CacheHandle` precedent.
pub type CacheHandle = Arc<Mutex<S3Cache>>;

impl Default for S3Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl S3Cache {
    pub fn new() -> Self {
        Self {
            mmu: FlashMmu::new(),
            enabled: [false; 2],
            disabled_at: [0; 2],
            disabled_by: [None; 2],
            policy: CacheOffPolicy::default(),
            filled: [None; MMU_ENTRIES],
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

    /// Is `which` enabled — bit 0 of its `*_ctrl` register?
    pub fn enabled(&self, which: Which) -> bool {
        self.enabled[which.index()]
    }

    /// The `EXTMEM` view stored a new `*_ctrl` word: record the enable bit,
    /// and — when it went off — the cycle and the pc of the store, which the
    /// stop's message needs. Returns whether the bit changed, so the view
    /// can ask for a slice boundary and the machine can arm the watch.
    pub fn note_ctrl(&mut self, which: Which, word: u32, at: Cycles, pc: u32) -> bool {
        let now_on = word & CACHE_ENABLE != 0;
        let was_on = self.enabled[which.index()];
        self.enabled[which.index()] = now_on;
        if was_on && !now_on {
            self.disabled_at[which.index()] = at;
            self.disabled_by[which.index()] = Some(pc);
            log::debug!("cache: the {which} was disabled at cycle {at} by pc {pc:#010x}");
        }
        was_on != now_on
    }

    /// When `which` was last turned off, and the pc that did it.
    pub fn disabled(&self, which: Which) -> (Cycles, Option<u32>) {
        (
            self.disabled_at[which.index()],
            self.disabled_by[which.index()],
        )
    }

    /// [`FlashMmu::translate`].
    pub fn translate(&self, vaddr: u32) -> Option<u32> {
        self.mmu.translate(vaddr)
    }

    /// Should the machine arm D4's watch? Only under
    /// [`CacheOffPolicy::Stop`], and only while **either** cache is off —
    /// one core runs on this chip, so there is one watch.
    pub fn watch_wanted(&self) -> bool {
        self.policy == CacheOffPolicy::Stop && !(self.enabled[0] && self.enabled[1])
    }

    /// See [`S3Cache`]'s `host_access`.
    pub fn set_host_access(&mut self, on: bool) {
        self.host_access = on;
    }

    /// Does entry `index`'s window pages already hold flash page `page`?
    pub fn already_filled(&self, index: u32, page: u32) -> bool {
        self.filled
            .get(index as usize)
            .copied()
            .flatten()
            .is_some_and(|held| held == page)
    }

    pub fn note_filled(&mut self, index: u32, page: u32) {
        if let Some(slot) = self.filled.get_mut(index as usize) {
            *slot = Some(page);
        }
    }

    /// What entry `index`'s window pages hold, if they were ever filled.
    pub fn filled_page(&self, index: u32) -> Option<u32> {
        self.filled.get(index as usize).copied().flatten()
    }

    /// Forget what one entry's window pages hold — the bytes under them
    /// moved.
    pub fn forget_fill(&mut self, index: u32) {
        if let Some(slot) = self.filled.get_mut(index as usize) {
            *slot = None;
        }
    }

    /// Forget every fill — a snapshot restore.
    pub fn forget_fills(&mut self) {
        self.filled = [None; MMU_ENTRIES];
    }

    /// The watch's entry point: one guest access.
    fn note_access(&mut self, addr: u32, fetch: bool) {
        if self.host_access || self.offence.is_some() {
            return;
        }
        let Some(window) = Window::of(addr) else {
            return;
        };
        let cache = window.cache();
        if self.enabled(cache) {
            return;
        }
        let (disabled_at, disabled_by) = self.disabled(cache);
        self.offence = Some(CacheOffAccess {
            addr,
            window,
            cache,
            fetch,
            disabled_at,
            disabled_by,
        });
    }

    /// The offence, if there is one, cleared.
    pub fn take_offence(&mut self) -> Option<CacheOffAccess> {
        self.offence.take()
    }

    /// Snapshot: the enable bits with their cycle/pc provenance, then the
    /// table. Restored by the `EXTMEM` view's `load_state`.
    pub fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 * 14 + MMU_ENTRIES * 4);
        for i in 0..2 {
            out.push(u8::from(self.enabled[i]));
            out.extend_from_slice(&self.disabled_at[i].to_le_bytes());
            out.push(u8::from(self.disabled_by[i].is_some()));
            out.extend_from_slice(&self.disabled_by[i].unwrap_or(0).to_le_bytes());
        }
        for entry in &self.mmu.table {
            out.extend_from_slice(&entry.to_le_bytes());
        }
        out
    }

    pub fn load_state(&mut self, bytes: &[u8]) {
        let head = 2 * 14;
        if bytes.len() != head + MMU_ENTRIES * 4 {
            log::warn!(
                "S3Cache::load_state: {} bytes, expected {}; ignored",
                bytes.len(),
                head + MMU_ENTRIES * 4
            );
            return;
        }
        for i in 0..2 {
            let c = &bytes[i * 14..(i + 1) * 14];
            self.enabled[i] = c[0] != 0;
            self.disabled_at[i] = u64::from_le_bytes(c[1..9].try_into().expect("8 bytes"));
            self.disabled_by[i] =
                (c[9] != 0).then(|| u32::from_le_bytes(c[10..14].try_into().expect("4 bytes")));
        }
        for (i, entry) in bytes[head..].chunks_exact(4).enumerate() {
            self.mmu.table[i] = u32::from_le_bytes(entry.try_into().expect("4 bytes"));
        }
        // Whatever the windows hold now, the table just changed under them.
        self.forget_fills();
        self.mmu.mark_all_dirty();
    }
}

/// **The fill.** Copy every page the table marked dirty out of the flash
/// chip and into the RAM regions behind **both** windows. Returns how many
/// entries were copied.
///
/// Called once by the builder after a direct load programs the table, and
/// from the machine's slice loop whenever the table or the flash under a
/// mapped page moved.
///
/// # Why the window is a fill and not a per-access translation
///
/// Instruction fetch stays a plain RAM read, which is what keeps this
/// machine fast enough to be used. The consequence is that the model is
/// **stricter than silicon about staleness**: a real cache serves stale
/// lines until it is flushed, and this one never does. Stated, not hidden.
///
/// # What an unmapped entry means here
///
/// An entry with a flag bit set names no flash page, and the fill copies
/// nothing for it. If the entry's window pages *had* been filled — the
/// bootloader's `Cache_MMU_Init` invalidating the pages the ROM mapped its
/// image through — they are overwritten with `0xff` and the fill said so at
/// `debug`: on silicon a read there is a cache-MMU fault, and erased bytes
/// are the closest a RAM window can come to "nothing here" without
/// inventing an exception the hart has no model of. **Modeled**, and named
/// as the difference it is.
pub fn fill(bus: &mut SocBus, flash: &crate::flash::FlashHandle, cache: &CacheHandle) -> usize {
    // `(index, entry, what the window pages held)`, gathered before the
    // flash lock is taken so the two are never held at once.
    let work: Vec<(u32, u32, Option<u32>)> = {
        let mut c = cache.lock().expect("cache poisoned");
        let dirty = c.mmu.take_dirty();
        if dirty.is_empty() {
            return 0;
        }
        dirty
            .into_iter()
            .map(|index| (index, c.mmu.entry(index as usize), c.filled_page(index)))
            .collect()
    };

    let flash = flash.lock().expect("flash poisoned");
    let mut filled = 0;
    for (index, entry, held) in work {
        let (drom, irom) = FlashMmu::entry_vaddrs(index);
        if !FlashMmu::is_mapped(entry) {
            if held.is_some() {
                let erased = vec![0xffu8; PAGE_LEN as usize];
                for vaddr in [drom, irom] {
                    if let Err(e) = bus.load_image(vaddr, &erased) {
                        log::debug!("cache: entry {index} ({vaddr:#010x}) not cleared: {e:?}");
                    }
                }
                log::debug!(
                    "cache: entry {index} became unmapped ({entry:#06x}); its window pages \
                     read as erased flash now"
                );
                cache.lock().expect("cache poisoned").forget_fill(index);
            }
            continue;
        }
        let page = entry & MMU_PAGE_MASK;
        if held == Some(page) {
            continue;
        }
        let paddr = page << PAGE_SHIFT;
        let Some(bytes) = flash.peek(paddr, PAGE_LEN) else {
            log::warn!(
                "cache: entry {index} ({drom:#010x} / {irom:#010x}) maps flash {paddr:#010x}, \
                 past the {:#x}-byte chip; the pages are left as they were",
                flash.len()
            );
            continue;
        };
        let mut ok = true;
        for vaddr in [drom, irom] {
            if let Err(e) = bus.load_image(vaddr, bytes) {
                log::warn!("cache: entry {index} ({vaddr:#010x}) has no RAM behind it: {e:?}");
                ok = false;
            }
        }
        if ok {
            cache
                .lock()
                .expect("cache poisoned")
                .note_filled(index, page);
            filled += 1;
        }
    }
    filled
}

/// D4's watch: a [`MemoryCost`] that charges nothing and reports the first
/// guest access through a flash window whose cache is off.
///
/// Installed by the machine only while [`S3Cache::watch_wanted`], and
/// removed the moment both caches are back. See the module docs.
pub struct CacheOffWatch {
    cache: CacheHandle,
}

impl CacheOffWatch {
    pub fn new(cache: CacheHandle) -> Self {
        Self { cache }
    }
}

impl MemoryCost for CacheOffWatch {
    #[inline]
    fn fetch(&mut self, addr: u32) -> u32 {
        if let Ok(mut c) = self.cache.lock() {
            c.note_access(addr, true);
        }
        0
    }

    #[inline]
    fn load(&mut self, addr: u32, _width: u8) -> u32 {
        if let Ok(mut c) = self.cache.lock() {
            c.note_access(addr, false);
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
    fn the_table_is_where_and_what_cache_mmu_init_says() {
        assert_eq!(MMU_TABLE_BASE, 0x600C_5000);
        assert_eq!(MMU_ENTRIES, 0x200);
        assert_eq!(MMU_TABLE_LEN, 0x800);
        // `0x600c57fc` at `4004f86c` is the last entry.
        assert_eq!(MMU_TABLE_BASE + MMU_TABLE_LEN - 4, 0x600C_57FC);
        // 512 pages of 64 KiB is the 32 MiB window `memory.x` declares.
        assert_eq!(WINDOW_LEN, memmap::DROM_LEN);
        assert_eq!(WINDOW_LEN, memmap::IROM_LEN);
        // And the fresh table is what `Cache_MMU_Init` writes.
        let mmu = FlashMmu::new();
        assert!(mmu.entries().iter().all(|e| *e == MMU_INVALID));
    }

    #[test]
    fn the_index_arithmetic_is_the_roms_and_both_windows_share_it() {
        // `extui a3, a3, 16, 9`: nine bits from bit 16.
        assert_eq!(FlashMmu::entry_index(0x3C00_0000), Some(0));
        assert_eq!(FlashMmu::entry_index(0x3C01_0000), Some(1));
        assert_eq!(FlashMmu::entry_index(0x3C04_4190), Some(4));
        assert_eq!(FlashMmu::entry_index(0x3DFF_FFFF), Some(511));
        // The instruction window lands on the SAME indices.
        assert_eq!(FlashMmu::entry_index(0x4200_0000), Some(0));
        assert_eq!(FlashMmu::entry_index(0x4205_0020), Some(5));
        assert_eq!(FlashMmu::entry_index(0x43FF_FFFF), Some(511));
        // Outside the two 32 MiB windows nothing resolves — including the
        // ROM's own range refusals (`return 4`).
        assert!(FlashMmu::entry_index(0x4400_0000).is_none());
        assert!(FlashMmu::entry_index(0x3E00_0000).is_none());
        assert!(FlashMmu::entry_index(0x3FC8_8000).is_none());
        assert!(FlashMmu::entry_index(0x4037_8000).is_none());
        assert_eq!(FlashMmu::entry_vaddrs(5), (0x3C05_0000, 0x4205_0000));
    }

    /// The ruling: an unwritten entry is **unmapped** on this chip.
    #[test]
    fn an_unwritten_entry_is_unmapped_not_flash_page_zero() {
        let mmu = FlashMmu::new();
        assert_eq!(mmu.translate(0x3C00_0000), None);
        assert_eq!(mmu.translate(0x4200_0000), None);
        assert_eq!(mmu.translate(0x4205_0020), None);
        assert!(!FlashMmu::is_mapped(MMU_INVALID));
        // A zero entry, by contrast, IS flash page 0 — bits 15:14 clear.
        assert!(FlashMmu::is_mapped(0));
    }

    #[test]
    fn a_page_number_plus_an_offset_is_the_flash_byte_on_either_bus() {
        let mut mmu = FlashMmu::new();
        // `Cache_Ibus_MMU_Set(0, 0x4205_0000, 0x0006_0000, 64, 1, 0)`
        // stores `(0x60000 >> 16) | 0` = 6 at entry 5.
        mmu.set_entry(5, 6);
        assert_eq!(mmu.translate(0x4205_0000), Some(0x0006_0000));
        assert_eq!(mmu.translate(0x4205_0020), Some(0x0006_0020));
        // The same entry serves the data bus at the same index.
        assert_eq!(mmu.translate(0x3C05_0020), Some(0x0006_0020));
        // A flag bit set is not a flash page, whatever the low bits say.
        mmu.set_entry(6, 7 | MMU_INVALID);
        assert_eq!(mmu.translate(0x4206_0000), None);
        mmu.set_entry(7, 8 | 0x8000);
        assert_eq!(mmu.translate(0x4207_0000), None);
        assert_eq!(mmu.take_dirty(), vec![5, 6, 7]);
    }

    #[test]
    fn a_flash_write_under_a_mapped_page_marks_its_entries() {
        let mut mmu = FlashMmu::new();
        mmu.set_entry(3, 0x61);
        mmu.set_entry(9, 0x61);
        mmu.set_entry(4, 0x62);
        mmu.take_dirty();
        assert_eq!(mmu.invalidate_page_at(0x0061_0800), vec![3, 9]);
        assert_eq!(mmu.take_dirty(), vec![3, 9]);
    }

    /// The polarity, held in both directions: `1` is ON.
    #[test]
    fn one_means_enabled_and_the_watch_arms_only_while_a_cache_is_off() {
        let mut c = S3Cache::new();
        assert!(
            !c.enabled(Which::ICache),
            "the PAC gives icache_ctrl no reset: off"
        );
        assert!(!c.enabled(Which::DCache));
        assert!(c.watch_wanted(), "reset leaves both caches off");

        // `Cache_Enable_ICache`: +0x60 |= 1. `Cache_Enable_DCache`: +0x00 |= 1.
        assert!(c.note_ctrl(Which::ICache, 0x0000_0001, 100, 0x4004_f31b));
        assert!(c.enabled(Which::ICache));
        assert!(c.watch_wanted(), "the DCache is still off");
        assert!(c.note_ctrl(Which::DCache, 0x0000_0001, 101, 0x4004_f38f));
        assert!(c.enabled(Which::DCache));
        assert!(!c.watch_wanted(), "both on: nothing to arm");

        // `Cache_Disable_ICache`: +0x60 &= ~1, and the cycle and pc are kept.
        assert!(c.note_ctrl(Which::ICache, 0x0000_000e, 4_242, 0x4004_f2cb));
        assert!(!c.enabled(Which::ICache));
        assert_eq!(c.disabled(Which::ICache), (4_242, Some(0x4004_f2cb)));
        assert!(c.watch_wanted());
        // A write that leaves the bit as it was changes nothing.
        assert!(!c.note_ctrl(Which::ICache, 0x0000_000e, 4_300, 0));
        assert_eq!(c.disabled(Which::ICache), (4_242, Some(0x4004_f2cb)));

        c.set_policy(CacheOffPolicy::Permit);
        assert!(!c.watch_wanted(), "`permit` does not check at all");
    }

    #[test]
    fn the_watch_reports_the_first_window_access_whose_cache_is_off() {
        let handle = S3Cache::handle();
        {
            let mut c = handle.lock().unwrap();
            c.note_ctrl(Which::ICache, 1, 10, 0x4004_f31b);
            c.note_ctrl(Which::DCache, 1, 11, 0x4004_f38f);
            c.note_ctrl(Which::ICache, 0, 1_284_610, 0x4004_f2cb);
        }
        let mut watch = CacheOffWatch::new(handle.clone());
        // IRAM is not a flash window: the bootloader and esp-storage run
        // there with the caches off and must not trigger.
        assert_eq!(watch.fetch(0x4037_8400), 0);
        assert_eq!(watch.load(0x3FC8_B320, 4), 0);
        assert!(handle.lock().unwrap().offence.is_none());
        // The DCache is on: a DROM read is fine.
        watch.load(0x3C00_0140, 4);
        assert!(handle.lock().unwrap().offence.is_none());

        assert_eq!(watch.fetch(0x4205_1A2C), 0);
        let hit = handle
            .lock()
            .unwrap()
            .take_offence()
            .expect("a fetch through IROM with the ICache off");
        assert_eq!(hit.addr, 0x4205_1A2C);
        assert_eq!(hit.window, Window::Irom);
        assert_eq!(hit.cache, Which::ICache);
        assert!(hit.fetch);
        assert_eq!(hit.disabled_at, 1_284_610);
        assert_eq!(hit.disabled_by, Some(0x4004_f2cb));

        // Now the other way round: DCache off, ICache on.
        {
            let mut c = handle.lock().unwrap();
            c.note_ctrl(Which::ICache, 1, 20, 0);
            c.note_ctrl(Which::DCache, 0, 21, 0x4004_f33f);
        }
        watch.fetch(0x4205_1A2C);
        assert!(handle.lock().unwrap().offence.is_none(), "the ICache is on");
        watch.load(0x3C00_0100, 4);
        let hit = handle.lock().unwrap().take_offence().expect("a DROM read");
        assert_eq!(hit.window, Window::Drom);
        assert_eq!(hit.cache, Which::DCache);
        assert!(!hit.fetch);
        assert_eq!(hit.disabled_by, Some(0x4004_f33f));
    }

    #[test]
    fn a_host_side_access_never_arms_the_watch() {
        let handle = S3Cache::handle();
        handle.lock().unwrap().set_host_access(true);
        let mut watch = CacheOffWatch::new(handle.clone());
        watch.fetch(0x4205_1A2C);
        watch.load(0x3C00_0100, 4);
        assert!(handle.lock().unwrap().offence.is_none());
    }

    #[test]
    fn the_cache_state_round_trips_through_a_snapshot() {
        let mut c = S3Cache::new();
        c.note_ctrl(Which::ICache, 1, 5, 0x4004_f31b);
        c.note_ctrl(Which::ICache, 0, 9, 0x4004_f2cb);
        c.note_ctrl(Which::DCache, 1, 12, 0x4004_f38f);
        c.mmu.set_entry(5, 0x31);
        c.mmu.set_entry(511, 0x0d);
        let bytes = c.save_state();
        let mut back = S3Cache::new();
        back.load_state(&bytes);
        assert!(!back.enabled(Which::ICache));
        assert!(back.enabled(Which::DCache));
        assert_eq!(back.disabled(Which::ICache), (9, Some(0x4004_f2cb)));
        assert_eq!(back.mmu.entry(5), 0x31);
        assert_eq!(back.mmu.entry(511), 0x0d);
        assert_eq!(back.mmu.entry(0), MMU_INVALID);
        assert!(back.mmu.has_dirty(), "a restore refills everything");
    }
}
