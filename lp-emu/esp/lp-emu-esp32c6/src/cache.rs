//! The MSPI cache MMU — the page table the `0x4200_0000` window reads
//! through.
//!
//! # Provenance: the mask ROM, not a datasheet
//!
//! Every number here is read off the vendored ROM ELF
//! (`lp-emu/esp/roms/esp32c6_rev0_rom.elf`) with
//! `riscv64-unknown-elf-objdump -d`. Three functions say the whole format:
//!
//! **`Cache_MMU_Init`** (`0x4002_7c76`) — the table's size, and what an
//! invalid entry looks like:
//!
//! ```text
//! li   a3, 256
//! loop: sw a5, 896(a4)    ; SPI0 + 0x380  mmu_item_index = i
//!       sw x0, 892(a4)    ; SPI0 + 0x37c  mmu_item_content = 0
//!       addi a5, a5, 1 ; bne a5, a3, loop
//! ```
//!
//! **`Cache_MSPI_MMU_Set`** (`0x4002_7c90`) — the entry's bits and the
//! index arithmetic:
//!
//! ```text
//! sw   a5, 896(s3)        ; index  = ((vaddr & mask) >> shift) + n
//! or   a5, a5, a3         ; a3 = encrypt_flag << 10  (ROM_Direct_Boot_MMU_Init: `slli s0,0xa`)
//! ori  a5, a5, 512        ; bit 9 = VALID
//! sw   a5, 892(s3)        ; content = page | encrypt | VALID
//! ```
//!
//! with `shift` chosen from the page size (`64 → 16`, `32 → 15`, `16 → 14`,
//! `8 → 13` KiB) and `mask = (0x0100_0000 >> page_mode) - 1`.
//!
//! **`MMU_Get_Page_Mode` / `MMU_Set_Page_Mode`** (`0x4002_75ea` /
//! `0x4002_75d4`) — where the page mode lives:
//!
//! ```text
//! lw   a0, 900(a5)        ; SPI0 + 0x384  mmu_power_ctrl
//! srli a0, a0, 3 ; andi a0, a0, 3
//! ```
//!
//! which is the PAC's `mmu_power_ctrl` bits 4:3, documented as "0: Max page
//! size, 1: /2, 2: /4, 3: /8". The reset value is 0, so the boot table is
//! 256 entries of 64 KiB covering `0x4200_0000..0x4300_0000` — the sixteen
//! megabytes esp-hal's linker script splits by convention into an 8 MiB
//! `ROM` window and an 8 MiB `RODATA` window ([`crate::memmap`]).
//!
//! # How the window is served
//!
//! [`translate`](CacheMmu::translate) is the whole of the address path, and
//! it is one function on purpose (director note 6): a later `t2` rung hangs
//! its cache-miss wait states off exactly this lookup.
//!
//! The window itself is a **cache fill**, not a per-access translation:
//! [`fill`] copies each valid page's flash bytes into the RAM region behind
//! `0x4200_0000` whenever the table changes or the flash under a mapped page
//! is written. Two consequences, both stated rather than hidden:
//!
//! - Instruction fetch stays a RAM read, which is what keeps the machine
//!   fast enough to be used.
//! - The model is **stricter than silicon about staleness**: a real cache
//!   keeps serving the old bytes until something invalidates it, and this
//!   one refills at the next slice boundary. A firmware bug that writes
//!   flash under its own `.text` and forgets to invalidate would therefore
//!   *work* here and fault on a board. Nothing in this milestone's images
//!   writes a mapped page — the app lives in `factory` and littlefs in
//!   `lpfs`, and only `lpfs` is ever written — so the difference is
//!   documented rather than exercised.

use std::sync::{Arc, Mutex};

use lp_emu_esp_common::SocBus;

use crate::flash::{BLOCK_LEN, FlashHandle};
use crate::memmap;

/// Entries in the table. `Cache_MMU_Init` clears exactly this many.
pub const ENTRIES: usize = 256;

