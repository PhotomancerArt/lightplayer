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
//!
//! # What the *unwritten* part of a mapped page reads as, and why it moved
//!
//! A fill copies a whole page, so every byte of a mapped page that no
//! segment covers now reads as **`0xff`** — erased flash. Under M3 the
//! window was a zeroed RAM region with the segments placed into it, and the
//! same bytes read as `0x00`. On the shipped memfs image that is 26,262
//! bytes, `0x4223996a..0x4224_0000`: the tail of the last page the app's
//! `.text` reaches into.
//!
//! `0xff` is the honest answer — a flasher writes an image and leaves the
//! rest of the sector erased, and the cache window on a board reads erased
//! flash there — but it is a change to emulated memory that no flash
//! *controller* required, so it is worth naming. It was checked and is
//! **inert on the images this milestone runs**: the memfs boot is
//! instruction-for-instruction identical either way (47,681,337 to the 5.5 s
//! deadline, `[stack] high-water 11432 B of 71960 B`, verified by running the
//! same ELF against a `0x00`-filled chip). The guest never reads past the end
//! of its own `.text`.
//!
//! It is written down because the *address* of that boundary is a property
//! of the ELF, not of the model: a firmware that did read past `.text` would
//! do it at a different offset in every build, and the symptom would look
//! like the machine being nondeterministic when it is not. (M5 P1 chased a
//! neighbouring illusion to the same root — DD45: the CI runner compiles the
//! pinned reference firmware into a *different binary* than a local build,
//! so a figure that depends on code layout differs between them while every
//! register access is identical.)

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

// ---------------------------------------------------------------------------
// What an address costs: the cache's fills and the APB's wait states (`t3`)
// ---------------------------------------------------------------------------
//
// Everything below is the C6 half of `lp_emu_core::MemoryCost`. It hangs off
// this module because this module is where the flash window's address path
// already lives ([`CacheMmu::translate`], and the module doc above): the
// window is the only part of the address space whose cost is not one cycle,
// and the cache that serves it is the reason.
//
// It does **not** call `translate`, and that is deliberate. The fill model
// above copies whole 64 KiB pages into RAM, so by the time the hart fetches,
// the translation has already happened; what is left to charge is the
// *cache*, which on this part is indexed by the virtual address. The two
// agree by construction here: a set index spans
// `SETS * LINE_BYTES` = 8 KiB, which is inside a 64 KiB page, so the index
// bits come from the page offset and are the same virtually or physically.
// (The tags are not, but no two virtual pages in these images map to one
// physical page, so there is no alias to get wrong.)

/// Bytes in a cache line, and so the size of a fill.
///
/// `measured` **and** `documented`, agreeing: the mask ROM's
/// `Cache_Get_ICache_Line_Size` (`0x4002_75aa`) is two instructions —
///
/// ```text
/// 400275aa <Cache_Get_ICache_Line_Size>:
///   400275aa: 02000513   li   a0,32
///   400275ae: 8082       ret
/// ```
///
/// — and P2's `rodata_stride` kernel found the knee between 16 and 32 bytes
/// with the instruction side (`code_walk`) agreeing independently
/// (`docs/reports/2026-09-08-esp32c6-t3-calibration.md` §2.4).
pub const LINE_BYTES: u32 = 32;

/// Ways. `documented` — the mask ROM, not the TRM (see [`CACHE_BYTES`]).
///
/// `Cache_Get_Mode` (`0x4002_75b0`) fills a three-field descriptor and
/// `Cache_Travel_Tag_Memory` (`0x4002_7d96`) reads it back as
/// `{ u32 size; u16 line; u8 ways; }`:
///
/// ```text
/// 400275c6: 4791         li   a5,4
/// 400275c8: 00a41223     sh   a0,4(s0)      ; line size
/// 400275cc: 00f40323     sb   a5,6(s0)      ; ways = 4
/// ```
///
/// and the traversal divides by exactly those two:
///
/// ```text
/// 40027db4: lbu a5,6(s1)      ; ways
/// 40027db8: lw  a1,0(s1)      ; total size
/// 40027dc0: divu a1,a1,a5     ; bytes per way
/// 40027ddc: remu a1,s0,a1     ; address within a way
/// 40027de0: divu a1,a1,a4     ; / line size = set index
/// 40027dea: bltu a5,a0,…      ; for way in 0..ways
/// ```
pub const WAYS: usize = 4;

