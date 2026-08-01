//! Flat, `Vec`-backed memory with per-region D-bus / I-bus dual mapping.
//!
//! The hardware fact this models (see `../FINDINGS.md`, E2 and C2): a byte
//! written at a D-bus (data) address may be fetchable at an I-bus
//! (instruction) alias address. The runner firmware writes payloads via the
//! D-bus view and *executes them at the I-bus view*, so self-addressing code
//! (`l32r` literals, `call8` targets) only behaves identically if the emulator
//! models the same alias — one backing store reachable at two address ranges.
//!
//! The *shape* of the alias is per-board (see [`AliasRule`]):
//! - ESP32-S3 SRAM1: a constant offset, `iram = dram + 0x6F_0000` (E2).
//! - Classic ESP32 SRAM1: **word-mirrored**, `iram = 0x400B_FFFC − (dram −
//!   0x3FFE_0000)` — the two windows run in opposite directions at word
//!   granularity, bytes within each word verbatim (C2b, 5 sentinels).
//!
//! Not every region is SRAM. Firmware `.text` executes from **flash through
//! the cache** (XIP), which the address space exposes as two read-only windows
//! — an executable IROM window and a data-only DROM window — so
//! [`Memory::add_rom`] adds regions that fault on a store. See
//! [`crate::board::BoardProfile`] for the per-chip addresses.
//!
//! Original code; no derivation from QEMU/binutils (see the repo license ADR).

use std::sync::{Arc, Mutex};

use crate::error::{Trap, TrapKind};

/// Base D-bus address of the **host-shared** region — the window a host
/// emulation engine maps its own buffer into so guest code can read and write
/// it without copying (see [`Memory::add_shared`]).
///
/// This address is **host-emulator fiction**. On real silicon a shader's vmctx
/// lives in ordinary DRAM and the on-device JIT (`rt_jit`) never needs a region
/// like this; the address exists only so a host engine can hand the guest a
/// pointer into host memory.
///
/// Chosen **reserved on both chips**, not merely unused by a profile:
/// `0x3000_0000` is below the lowest address either data bus decodes (the S3's
/// external-memory window opens at `0x3C00_0000`; classic's DROM0 opens at
/// `0x3F40_0000`), so no future profile can grow into it.
///
/// It used to be `0x3F40_0000`, on the weaker ground that no *profile* mapped
/// it. That stopped being true the moment the profiles gained modeled flash
/// windows: `0x3F40_0000` **is** classic's DROM base
/// ([`crate::board::BoardProfile::esp32`]). The `add_shared` overlap assertion
/// caught it, which is exactly what it is for; the fix is a base that is not a
/// hardware address at all.
///
/// It is deliberately **not** `lp_emu_core::DEFAULT_SHARED_START` (the rv32
/// engine's `0x4000_0000`): that address is [`crate::SENTINEL_PC`], the
/// unmapped return address the windowed run harness detects a top-level return
/// with. Mapping the shared region there would put the vmctx at the sentinel
/// and silently undermine the "chosen unmapped" property that harness relies
/// on. The two ISAs therefore use different shared bases, which costs nothing —
/// the guest reaches the region only through a pointer argument.
pub const SHARED_DBUS_BASE: u32 = 0x3000_0000;

/// ESP32-S3 SRAM1 D-bus window start (data view).
pub const SRAM1_DBUS_START: u32 = 0x3FC8_8000;
/// ESP32-S3 SRAM1 D-bus window end (exclusive).
pub const SRAM1_DBUS_END: u32 = 0x3FCF_0000;
/// Offset from an ESP32-S3 D-bus SRAM1 address to its I-bus alias.
pub const IBUS_ALIAS_OFFSET: u32 = 0x006F_0000;