/// `SPI0 + 0x37c`, the PAC's `mmu_item_content`.
pub const ITEM_CONTENT: u32 = 0x37c;
/// `SPI0 + 0x380`, the PAC's `mmu_item_index`.
pub const ITEM_INDEX: u32 = 0x380;
/// `SPI0 + 0x384`, the PAC's `mmu_power_ctrl`; bits 4:3 are the page mode.
pub const POWER_CTRL: u32 = 0x384;

/// Bit 9 — `Cache_MSPI_MMU_Set`'s `ori a5, a5, 512`.
pub const VALID: u32 = 1 << 9;
/// Bit 10 — the cache-encryption flag (`ROM_Direct_Boot_MMU_Init` shifts
/// `ets_efuse_cache_encryption_enabled()` left by 10 and ORs it in).
/// Flash encryption is not modelled; an entry that carries this bit is
/// reported once by [`fill`] and its bytes are served in the clear.
pub const ENCRYPT: u32 = 1 << 10;
/// The physical page number, below [`VALID`]: nine bits, 512 pages.
pub const PAGE_MASK: u32 = VALID - 1;

/// The window the table covers at page mode 0: `0x4200_0000..0x4300_0000`.
/// `Cache_MSPI_MMU_Set` masks the virtual address with
/// `(0x0100_0000 >> page_mode) - 1`.
pub const WINDOW_LEN: u32 = 0x0100_0000;

/// The page table, and the page mode that says how big a page is.
#[derive(Clone, Debug)]
pub struct CacheMmu {
    entries: [u32; ENTRIES],
    /// `mmu_item_index`: which entry the next `mmu_item_content` access
    /// reads or writes. No auto-increment — `Cache_MSPI_MMU_Set` writes the
    /// index before every content word.
    index: u32,
    /// `mmu_power_ctrl[4:3]`. 0 = 64 KiB pages.
    page_mode: u8,
    /// Entry indices whose mapping changed since the last [`fill`].
    dirty: Vec<u32>,
}

/// The table, shared between SPI0's register view and the machine's fill.
pub type CacheHandle = Arc<Mutex<CacheMmu>>;

impl Default for CacheMmu {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheMmu {
    /// The table as reset leaves it: every entry invalid, 64 KiB pages.
    ///
    /// Invalid is `0`, which is what `Cache_MMU_Init` writes — a machine
    /// that started with an identity map would hide the fact that a direct
    /// load has to program this table itself (`crate::loader`).
    pub fn new() -> Self {
        Self {
            entries: [0; ENTRIES],
            index: 0,
            page_mode: 0,
            dirty: Vec::new(),
        }
    }

    /// Bytes per page: 64 KiB >> the page mode.
    pub fn page_len(&self) -> u32 {
        BLOCK_LEN >> self.page_mode
    }

    /// `Cache_MSPI_MMU_Set`'s shift: 16 for 64 KiB pages, one less per mode.
    fn shift(&self) -> u32 {
        16 - u32::from(self.page_mode)
    }

    /// `Cache_MSPI_MMU_Set`'s `(0x0100_0000 >> page_mode) - 1`.
    fn vaddr_mask(&self) -> u32 {
        (WINDOW_LEN >> self.page_mode) - 1
    }

    pub fn page_mode(&self) -> u8 {
        self.page_mode
    }

    pub fn set_page_mode(&mut self, mode: u8) {
        let mode = mode & 3;
        if mode == self.page_mode {
            return;
        }
        self.page_mode = mode;
        // Every mapping now means something else.
        self.dirty = (0..ENTRIES as u32).collect();
    }

    /// The window this table serves.
    pub fn window(&self) -> (u32, u32) {
        (memmap::FLASH_CACHE_BASE, WINDOW_LEN >> self.page_mode)
    }

    /// The table index a virtual address falls in, or `None` if it is
    /// outside the window.
    pub fn entry_index(&self, vaddr: u32) -> Option<u32> {
        let (base, len) = self.window();
        if vaddr < base || vaddr - base >= len {
            return None;
        }
        Some((vaddr & self.vaddr_mask()) >> self.shift())
    }