/// Total bytes of cache. `documented` — `Cache_Get_Mode`'s first store:
///
/// ```text
/// 400275b4: 67a1     lui  a5,0x8     ; 0x8000 = 32,768
/// 400275b6: c11c     sw   a5,0(a0)
/// ```
///
/// **The TRM's Cache chapter was not consulted** — it is not in this
/// repository and this phase ran offline — so OQ3's "cite both, and the ROM
/// wins if they disagree" is met on the ROM side only. That is a reported
/// deviation, not a silent one; the ROM is the source that wins by OQ3's own
/// rule, and the one geometry number the ROM does *not* have to be trusted
/// for — the line — is independently measured.
///
/// One cache, not two: the ROM has a single descriptor, a single
/// `Cache_Get_Mode`, and `EXTMEM`'s size/blocksize registers come as one
/// `l1_icache_*` pair and one `l1_cache_*` pair over the same array
/// (`regs/extmem.rs`). Instruction fetch and data reads through the flash
/// window therefore share these tags.
pub const CACHE_BYTES: u32 = 32 * 1024;

/// Sets: 32 KiB / 32 B / 4 ways.
pub const SETS: usize = (CACHE_BYTES / LINE_BYTES) as usize / WAYS;

/// What one line's fill costs the CPU, in CPU cycles.
///
/// `measured`, from `code_walk` — 98,304 bytes of straight-line 4-byte
/// assembly, a working set three times the cache, all-miss on every pass:
///
/// ```text
/// (1,064,273 silicon cycles − 24,576 instructions × 1 cycle) / 3,072 lines
///   = 338.44 cycles per 32-byte line
/// ```
///
/// Cross-checked by the data side, which shares no code and no address
/// space with it: `rodata_stride/16` puts two accesses in one line and costs
/// 168.71 memory cycles each (2 × 168.71 = 337.4 per fill, 0.3 % away), and
/// `rodata_stride/32`–`/4096` put one access in each line at 349.3–350.0
/// (3.3 % the other way). 338 reproduces all three inside ±3.4 %.
///
/// **The documented flash configuration cannot produce this number, and that
/// is a finding rather than a rounding.** The image header says `SPI Speed:
/// 40MHz, SPI Mode: DIO` (the bootloader's own banner, quoted in the
/// `cycle-probe` silicon transcript at lines 26 and 38–39), and DIO at
/// 40 MHz reads 32 bytes in
///
/// ```text
///   8 clocks command (1 bit/clock)
/// + 16 clocks address+mode (32 bits at 2 bits/clock)
/// + 128 clocks data (256 bits at 2 bits/clock)
/// = 152 SPI clocks × (160 MHz CPU / 40 MHz SPI) = 608 CPU cycles
/// ```
///
/// which is 1.8× what silicon charges. A 32-byte line cannot arrive in 338
/// CPU cycles at two bits per 40 MHz clock — 128 clocks of data alone is 512
/// CPU cycles — so the part is not reading the way the header describes:
/// four bits per clock at 40 MHz would be ≈336, and two bits per clock at
/// 80 MHz ≈304 plus controller overhead. Which one it is needs a kernel that
/// reads SPI0's clock and mode registers at run time, which `cycle-probe`
/// does not have. **The constant is the measurement, not the arithmetic**;
/// the arithmetic is written out because the brief asked for it and because
/// it is the thing that turned out to be wrong.
pub const LINE_FILL_CYCLES: u32 = 338;

/// What an APB access costs beyond the load or store itself.
///
/// `measured`, from `mmio_poll`: a three-instruction poll loop costs 12.0011
/// cycles an iteration on silicon at **both** UART0 `status` (`0x6000_001C`)
/// and SYSTIMER `unit0_value.lo` (`0x6000_A044`) — indistinguishable, so
/// this is the bus's cost and not a peripheral's. Less the three
/// instructions at `iram_loop`'s measured 1.000 cycles each: **9** cycles of
/// wait state, which is the "10-cycle APB read" of §2.2 with the load
/// instruction's own cycle taken out (this hook charges what is *beyond* the
/// instruction's class).
///
/// **Stores were not measured.** `cycle-probe` polls; it never writes in a
/// loop. They are charged the same here, because a write that did not wait
/// for the bus would have to be posted, and a posted write on a bus this
/// slow would show up as a *negative* residual somewhere — but that is
/// reasoning, not a reading, and a `mmio_store` kernel is named in the
/// report as the capture that would settle it.
pub const APB_ACCESS_CYCLES: u32 = 9;