/// How a region's D-bus (data) addresses map to I-bus (executable) addresses.
///
/// Expressed as a rule, not a constant, because the classic ESP32's SRAM1
/// mapping is not an offset (FINDINGS C2b). All variants map at *word*
/// granularity with bytes within each 32-bit word preserved verbatim
/// (little-endian, no swap) — hardware-verified on both boards.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AliasRule {
    /// `iram = dram + offset`. ESP32-S3 SRAM1 (`+0x6F_0000`, FINDINGS E2) and
    /// classic ESP32 RTC-fast (`+0xC4_0000`, FINDINGS C2a) are this shape.
    Offset(u32),
    /// The region's own address is directly fetchable — no separate D-bus
    /// view. Classic ESP32 SRAM0 is this shape (FINDINGS C2c/C2x).
    Identity,
    /// Word-mirrored: `iram_word = iram_top − (dram_word − dram_base)`, the
    /// two windows running in opposite directions word by word; the byte
    /// offset within each word is preserved. Classic ESP32 SRAM1 (FINDINGS
    /// C2b: H2 matched all 5 sentinels, H1-linear matched none).
    ///
    /// `dram_base`/`iram_top` are the *window* constants (classic:
    /// `0x3FFE_0000` / `0x400B_FFFC`), not region-relative — any region
    /// inside the window uses the same global rule.
    WordMirrored { dram_base: u32, iram_top: u32 },
}

impl AliasRule {
    /// The I-bus (executable) address of the byte at D-bus address `dbus`.
    pub fn dbus_to_ibus(&self, dbus: u32) -> u32 {
        match *self {
            AliasRule::Offset(o) => dbus.wrapping_add(o),
            AliasRule::Identity => dbus,
            AliasRule::WordMirrored {
                dram_base,
                iram_top,
            } => iram_top
                .wrapping_sub((dbus & !3).wrapping_sub(dram_base))
                .wrapping_add(dbus & 3),
        }
    }

    /// The D-bus (data) address of the byte at I-bus address `ibus`. Exact
    /// inverse of [`dbus_to_ibus`](Self::dbus_to_ibus) at byte granularity.
    pub fn ibus_to_dbus(&self, ibus: u32) -> u32 {
        match *self {
            AliasRule::Offset(o) => ibus.wrapping_sub(o),
            AliasRule::Identity => ibus,
            AliasRule::WordMirrored {
                dram_base,
                iram_top,
            } => dram_base
                .wrapping_add(iram_top.wrapping_sub(ibus & !3))
                .wrapping_add(ibus & 3),
        }
    }
}

/// A contiguous, `Vec`-backed memory region.
///
/// A region is addressable at its D-bus range `[dbus_start, dbus_start + len)`
/// for data access. If `alias` is set the same backing bytes are *also*
/// addressable — for both fetch and data — at the I-bus addresses the rule
/// maps the D-bus range to.
struct Region {
    dbus_start: u32,
    alias: Option<AliasRule>,
    data: Vec<u8>,
    writable: bool,
}

impl Region {
    /// Byte index within `data` for `addr` if it falls in the D-bus range.
    fn dbus_index(&self, addr: u32) -> Option<usize> {
        let end = self.dbus_start.wrapping_add(self.data.len() as u32);
        if addr >= self.dbus_start && addr < end {
            Some((addr - self.dbus_start) as usize)
        } else {
            None
        }
    }

    /// Byte index within `data` for `addr` if it falls in the I-bus view.
    ///
    /// Maps `addr` back to its D-bus counterpart via the alias rule; the
    /// D-bus range check then gates whether it lands in this region (an
    /// address outside the aliased image maps outside the range and misses).
    fn ibus_index(&self, addr: u32) -> Option<usize> {
        let rule = self.alias?;
        self.dbus_index(rule.ibus_to_dbus(addr))
    }

    /// Inclusive `[lo, hi]` bounds of this region's I-bus image, or `None` when
    /// it has no alias. Conservative: computed from the endpoints and rounded
    /// out to word boundaries, because a word-mirrored alias runs *downward*
    /// and so maps the region's low D-bus address to its image's high end.
    fn ibus_bounds(&self) -> Option<(u32, u32)> {
        let rule = self.alias?;
        let last = self.dbus_start.wrapping_add(self.data.len() as u32 - 1);
        let a = rule.dbus_to_ibus(self.dbus_start);
        let b = rule.dbus_to_ibus(last);
        Some((a.min(b) & !3, (a.max(b) | 3)))
    }

    /// Every inclusive address range this region answers to, each with the name
    /// of the view it comes from: its D-bus range, plus its I-bus image when it
    /// has one. Used by the overlap checks — an address in *either* view
    /// resolves to this region, so both count.
    ///
    /// Under [`AliasRule::Identity`] the two coincide and the duplicate is
    /// harmless: the checks are pure comparisons.
    fn address_views(&self) -> impl Iterator<Item = (&'static str, u32, u32)> {
        let dbus = (
            "D-bus range",
            self.dbus_start,
            self.dbus_start.wrapping_add(self.data.len() as u32 - 1),
        );
        core::iter::once(dbus).chain(self.ibus_bounds().map(|(lo, hi)| ("I-bus image", lo, hi)))
    }
}