    /// **The address path.** A virtual address in the flash window to a flash
    /// byte offset, or `None` if the page's entry is invalid.
    pub fn translate(&self, vaddr: u32) -> Option<u32> {
        let index = self.entry_index(vaddr)?;
        let entry = self.entries[index as usize];
        if entry & VALID == 0 {
            return None;
        }
        let page = entry & PAGE_MASK;
        Some((page << self.shift()) | (vaddr & (self.page_len() - 1)))
    }

    pub fn entry(&self, index: u32) -> u32 {
        self.entries.get(index as usize).copied().unwrap_or(0)
    }

    /// Write one entry, as `mmu_item_content` does. Out-of-range indices are
    /// dropped: the register is 32 bits wide and the table is 256 entries,
    /// so a guest can name an entry that does not exist.
    pub fn set_entry(&mut self, index: u32, value: u32) {
        let Some(slot) = self.entries.get_mut(index as usize) else {
            log::warn!("cache: mmu_item_index {index} is past the {ENTRIES}-entry table");
            return;
        };
        if *slot == value {
            return;
        }
        *slot = value;
        if !self.dirty.contains(&index) {
            self.dirty.push(index);
        }
    }

    pub fn index(&self) -> u32 {
        self.index
    }

    pub fn set_index(&mut self, index: u32) {
        self.index = index;
    }

    /// Map `vaddr`'s page to the flash offset `paddr`. What the loader does
    /// in place of the second-stage bootloader's `Cache_MSPI_MMU_Set` calls.
    ///
    /// Returns `false` if `vaddr` is outside the window or `paddr` is not
    /// page-aligned — `paddr % page == vaddr % page` is the constraint that
    /// makes esp-hal link at `0x4200_0020` rather than at the window base.
    pub fn map(&mut self, vaddr: u32, paddr: u32) -> bool {
        let Some(index) = self.entry_index(vaddr) else {
            return false;
        };
        let page_len = self.page_len();
        if !paddr.is_multiple_of(page_len) {
            return false;
        }
        let page = paddr >> self.shift();
        if page > PAGE_MASK {
            return false;
        }
        self.set_entry(index, page | VALID);
        true
    }

    /// Every mapped page, as `(virtual base, flash offset)`, in index order.
    pub fn mapped_pages(&self) -> Vec<(u32, u32)> {
        let page_len = self.page_len();
        let (base, _) = self.window();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| *e & VALID != 0)
            .map(|(i, e)| (base + i as u32 * page_len, (*e & PAGE_MASK) << self.shift()))
            .collect()
    }

    /// Mark every valid page for a refill — what a flash write under the
    /// window means.
    pub fn invalidate_page_at(&mut self, flash_offset: u32) {
        let page_len = self.page_len();
        let page = flash_offset / page_len;
        for (index, entry) in self.entries.iter().enumerate() {
            if *entry & VALID != 0 && (*entry & PAGE_MASK) == page {
                let index = index as u32;
                if !self.dirty.contains(&index) {
                    self.dirty.push(index);
                }
            }
        }
    }

    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// The entries needing a refill, and clear the list.
    pub fn take_dirty(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.dirty)
    }

    pub fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ENTRIES * 4 + 8);
        out.extend_from_slice(&self.index.to_le_bytes());
        out.push(self.page_mode);
        for entry in &self.entries {
            out.extend_from_slice(&entry.to_le_bytes());
        }
        out
    }

    pub fn load_state(&mut self, bytes: &[u8]) {
        if bytes.len() < 5 + ENTRIES * 4 {
            log::warn!("CacheMmu::load_state: {} bytes is too short", bytes.len());
            return;
        }
        self.index = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        self.page_mode = bytes[4] & 3;
        for (i, slot) in self.entries.iter_mut().enumerate() {
            let at = 5 + i * 4;
            *slot = u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        }
        // Whatever the window holds now, the table just changed under it.
        self.dirty = (0..ENTRIES as u32).collect();
    }
}

