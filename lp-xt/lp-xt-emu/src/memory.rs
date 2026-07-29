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
//! Original code; no derivation from QEMU/binutils (see the repo license ADR).

use crate::error::{Trap, TrapKind};

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
}

/// The emulator's physical address space: a set of regions, each optionally
/// carrying an [`AliasRule`] that makes its bytes fetchable at an I-bus view.
pub struct Memory {
    regions: Vec<Region>,
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
            _reserved: (),
        }
    }

    /// Add a plain read/write data region with no executable alias.
    pub fn add_ram(&mut self, dbus_start: u32, len: usize) {
        self.regions.push(Region {
            dbus_start,
            alias: None,
            data: vec![0u8; len],
            writable: true,
        });
    }

    /// Add a writable region whose bytes are also fetchable at the I-bus view
    /// given by `rule`. Window containment is the caller's business — the
    /// board profile validates its regions against its own dual-mapped window
    /// (see [`crate::board::BoardProfile::install`]).
    pub fn add_executable(&mut self, dbus_start: u32, len: usize, rule: AliasRule) {
        self.regions.push(Region {
            dbus_start,
            alias: Some(rule),
            data: vec![0u8; len],
            writable: true,
        });
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
        for (i, b) in bytes.iter().enumerate() {
            let a = addr.wrapping_add(i as u32);
            let (ri, idx) = self
                .resolve(a, Access::Data)
                .unwrap_or_else(|| panic!("load_bytes: address {a:#x} not mapped"));
            self.regions[ri].data[idx] = *b;
        }
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