/// A region whose bytes are owned by the **host**, mapped into the guest's data
/// space so both sides see the same memory (see [`Memory::add_shared`]).
///
/// Modeled on `lp-emu-core`'s `shared_backing`: one field on [`Memory`] rather
/// than a variant of [`Region`], so the region list and its alias machinery are
/// untouched and each typed access takes the lock exactly once.
struct SharedRegion {
    dbus_start: u32,
    /// Length captured when the region was added. The backing must not be
    /// resized afterwards; accesses bounds-check against the live `Vec` anyway.
    len: usize,
    backing: Arc<Mutex<Vec<u8>>>,
}

impl SharedRegion {
    fn index(&self, addr: u32) -> Option<usize> {
        let end = self.dbus_start.wrapping_add(self.len as u32);
        if addr >= self.dbus_start && addr < end {
            Some((addr - self.dbus_start) as usize)
        } else {
            None
        }
    }
}

/// The emulator's physical address space: a set of regions, each optionally
/// carrying an [`AliasRule`] that makes its bytes fetchable at an I-bus view.
pub struct Memory {
    regions: Vec<Region>,
    /// Host-owned data window, when one has been attached
    /// ([`add_shared`](Self::add_shared)). Deliberately not a [`Region`]: it is
    /// never fetchable, and its bytes live behind a lock.
    shared: Option<SharedRegion>,
    /// Max number of bytes accessible from any address (for load/store bounds).
    #[allow(dead_code, reason = "layout placeholder kept from the source repo")]
    _reserved: (),
}

/// How a resolved address may be used.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Access {
    /// Data load/store — permitted at either the D-bus or I-bus view.
    Data,
    /// Instruction fetch — permitted only at the I-bus (executable) view. A
    /// fetch of a D-bus-only address models the hardware `InstrFetchError`
    /// (FINDINGS E2D: jumping to the D-bus address faults, EXCCAUSE 2).
    Fetch,
}

impl Memory {
    /// Empty address space.
    pub fn new() -> Memory {
        Memory {
            regions: Vec::new(),
            shared: None,
            _reserved: (),
        }
    }

    /// Add a plain read/write data region with no executable alias.
    pub fn add_ram(&mut self, dbus_start: u32, len: usize) {
        self.push_region(dbus_start, len, None, true);
    }

    /// Add a writable region whose bytes are also fetchable at the I-bus view
    /// given by `rule`. Window containment is the caller's business — the
    /// board profile validates its regions against its own dual-mapped window
    /// (see [`crate::board::BoardProfile::install`]).
    pub fn add_executable(&mut self, dbus_start: u32, len: usize, rule: AliasRule) {
        self.push_region(dbus_start, len, Some(rule), true);
    }

    /// Add a **read-only** region: guest loads and fetches behave as for any
    /// other region, guest stores fault with [`EXC_LOAD_STORE_ERROR`].
    ///
    /// This is how flash-resident firmware is modeled. On every ESP32 the
    /// application's `.text` and `.rodata` execute and load *from flash through
    /// the cache* (XIP), which the address space exposes as read-only windows;
    /// a store there is a bug on hardware and is a bug here.
    ///
    /// `alias` is `Some(AliasRule::Identity)` for an executable (IROM) window —
    /// flash instruction addresses are already the fetch addresses, there is no
    /// separate data view to alias from — and `None` for a data-only (DROM)
    /// window, which then faults on fetch exactly as any data region does.
    ///
    /// The loader paths ([`load_bytes`](Self::load_bytes),
    /// [`load_region`](Self::load_region)) deliberately ignore the read-only
    /// flag: placing an image into flash is what a flasher does, not what the
    /// guest does.
    pub fn add_rom(&mut self, dbus_start: u32, len: usize, alias: Option<AliasRule>) {
        self.push_region(dbus_start, len, alias, false);
    }