/// Invalid tag. Line indices are `addr >> 5` over a 32-bit space, so
/// `u32::MAX` is only reachable at `0xffff_ffe0`, which is not memory on
/// this part.
const NO_LINE: u32 = u32::MAX;

/// The initial MRU order of a set: way 0 most recent … way 3 least.
/// Two bits per way, most-recently-used in the low pair.
const FRESH_ORDER: u8 = 0b11_10_01_00;

/// The C6's answer to "what does this address cost?" — the `t3` model.
///
/// Three terms, and no fourth:
///
/// - a **fill** ([`LINE_FILL_CYCLES`]) whenever an access misses the flash
///   window's cache;
/// - **nothing** for a hit, which is measured rather than assumed:
///   `flash_loop` and `iram_loop` are the same 42 bytes of machine code at
///   two addresses and cost the same to 0.0002 cycles per instruction once
///   resident (§2.1), so flash residency itself is free;
/// - the **APB**'s wait states ([`APB_ACCESS_CYCLES`]) on a load or store in
///   the high peripheral window.
///
/// **There is no per-slice term and no scale factor.** P2's `slice_shape`
/// kernel did not isolate a per-slice cost — it measured the console path,
/// where silicon is 10,727 cycles *faster* than the emulator, the opposite
/// sign to the +4,075 the harness's tick 15 shows — so there is no measured
/// number to charge and none is invented (the phase brief's "refuse to add a
/// free parameter without a kernel that isolates it").
pub struct CacheCost {
    /// `SETS * WAYS` line indices (`addr >> 5`), [`NO_LINE`] when empty.
    /// Allocated once; nothing here allocates on the hot path.
    tags: Vec<u32>,
    /// One packed MRU order per set. Exact LRU: four ways fit in a byte.
    order: Vec<u8>,
    /// Statistics, for the calibration record. Not part of the model.
    fills: u64,
    hits: u64,
    apb: u64,
}

impl Default for CacheCost {
    fn default() -> Self {
        Self::new()
    }
}

impl CacheCost {
    pub fn new() -> Self {
        Self {
            tags: vec![NO_LINE; SETS * WAYS],
            order: vec![FRESH_ORDER; SETS],
            fills: 0,
            hits: 0,
            apb: 0,
        }
    }

    /// Empty the cache — what a reset, or a full invalidate, leaves.
    pub fn reset(&mut self) {
        self.tags.fill(NO_LINE);
        self.order.fill(FRESH_ORDER);
    }

    pub fn fills(&self) -> u64 {
        self.fills
    }
    pub fn hits(&self) -> u64 {
        self.hits
    }
    pub fn apb_accesses(&self) -> u64 {
        self.apb
    }

    /// Is this address served through the flash cache?
    ///
    /// The whole 16 MiB the MMU covers at page mode 0 — esp-hal's linker
    /// script splits it into an 8 MiB `ROM` window and an 8 MiB `RODATA`
    /// window by convention, but the cache does not know that and neither
    /// does this.
    #[inline]
    fn cached(addr: u32) -> bool {
        addr.wrapping_sub(memmap::FLASH_CACHE_BASE) < WINDOW_LEN
    }

    /// Is this address on the high peripheral bus the kernels measured?
    ///
    /// `0x6000_0000..0x6010_0000`. The low MMIO window (`0x2000_0000`, the
    /// interrupt controllers) is **not** charged: it is core-local rather
    /// than APB, no kernel measured it, and inventing either 0 or 9 for it
    /// would be the same unmeasured guess. Zero is the one that adds no
    /// term. Named in the report as a capture that is owed.
    #[inline]
    fn peripheral(addr: u32) -> bool {
        addr.wrapping_sub(memmap::MMIO_HIGH_BASE) < memmap::MMIO_HIGH_LEN
    }

    /// One line: hit, or fill and evict the least recently used way.
    #[inline]
    fn touch_line(&mut self, line: u32) -> u32 {
        let set = (line as usize) & (SETS - 1);
        let base = set * WAYS;
        let mut order = self.order[set];

        for slot in 0..WAYS {
            if self.tags[base + slot] == line {
                self.hits += 1;
                self.order[set] = promote(order, slot as u8);
                return 0;
            }
        }

        // Miss: the way at the far end of the MRU list is the victim.
        let victim = ((order >> ((WAYS as u8 - 1) * 2)) & 3) as usize;
        self.tags[base + victim] = line;
        order = promote(order, victim as u8);
        self.order[set] = order;
        self.fills += 1;
        LINE_FILL_CYCLES
    }