/// Copy every page the table marked dirty out of the flash image and into
/// the RAM region behind the window. Returns the number of pages filled.
///
/// Called once by the builder after the loader programs the table, and from
/// the machine's slice loop whenever the table or the flash under it moved.
pub fn fill(bus: &mut SocBus, flash: &FlashHandle, cache: &CacheHandle) -> usize {
    let (dirty, page_len, entries, base) = {
        let mut mmu = cache.lock().unwrap();
        let dirty = mmu.take_dirty();
        let page_len = mmu.page_len();
        let (base, _) = mmu.window();
        let entries: Vec<(u32, u32)> = dirty.iter().map(|i| (*i, mmu.entry(*i))).collect();
        (dirty, page_len, entries, base)
    };
    if dirty.is_empty() {
        return 0;
    }
    let flash = flash.lock().unwrap();
    let mut filled = 0;
    for (index, entry) in entries {
        let vaddr = base + index * page_len;
        if entry & VALID == 0 {
            // An entry that went invalid: the window keeps whatever it held.
            // Silicon would fault on a fetch through it; this machine cannot
            // tell a fetch from a stale byte, so it says so and moves on.
            log::debug!("cache: entry {index} ({vaddr:#010x}) is invalid; the window is stale");
            continue;
        }
        if entry & ENCRYPT != 0 {
            log::warn!(
                "cache: entry {index} ({vaddr:#010x}) asks for flash encryption, which is not \
                 modelled; the page is served in the clear"
            );
        }
        let paddr = (entry & PAGE_MASK) << (page_len.trailing_zeros());
        let Some(bytes) = flash.peek(paddr, page_len) else {
            log::warn!(
                "cache: entry {index} ({vaddr:#010x}) maps flash {paddr:#010x}, past the \
                 {:#x}-byte chip; the page is left as it was",
                flash.len()
            );
            continue;
        };
        if let Err(e) = bus.load_image(vaddr, bytes) {
            log::warn!("cache: filling {vaddr:#010x} from flash {paddr:#010x}: {e:?}");
            continue;
        }
        filled += 1;
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reset_table_maps_nothing_the_way_cache_mmu_init_leaves_it() {
        let mmu = CacheMmu::new();
        assert_eq!(mmu.page_len(), 64 * 1024);
        assert_eq!(mmu.page_mode(), 0);
        assert_eq!(mmu.window(), (0x4200_0000, 0x0100_0000));
        assert_eq!(mmu.translate(0x4200_0020), None);
        assert!(mmu.mapped_pages().is_empty());
    }

    #[test]
    fn the_index_arithmetic_is_cache_mspi_mmu_sets() {
        let mmu = CacheMmu::new();
        // ((vaddr & 0x00ff_ffff) >> 16)
        assert_eq!(mmu.entry_index(0x4200_0000), Some(0));
        assert_eq!(mmu.entry_index(0x4200_ffff), Some(0));
        assert_eq!(mmu.entry_index(0x4201_0000), Some(1));
        assert_eq!(mmu.entry_index(0x4205_0020), Some(5));
        // The window is 16 MiB: `0x4300_0000` is past the last entry, and
        // esp-hal's separate `0x4280_0000` DROM window is inside it.
        assert_eq!(mmu.entry_index(0x4280_0000), Some(128));
        assert_eq!(mmu.entry_index(0x42ff_ffff), Some(255));
        assert_eq!(mmu.entry_index(0x4300_0000), None);
        assert_eq!(mmu.entry_index(0x4080_0000), None);
    }

    #[test]
    fn a_mapped_page_translates_and_keeps_the_offset_within_it() {
        let mut mmu = CacheMmu::new();
        // The mapping a direct load programs: the app's flash window sits at
        // the `factory` partition, `0x10000`.
        assert!(mmu.map(0x4200_0000, 0x0001_0000));
        assert_eq!(mmu.entry(0), 1 | VALID);
        assert_eq!(mmu.translate(0x4200_0020), Some(0x0001_0020));
        assert_eq!(mmu.translate(0x4200_ffff), Some(0x0001_ffff));
        // The next page is not mapped just because this one is.
        assert_eq!(mmu.translate(0x4201_0000), None);
        assert!(mmu.map(0x4201_0000, 0x0002_0000));
        assert_eq!(mmu.translate(0x4201_0004), Some(0x0002_0004));
        assert_eq!(
            mmu.mapped_pages(),
            vec![(0x4200_0000, 0x0001_0000), (0x4201_0000, 0x0002_0000)]
        );
    }

    #[test]
    fn a_paddr_that_is_not_page_aligned_is_refused_rather_than_rounded() {
        let mut mmu = CacheMmu::new();
        // `paddr % 64K == vaddr % 64K` is why esp-hal links at
        // `0x4200_0020` and not at the window base; a loader that rounded
        // here would shift the app by 32 bytes and nothing would say so.
        assert!(!mmu.map(0x4200_0000, 0x0001_0020));
        assert!(!mmu.map(0x4400_0000, 0x0001_0000));
        assert_eq!(mmu.entry(0), 0);
    }

    #[test]
    fn the_entry_format_is_the_one_cache_mspi_mmu_set_writes() {
        let mut mmu = CacheMmu::new();
        // page 1, VALID: `ori a5, a5, 512`
        mmu.set_entry(0, 1 | VALID);
        assert_eq!(mmu.translate(0x4200_0000), Some(0x0001_0000));
        // Bit 9 clear is invalid — what `Cache_MMU_Init`'s zero means.
        mmu.set_entry(0, 1);
        assert_eq!(mmu.translate(0x4200_0000), None);
        // The page number is nine bits, so 512 pages of 64 KiB = 32 MiB.
        assert_eq!(PAGE_MASK, 0x1ff);
        mmu.set_entry(0, PAGE_MASK | VALID);
        assert_eq!(mmu.translate(0x4200_0000), Some(0x01ff_0000));
    }

    #[test]
    fn a_page_mode_change_resizes_the_pages_and_the_window() {
        let mut mmu = CacheMmu::new();
        // `MMU_Set_Page_Mode` writes bits 4:3 of `mmu_power_ctrl`; mode 1 is
        // "max page size / 2".
        mmu.set_page_mode(1);
        assert_eq!(mmu.page_len(), 32 * 1024);
        assert_eq!(mmu.window(), (0x4200_0000, 0x0080_0000));
        assert_eq!(mmu.entry_index(0x4200_8000), Some(1));
        assert!(mmu.map(0x4200_8000, 0x0000_8000));
        assert_eq!(mmu.translate(0x4200_8004), Some(0x0000_8004));
        assert_eq!(mmu.entry_index(0x4280_0000), None, "the window halved");
    }

    #[test]
    fn only_the_entries_that_changed_are_dirty() {
        let mut mmu = CacheMmu::new();
        assert!(!mmu.has_dirty());
        mmu.map(0x4200_0000, 0x0001_0000);
        mmu.map(0x4201_0000, 0x0002_0000);
        assert_eq!(mmu.take_dirty(), vec![0, 1]);
        assert!(!mmu.has_dirty());
        // Writing the same value again is not a change.
        mmu.map(0x4200_0000, 0x0001_0000);
        assert!(!mmu.has_dirty());
        // A flash write under a mapped page is.
        mmu.invalidate_page_at(0x0002_0800);
        assert_eq!(mmu.take_dirty(), vec![1]);
        mmu.invalidate_page_at(0x0031_0000);
        assert!(!mmu.has_dirty(), "lpfs is not mapped into the window");
    }

    #[test]
    fn state_round_trips_and_comes_back_needing_a_refill() {
        let mut mmu = CacheMmu::new();
        mmu.map(0x4205_0000, 0x0006_0000);
        mmu.set_index(42);
        mmu.set_page_mode(2);
        let blob = mmu.save_state();
        let mut other = CacheMmu::new();
        other.load_state(&blob);
        assert_eq!(other.index(), 42);
        assert_eq!(other.page_mode(), 2);
        assert_eq!(other.entry(5), mmu.entry(5));
        assert!(other.has_dirty(), "a restored table has not been filled");
    }
}