    /// Install a region, asserting it overlaps no existing one — in either
    /// address view.
    ///
    /// The board profiles now install five regions apiece across three address
    /// quadrants (SRAM code, SRAM data, stack, flash IROM, flash DROM), and
    /// [`resolve`](Self::resolve) is first-match-wins, so an overlap would not
    /// fault — it would silently shadow. [`add_shared`](Self::add_shared) has
    /// asserted this for its one window since it was added; the regions
    /// themselves now get the same guarantee from the same check.
    fn push_region(
        &mut self,
        dbus_start: u32,
        len: usize,
        alias: Option<AliasRule>,
        writable: bool,
    ) {
        assert!(len > 0, "region at {dbus_start:#x} is empty");
        let region = Region {
            dbus_start,
            alias,
            data: vec![0u8; len],
            writable,
        };
        for (view, lo, hi) in region.address_views() {
            self.assert_free((lo, hi), "new region's", view);
        }
        self.regions.push(region);
    }

    /// Assert `[lo, hi]` overlaps no installed region, in either view.
    ///
    /// `what`/`new_view` only name the range in the panic message, so they stay
    /// `&str`: this runs on every region add, and an emulator is built per host
    /// call. Nothing is formatted unless the assertion actually fires.
    fn assert_free(&self, (lo, hi): (u32, u32), what: &str, new_view: &str) {
        for r in &self.regions {
            for (view, r_lo, r_hi) in r.address_views() {
                assert!(
                    hi < r_lo || lo > r_hi,
                    "{what} {new_view} {lo:#x}..={hi:#x} overlaps the {view} \
                     {r_lo:#x}..={r_hi:#x} of an installed region"
                );
            }
        }
    }

    /// S3 convenience: add a region backing the ESP32-S3 SRAM1 dual mapping —
    /// writable via the D-bus range and fetchable/readable via the I-bus alias
    /// `+0x6F_0000`. Asserts the S3 window; other boards go through
    /// [`add_executable`](Self::add_executable) with their own [`AliasRule`].
    pub fn add_sram1(&mut self, dbus_start: u32, len: usize) {
        assert!(
            (SRAM1_DBUS_START..SRAM1_DBUS_END).contains(&dbus_start),
            "SRAM1 region base {dbus_start:#x} outside the S3 dual-mapped window"
        );
        self.add_executable(dbus_start, len, AliasRule::Offset(IBUS_ALIAS_OFFSET));
    }

    /// Map a **host-owned** buffer into the guest's data space at `dbus_start`,
    /// so guest loads and stores there read and write the host's bytes directly
    /// — no copying, and the host sees guest writes as soon as it takes the
    /// lock. This is how a host emulation engine hands compiled shader code its
    /// vmctx, uniforms, globals and texture buffers.
    ///
    /// The region is **data only**: it carries no [`AliasRule`], so an
    /// instruction fetch from it takes the ordinary
    /// [`EXC_INSTR_FETCH_ERROR`] path. That is correct — jumping into the vmctx
    /// is a bug, and modeling it as one keeps the emulator honest.
    ///
    /// Call this *after* the board profile has installed its regions
    /// ([`crate::board::BoardProfile::install`]); the overlap assertion below is
    /// what keeps the chosen base ([`SHARED_DBUS_BASE`]) honest as profiles
    /// change, rather than a comment claiming the address is free.
    ///
    /// # Panics
    /// If a shared region is already attached, if `backing` is empty, or if the
    /// range overlaps any installed region — in either its D-bus range or its
    /// I-bus image.
    pub fn add_shared(&mut self, dbus_start: u32, backing: Arc<Mutex<Vec<u8>>>) {
        assert!(
            self.shared.is_none(),
            "a shared region is already attached at {:#x}",
            self.shared.as_ref().map_or(0, |s| s.dbus_start)
        );
        let len = backing.lock().expect("shared backing lock").len();
        assert!(len > 0, "shared backing is empty");
        let lo = dbus_start;
        let hi = dbus_start.wrapping_add(len as u32 - 1);
        self.assert_free((lo, hi), "shared region", "");
        self.shared = Some(SharedRegion {
            dbus_start,
            len,
            backing,
        });
    }

    /// The I-bus (executable) alias of a D-bus **ESP32-S3** SRAM1 address.
    /// S3-specific; profile-aware code uses [`AliasRule::dbus_to_ibus`] /
    /// [`crate::board::BoardProfile::code_ibus_base`] instead.
    pub fn ibus_alias(dbus_addr: u32) -> u32 {
        dbus_addr.wrapping_add(IBUS_ALIAS_OFFSET)
    }