    /// The lines an access of `width` bytes at `addr` touches. A misaligned
    /// access across a line boundary is two fills, which is what the part
    /// does — the C6 core performs misaligned data accesses in hardware.
    #[inline]
    fn span(&mut self, addr: u32, width: u8) -> u32 {
        let first = addr >> LINE_BYTES.trailing_zeros();
        let last = addr.wrapping_add(u32::from(width) - 1) >> LINE_BYTES.trailing_zeros();
        let mut cycles = self.touch_line(first);
        if last != first {
            cycles += self.touch_line(last);
        }
        cycles
    }
}

/// Move `way` to the front of a set's packed MRU list.
#[inline]
fn promote(order: u8, way: u8) -> u8 {
    let mut out = way;
    let mut shift = 2;
    for slot in 0..WAYS as u8 {
        let w = (order >> (slot * 2)) & 3;
        if w != way {
            out |= w << shift;
            shift += 2;
        }
    }
    out
}

impl lp_emu_core::MemoryCost for CacheCost {
    /// An instruction fetch.
    ///
    /// Charged on the line holding `addr` only. A 4-byte instruction at a
    /// 2-byte offset can straddle a line, and this misses that second line —
    /// but the very next fetch is in it, so the fill is charged one
    /// instruction later rather than not at all, and a straight-line walk of
    /// 4-byte instructions (which is what `code_walk` calibrated on) never
    /// straddles at all. The alternative would need the instruction's width
    /// at fetch time, which the bus does not have.
    #[inline]
    fn fetch(&mut self, addr: u32) -> u32 {
        if !Self::cached(addr) {
            return 0;
        }
        self.touch_line(addr >> LINE_BYTES.trailing_zeros())
    }

    #[inline]
    fn load(&mut self, addr: u32, width: u8) -> u32 {
        if Self::cached(addr) {
            return self.span(addr, width);
        }
        if Self::peripheral(addr) {
            self.apb += 1;
            return APB_ACCESS_CYCLES;
        }
        0
    }

    #[inline]
    fn store(&mut self, addr: u32, width: u8) -> u32 {
        // A store into the flash window is not a thing the guest does — the
        // window is read-only on the part — but the model answers for it
        // anyway rather than pretending the address is free.
        if Self::cached(addr) {
            return self.span(addr, width);
        }
        if Self::peripheral(addr) {
            self.apb += 1;
            return APB_ACCESS_CYCLES;
        }
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_core::MemoryCost as _;

    #[test]
    fn the_geometry_is_the_one_the_mask_rom_describes() {
        // Cache_Get_Mode: size 0x8000, ways 4; Cache_Get_ICache_Line_Size: 32.
        assert_eq!(CACHE_BYTES, 32_768);
        assert_eq!(WAYS, 4);
        assert_eq!(LINE_BYTES, 32);
        assert_eq!(SETS, 256, "32 KiB / 32 B / 4 ways");
        // A set index spans 8 KiB, inside a 64 KiB page: virtual and
        // physical indexing agree, which is why this model needs no
        // translation.
        assert!(SETS as u32 * LINE_BYTES <= BLOCK_LEN);
    }

    #[test]
    fn only_the_flash_window_and_the_apb_cost_anything() {
        let mut c = CacheCost::new();
        // HP SRAM: free, at any width.
        assert_eq!(c.load(memmap::HP_SRAM_BASE, 4), 0);
        assert_eq!(c.store(memmap::HP_SRAM_BASE + 0x40, 1), 0);
        assert_eq!(c.fetch(memmap::HP_SRAM_BASE + 0x80), 0);
        // The mask ROM's own text: free (it is not behind the cache).
        assert_eq!(c.fetch(memmap::ROM_MASK_BASE), 0);
        // UART0 status, the register the harness polls: the APB's wait.
        assert_eq!(c.load(0x6000_001C, 4), APB_ACCESS_CYCLES);
        assert_eq!(c.store(0x6000_0000, 4), APB_ACCESS_CYCLES);
        // The interrupt controller window is deliberately uncharged.
        assert_eq!(c.load(0x2000_1000, 4), 0);
        assert_eq!(c.fills(), 0);
        assert_eq!(c.apb_accesses(), 2);
    }

    #[test]
    fn a_line_is_filled_once_and_then_free() {
        let mut c = CacheCost::new();
        let base = memmap::FLASH_CACHE_BASE + 0x1000;
        assert_eq!(c.fetch(base), LINE_FILL_CYCLES, "cold");
        for off in (0..LINE_BYTES).step_by(4) {
            assert_eq!(c.fetch(base + off), 0, "the rest of the line is resident");
        }
        assert_eq!(
            c.fetch(base + LINE_BYTES),
            LINE_FILL_CYCLES,
            "the next line"
        );
        assert_eq!(c.fills(), 2);
        assert_eq!(c.hits(), 8);
    }

    #[test]
    fn a_working_set_bigger_than_the_cache_never_hits_and_one_smaller_never_misses() {
        // This is `code_walk`'s premise and its result: 96 KiB of
        // straight-line code is all-miss on the second pass as well as the
        // first, and the same walk inside the cache is all-hit.
        let base = memmap::FLASH_CACHE_BASE + 0x2_0000;
        for (bytes, expect_second_pass_fills) in [
            (3 * CACHE_BYTES, 3 * CACHE_BYTES / LINE_BYTES),
            (CACHE_BYTES / 2, 0),
        ] {
            let mut c = CacheCost::new();
            for _pass in 0..2 {
                for off in (0..bytes).step_by(4) {
                    c.fetch(base + off);
                }
            }
            let first_pass = bytes / LINE_BYTES;
            assert_eq!(
                c.fills(),
                u64::from(first_pass + expect_second_pass_fills),
                "{bytes} bytes over two passes"
            );
        }
    }

    #[test]
    fn four_ways_of_conflict_fit_and_the_fifth_evicts_the_oldest() {
        let mut c = CacheCost::new();
        // Five lines that land in the same set: one way-span apart.
        let way_span = SETS as u32 * LINE_BYTES;
        let a = memmap::FLASH_CACHE_BASE;
        for i in 0..4 {
            assert_eq!(c.fetch(a + i * way_span), LINE_FILL_CYCLES);
        }
        // All four are resident.
        for i in 0..4 {
            assert_eq!(c.fetch(a + i * way_span), 0);
        }
        // A fifth evicts line 0, which was touched least recently.
        assert_eq!(c.fetch(a + 4 * way_span), LINE_FILL_CYCLES);
        assert_eq!(c.fetch(a), LINE_FILL_CYCLES, "line 0 was the victim");
        assert_eq!(c.fetch(a + 3 * way_span), 0, "line 3 was not");
    }

    #[test]
    fn a_load_across_a_line_boundary_is_two_fills() {
        let mut c = CacheCost::new();
        let edge = memmap::FLASH_CACHE_BASE + LINE_BYTES - 2;
        assert_eq!(c.load(edge, 4), 2 * LINE_FILL_CYCLES);
        assert_eq!(c.fills(), 2);
        // And an aligned one in the same pair is free afterwards.
        assert_eq!(c.load(memmap::FLASH_CACHE_BASE, 4), 0);
    }

    #[test]
    fn the_same_access_stream_costs_the_same_twice() {
        // Determinism at the smallest scale the model has: no host clock, no
        // hash seed, no allocator address reaches the answer.
        let run = || {
            let mut c = CacheCost::new();
            let mut total = 0u64;
            for i in 0..20_000u32 {
                let a = memmap::FLASH_CACHE_BASE + (i.wrapping_mul(2_654_435_761) & 0x000f_ffff);
                total += u64::from(c.fetch(a));
                total += u64::from(c.load(a ^ 0x40, 4));
                total += u64::from(c.store(0x6000_0000 + (i & 0xff) * 4, 4));
            }
            (total, c.fills(), c.hits(), c.apb_accesses())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn the_promote_order_is_an_exact_lru() {
        // way 3 to the front leaves 0,1,2 behind it in their old order.
        assert_eq!(promote(FRESH_ORDER, 3), 0b10_01_00_11);
        // Promoting the front way changes nothing.
        assert_eq!(promote(FRESH_ORDER, 0), FRESH_ORDER);
        // Every way still appears exactly once, from every starting order.
        for way in 0..WAYS as u8 {
            let o = promote(FRESH_ORDER, way);
            let mut seen = [false; WAYS];
            for slot in 0..WAYS as u8 {
                seen[((o >> (slot * 2)) & 3) as usize] = true;
            }
            assert!(seen.iter().all(|s| *s), "order {o:#010b} lost a way");
        }
    }

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