    /// Resolve `addr` for `access`, returning `(region_index, byte_index)`.
    fn resolve(&self, addr: u32, access: Access) -> Option<(usize, usize)> {
        // Fetch: only the executable (I-bus alias) view is valid.
        for (ri, r) in self.regions.iter().enumerate() {
            if let Some(idx) = r.ibus_index(addr) {
                return Some((ri, idx));
            }
            if access == Access::Data {
                if let Some(idx) = r.dbus_index(addr) {
                    return Some((ri, idx));
                }
            }
        }
        None
    }

    /// Copy `bytes` into mapped memory starting at `addr` (data write, ignores
    /// the writable flag — this is loader setup, not guest execution). `addr`
    /// may be either view; each byte is resolved individually, so loading a
    /// blob at contiguous **I-bus** addresses lands the bytes correctly even
    /// under a word-mirrored alias (where the backing D-bus image is not
    /// contiguous).
    pub fn load_bytes(&mut self, addr: u32, bytes: &[u8]) {
        if let Err(a) = self.try_load_bytes(addr, bytes) {
            panic!("load_bytes: address {a:#x} not mapped");
        }
    }

    /// Fallible [`load_bytes`](Self::load_bytes): on failure returns the first
    /// unmapped address instead of panicking.
    ///
    /// An image loader is given whatever addresses its ELF names, and "this
    /// segment is not in the modeled map" is a diagnosable condition its caller
    /// should report — not a panic from inside the memory model.
    pub fn try_load_bytes(&mut self, addr: u32, bytes: &[u8]) -> Result<(), u32> {
        if let Some(s) = &self.shared {
            if let Some(idx) = s.index(addr) {
                let mut v = s.backing.lock().expect("shared backing lock");
                let end = idx + bytes.len();
                assert!(
                    end <= v.len(),
                    "load_bytes: {} bytes at {addr:#x} run past the shared region",
                    bytes.len()
                );
                v[idx..end].copy_from_slice(bytes);
                return Ok(());
            }
        }
        for (i, b) in bytes.iter().enumerate() {
            let a = addr.wrapping_add(i as u32);
            let (ri, idx) = self.resolve(a, Access::Data).ok_or(a)?;
            self.regions[ri].data[idx] = *b;
        }
        Ok(())
    }

    /// Zero `len` bytes at `addr` through the loader path (the `p_memsz` tail of
    /// a `PT_LOAD` segment). Fallible for the same reason
    /// [`try_load_bytes`](Self::try_load_bytes) is.
    pub fn try_zero(&mut self, addr: u32, len: u32) -> Result<(), u32> {
        const CHUNK: usize = 4096;
        let zeros = [0u8; CHUNK];
        let mut done = 0u32;
        while done < len {
            let n = ((len - done) as usize).min(CHUNK);
            self.try_load_bytes(addr.wrapping_add(done), &zeros[..n])?;
            done += n as u32;
        }
        Ok(())
    }

    /// Copy `bytes` into the single region that starts at `dbus_start`, in one
    /// `copy_from_slice`.
    ///
    /// [`load_bytes`](Self::load_bytes) resolves every byte individually — it has
    /// to, because under a word-mirrored alias a contiguous I-bus blob is not
    /// contiguous in the backing store. That per-byte resolve is fine for a small
    /// payload and far too slow for a host engine reloading a whole ~128 KiB code
    /// region before every call. This is the bulk path for that case: `dbus_start`
    /// must be a region's exact base, and `bytes` must fit it.
    ///
    /// # Panics
    /// If no region starts at `dbus_start`, or `bytes` is longer than it.
    pub fn load_region(&mut self, dbus_start: u32, bytes: &[u8]) {
        let r = self
            .regions
            .iter_mut()
            .find(|r| r.dbus_start == dbus_start)
            .unwrap_or_else(|| panic!("load_region: no region based at {dbus_start:#x}"));
        assert!(
            bytes.len() <= r.data.len(),
            "load_region: {} bytes do not fit the {} byte region at {dbus_start:#x}",
            bytes.len(),
            r.data.len()
        );
        r.data[..bytes.len()].copy_from_slice(bytes);
    }

    // --- fetch ---

    /// Read up to `n` (1..=3) instruction bytes for decode, honoring fetch
    /// permission. Returns fewer bytes only at the very end of a region.
    pub fn fetch(&self, pc: u32, out: &mut [u8; 3]) -> Result<usize, Trap> {
        // The first byte must be fetchable; that classifies the address.
        if self.resolve(pc, Access::Fetch).is_none() {
            return Err(Trap {
                kind: TrapKind::Exception,
                cause: EXC_INSTR_FETCH_ERROR,
                pc,
                vaddr: pc,
            });
        }
        let mut got = 0;
        for i in 0..3u32 {
            match self.resolve(pc.wrapping_add(i), Access::Fetch) {
                Some((ri, idx)) => {
                    out[i as usize] = self.regions[ri].data[idx];
                    got += 1;
                }
                None => break,
            }
        }
        Ok(got)
    }

    // --- typed data access ---

    fn read_bytes(&self, addr: u32, n: u32) -> Result<u32, Trap> {
        // Shared region first: one lock per typed access, not per byte
        // (lp-emu-core's `shared_backing` sets the same granularity). An access
        // starting inside the window must finish inside it — the shared region
        // has no neighbours to straddle into.
        if let Some(s) = &self.shared {
            if let Some(idx) = s.index(addr) {
                let g = s.backing.lock().expect("shared backing lock");
                let mut v = 0u32;
                for i in 0..n as usize {
                    let b = *g.get(idx + i).ok_or_else(|| self.load_fault(addr))?;
                    v |= (b as u32) << (8 * i as u32);
                }
                return Ok(v);
            }
        }
        let mut v = 0u32;
        for i in 0..n {
            let a = addr.wrapping_add(i);
            match self.resolve(a, Access::Data) {
                Some((ri, idx)) => v |= (self.regions[ri].data[idx] as u32) << (8 * i),
                None => return Err(self.load_fault(addr)),
            }
        }
        Ok(v)
    }

    fn write_bytes(&mut self, addr: u32, n: u32, val: u32) -> Result<(), Trap> {
        if let Some(s) = &self.shared {
            if let Some(idx) = s.index(addr) {
                let mut g = s.backing.lock().expect("shared backing lock");
                for i in 0..n as usize {
                    match g.get_mut(idx + i) {
                        Some(slot) => *slot = (val >> (8 * i as u32)) as u8,
                        None => return Err(self.store_fault(addr)),
                    }
                }
                return Ok(());
            }
        }
        for i in 0..n {
            let a = addr.wrapping_add(i);
            match self.resolve(a, Access::Data) {
                Some((ri, idx)) => {
                    if !self.regions[ri].writable {
                        return Err(self.store_fault(addr));
                    }
                    self.regions[ri].data[idx] = (val >> (8 * i)) as u8;
                }
                None => return Err(self.store_fault(addr)),
            }
        }
        Ok(())
    }

    pub fn read_u8(&self, addr: u32) -> Result<u8, Trap> {
        Ok(self.read_bytes(addr, 1)? as u8)
    }
    pub fn read_u16(&self, addr: u32) -> Result<u16, Trap> {
        Ok(self.read_bytes(addr, 2)? as u16)
    }
    pub fn read_u32(&self, addr: u32) -> Result<u32, Trap> {
        self.read_bytes(addr, 4)
    }
    pub fn write_u8(&mut self, addr: u32, v: u8) -> Result<(), Trap> {
        self.write_bytes(addr, 1, v as u32)
    }
    pub fn write_u16(&mut self, addr: u32, v: u16) -> Result<(), Trap> {
        self.write_bytes(addr, 2, v as u32)
    }
    pub fn write_u32(&mut self, addr: u32, v: u32) -> Result<(), Trap> {
        self.write_bytes(addr, 4, v)
    }

    fn load_fault(&self, addr: u32) -> Trap {
        Trap {
            kind: TrapKind::Exception,
            cause: EXC_LOAD_STORE_ERROR,
            pc: 0,
            vaddr: addr,
        }
    }
    fn store_fault(&self, addr: u32) -> Trap {
        Trap {
            kind: TrapKind::Exception,
            cause: EXC_LOAD_STORE_ERROR,
            pc: 0,
            vaddr: addr,
        }
    }
}

impl Default for Memory {
    fn default() -> Self {
        Memory::new()
    }
}

/// EXCCAUSE for an instruction fetch to a non-executable address (matches the
/// S3's `InstrFetchError`; FINDINGS E2D observed EXCCAUSE 2).
pub const EXC_INSTR_FETCH_ERROR: u32 = 2;
/// EXCCAUSE for a bad load/store address (`LoadStoreErrorCause`).
pub const EXC_LOAD_STORE_ERROR: u32 = 3;
