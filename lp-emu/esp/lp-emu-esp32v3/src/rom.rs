//! The classic mask ROM: where its segments go, the data its `PT_LOAD`s do
//! **not** carry, and the hook table that is deliberately empty.
//!
//! # The ROM is loaded in every configuration
//!
//! Vision D6, plan PD7, and it is if anything more true on the classic than
//! on the C6: `fw-esp32v3` resolves `memcpy`, `memset`, the `str*` family,
//! `ets_delay_us` and `rtc_get_reset_reason` into the mask ROM through
//! esp-hal's linker script, so the ROM image is part of the memory map and
//! not an extra for a ROM-up boot. What is optional is running the boot
//! chain from the reset vector, which is M3 P7.
//!
//! # Three jobs, and why the middle one is the load-bearing one
//!
//! 1. [`load`] places every `PT_LOAD`. The vendored `esp32_rev300_rom.elf`
//!    has **39** of them and twenty are empty — eight `.bss_*` NOBITS at real
//!    DRAM addresses and twelve at `vaddr = 0` — so they are skipped and
//!    counted rather than placed at address zero. Two segments have
//!    `p_vaddr != p_paddr` (the four-byte `.from_now_on_*` pair, `m3/notes.md`
//!    §2); both place correctly by vaddr, and [`PlacedSegment`] records the
//!    mismatch rather than picking silently.
//!
//! 2. [`seed_data`] places the sections **no `PT_LOAD` covers**. The classic
//!    ROM's initialised data lives in `PROGBITS` sections flagged `W` but
//!    **not** `A`: they belong to no program header, so placing program
//!    headers alone leaves every one of them zero. The eight non-empty ones
//!    total about 7.8 KiB, and the one to know by name is `.data_xtos_pro` at
//!    `0x3FFE_0440` — 1,056 bytes of the ROM's xtos dispatch tables, and also
//!    exactly where the firmware later puts heap region 0 (L0's `[INIT] heap
//!    regions: 0 0x3ffe0440+15072`). Zeroing it does not fail here. It fails
//!    somewhere else, later, unrecognisably.
//!
//! 3. [`HookTable`] is **empty**, and the rule for adding to it is: try the
//!    real ROM path first. A hook is a last resort, for a routine whose real
//!    path cannot be made to work by giving a register a peripheral model
//!    owns its documented reset value.
//!
//! # Why the classic needs no `seed_data_image`
//!
//! The C6's [`DataImage`] equivalent had to *reconstruct* a copy image: its
//! ROM ELF is a debug view whose `_data_table_start` triples point at source
//! bytes no section of the ELF carries. The classic's do not: every
//! non-alloc `.data_*` section carries its bytes at a real file offset
//! (`.data_xtos_pro` at file `0x06EA10`, and so on), so [`seed_data`] reads
//! them directly and [`seed_data_image`] is **reporting only** — which of
//! them were seeded, how many bytes, how many were empty. Ruling **R2**: seed
//! every non-alloc `.data_*`, report the list, and assert it in `boot.rs` —
//! never "the ones a boot needed".

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use lp_emu_esp_common::{ElfImage, SocBus};

/// Force an `include_bytes!` blob to 8-byte alignment.
///
/// `include_bytes!` promises nothing about alignment, and `object`'s ELF
/// reader casts into the buffer, so an unaligned blob fails to parse with
/// "Invalid ELF header size or alignment" — an error that reads like a
/// corrupt file and is not one. The zero-length leading array is the
/// standard way to raise a struct's alignment without adding a byte to it.
/// The same shim, for the same reason, as `lp-emu-esp32c6/src/rom.rs:46-74`.
#[repr(C)]
struct Aligned<A, B: ?Sized> {
    _align: [A; 0],
    bytes: B,
}

/// The vendored file's length, spelled out in the type so a ROM swapped for
/// a **different chip's** fails to compile here, before any checksum test
/// runs.
const ROM_BYTES: usize = 857_500;

static VENDORED_V3_ROM_ALIGNED: &Aligned<u64, [u8; ROM_BYTES]> = &Aligned {
    _align: [],
    bytes: *include_bytes!("../../roms/esp32_rev300_rom.elf"),
};

/// The vendored ESP32 revision-300 mask ROM, compiled in.
///
/// Embedded rather than read from a path so that a test, a `cargo install`ed
/// binary and a CI runner all load the same 857,500 bytes without agreeing on
/// a working directory. `--rom <path>` overrides it; the sha256 of these
/// bytes is checked against `lp-emu/esp/roms/SHA256SUMS` by
/// `tests/rom_vendoring.rs`, which hashes **this** static rather than a
/// second copy of the blob.
pub static VENDORED_V3_ROM: &[u8] = &VENDORED_V3_ROM_ALIGNED.bytes;

/// `break 1, 15` — the **three-byte** form (`imms = 1`, `immt = 15`).
///
/// Xtensa has a two-byte `break.n` as well, and the three-byte form is the
/// one used here for the C6's reason (`lp-emu-esp32c6/src/rom.rs:75-81`,
/// adapted): every ROM entry point this could ever hook begins with a
/// three-byte instruction, and a two-byte patch would leave the last byte of
/// one behind for the `--hooks` listing to misreport as an instruction.
///
/// The value is **not** derived by analogy. `tests/boot.rs` asserts it equals
/// `lp_xt_inst::encode(&Inst::Break(1, 15))`, because M0 found a real
/// misdecode in this repo that came from trusting an encoding derived by
/// hand (the OP-IMM bitmanip transcription: 7 of 14 arms were wrong).
pub const BREAK_1_15: u32 = 0x0000_41F0;

/// [`BREAK_1_15`] as the three bytes a patch writes, little-endian.
pub const BREAK_1_15_BYTES: [u8; 3] = [0xF0, 0x41, 0x00];

/// Where the ROM's bytes went. The phase report lists these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedSegment {
    pub vaddr: u32,
    pub paddr: u32,
    pub filesz: u32,
    pub memsz: u32,
    pub execute: bool,
    /// The region names the bytes landed in, in address order. More than one
    /// when a segment spans a region boundary.
    pub regions: Vec<&'static str>,
    /// This segment's file bytes are the **ELF's own header and program
    /// header table**, not guest content. See [`is_header_map`].
    pub header_mapped: bool,
}

impl PlacedSegment {
    /// `p_vaddr != p_paddr`. Two of the classic ROM's segments are like this
    /// (the `.from_now_on_*` four-byte pair); both are placed by **vaddr**,
    /// and this says so rather than leaving a reader to wonder which was
    /// picked.
    pub const fn relocated(&self) -> bool {
        self.vaddr != self.paddr
    }
}

/// Something the ROM image and this memory map disagree about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RomError {
    Elf(String),
    /// A `PT_LOAD` or a seeded section covers an address no region claims.
    /// The brief's stop-and-report case: either the ROM is not the chip we
    /// think it is, or [`crate::memmap`] is wrong.
    Unmapped { vaddr: u32, memsz: u32, at: u32 },
    /// A hooked symbol is not in the ROM's symbol table.
    NoSuchSymbol(String),
}

impl fmt::Display for RomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RomError::Elf(m) => write!(f, "ROM ELF: {m}"),
            RomError::Unmapped { vaddr, memsz, at } => write!(
                f,
                "ROM PT_LOAD at 0x{vaddr:08x} (memsz 0x{memsz:x}) reaches 0x{at:08x}, \
                 which no region of the classic ESP32 map claims"
            ),
            RomError::NoSuchSymbol(s) => write!(f, "the ROM has no symbol `{s}`"),
        }
    }
}

impl std::error::Error for RomError {}

/// What a host hook did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookResult {
    /// The host function stood in for the ROM routine: return to the caller
    /// (`pc = a0`), with the return registers already set by the hook.
    Ret,
    /// The hook declined after all; give the guest the architectural
    /// breakpoint. Same as having no hook at the address.
    Breakpoint,
    /// Stop the run here, with the guest untouched: `pc` still at the
    /// symbol, every register as the caller left it. `--break-at`'s answer.
    Stop,
}

/// A host stand-in for a ROM routine.
///
/// It sees the whole machine, because a hook that could not read guest memory
/// (a string to print, a buffer to fill) would only be able to stand in for
/// routines that take no pointers — which is none of the interesting ones.
pub type HostHook = fn(&mut crate::machine::Machine) -> HookResult;

/// One installed hook.
#[derive(Clone, Copy)]
pub struct Hook {
    pub symbol: &'static str,
    pub address: u32,
    /// The three instruction bytes the `break` replaced, so the table can be
    /// uninstalled and so `--hooks` can show what was displaced.
    pub original: [u8; 3],
    pub call: HostHook,
}

impl fmt::Debug for Hook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hook")
            .field("symbol", &self.symbol)
            .field("address", &format_args!("{:#010x}", self.address))
            .field("original", &format_args!("{:02x?}", self.original))
            .finish()
    }
}

/// `pc` → host hook. **Empty by default**; see the module docs for the rule.
#[derive(Clone, Debug, Default)]
pub struct HookTable {
    by_pc: BTreeMap<u32, Hook>,
}

impl HookTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.by_pc.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_pc.len()
    }

    /// The hook claiming `pc`, if any.
    pub fn get(&self, pc: u32) -> Option<Hook> {
        self.by_pc.get(&pc).copied()
    }

    /// Every hook, in address order. What `--hooks` prints.
    pub fn hooks(&self) -> impl Iterator<Item = &Hook> {
        self.by_pc.values()
    }

    /// Patch `symbol`'s first instruction to [`BREAK_1_15`] and claim its
    /// address.
    ///
    /// The bytes that were there are kept in the returned table entry. Fails
    /// when the ROM has no such symbol — a hook aimed at a name that moved
    /// between ROM revisions must not silently do nothing.
    pub fn install(
        &mut self,
        bus: &mut SocBus,
        rom: &ElfImage,
        symbol: &'static str,
        call: HostHook,
    ) -> Result<u32, RomError> {
        let sym = rom
            .symbol(symbol)
            .ok_or_else(|| RomError::NoSuchSymbol(symbol.to_string()))?;
        self.install_at(bus, sym.address, symbol, call)
    }

    /// [`install`](Self::install) at an address already resolved — for a
    /// symbol found by demangled path, whose ELF name is not what the
    /// listing should show.
    pub fn install_at(
        &mut self,
        bus: &mut SocBus,
        address: u32,
        symbol: &'static str,
        call: HostHook,
    ) -> Result<u32, RomError> {
        let original = read_three(bus, address)?;
        bus.load_image(address, &BREAK_1_15_BYTES)
            .map_err(|e| RomError::Elf(format!("patching `{symbol}`: {e}")))?;
        self.by_pc.insert(
            address,
            Hook {
                symbol,
                address,
                original,
                call,
            },
        );
        Ok(address)
    }

    /// Put every displaced instruction back and forget the table.
    pub fn uninstall_all(&mut self, bus: &mut SocBus) {
        for hook in self.by_pc.values() {
            if let Err(e) = bus.load_image(hook.address, &hook.original) {
                log::warn!(
                    "rom: could not restore `{}` at {:#010x}: {e}",
                    hook.symbol,
                    hook.address
                );
            }
        }
        self.by_pc.clear();
    }
}

/// The three bytes at `address`, read through the bus's own decode so the
/// patch cannot land somewhere the guest would not fetch from.
fn read_three(bus: &mut SocBus, address: u32) -> Result<[u8; 3], RomError> {
    use lp_emu_core::Bus;
    let mut out = [0u8; 3];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = bus
            .read_u8(address + i as u32)
            .map_err(|e| RomError::Elf(format!("reading {:#010x}: {e}", address + i as u32)))?;
    }
    Ok(out)
}

/// Parse a ROM ELF from bytes.
pub fn parse(bytes: &[u8]) -> Result<ElfImage, RomError> {
    ElfImage::parse(bytes).map_err(|e| RomError::Elf(e.to_string()))
}

/// Read a ROM ELF from a file.
pub fn parse_file(path: &Path) -> Result<ElfImage, RomError> {
    let bytes = std::fs::read(path)
        .map_err(|e| RomError::Elf(format!("reading {}: {e}", path.display())))?;
    parse(&bytes)
}

/// The vendored ROM, parsed.
pub fn vendored() -> Result<ElfImage, RomError> {
    parse(VENDORED_V3_ROM)
}

/// Place every `PT_LOAD` of the ROM into the bus.
///
/// The ELF says where; nothing here assumes. Empty segments are skipped and
/// counted ([`empty_segments`]), a segment that straddles a region boundary
/// is chunked rather than refused, and a segment whose bytes fall outside
/// every region is [`RomError::Unmapped`] — the stop-and-report case.
pub fn load(bus: &mut SocBus, rom: &ElfImage) -> Result<Vec<PlacedSegment>, RomError> {
    let mut placed = Vec::new();
    for seg in &rom.segments {
        if seg.memsz == 0 {
            continue;
        }
        let header_mapped = is_header_map(&seg.data);
        let (at, data, memsz) = if header_mapped {
            // The file half is the ELF's own headers; only the zero-fill
            // tail is guest memory. See `is_header_map`.
            let skip = seg.filesz();
            (
                seg.vaddr + skip,
                &[][..],
                seg.memsz.saturating_sub(skip),
            )
        } else {
            (seg.vaddr, &seg.data[..], seg.memsz)
        };
        let regions = if memsz == 0 {
            Vec::new()
        } else {
            place_spanning(bus, at, data, memsz)?
        };
        placed.push(PlacedSegment {
            vaddr: seg.vaddr,
            paddr: seg.paddr,
            filesz: seg.filesz(),
            memsz: seg.memsz,
            execute: seg.execute,
            regions,
            header_mapped,
        });
    }
    Ok(placed)
}

/// Is this segment's file content the **ELF's own header and program header
/// table** rather than guest memory?
///
/// ⚠️ **A fact of this ROM that `m3/notes.md` §2 did not predict, and it is
/// load-bearing.** The vendored `esp32_rev300_rom.elf`'s first `PT_LOAD` is
///
/// ```text
/// LOAD  Offset 0x000000  VirtAddr 0x3ffadafc  FileSiz 0x00514  MemSiz 0x00534  RW
///       .bss_hal .bss_ets .bss_cache .bss_newlib
/// ```
///
/// — `p_offset = 0`, so its file bytes are the 52-byte ELF header plus the
/// 39 × 32-byte program headers, exactly `0x514`. Every section it covers is
/// `NOBITS`. The linker pulled the segment's `vaddr` **down** by `0x514` from
/// where the sections actually live (`0x3FFA_E010`) so that the headers would
/// fit in front of them.
///
/// So placing this segment by vaddr the ordinary way would write the ELF's
/// own header at `0x3FFA_DAFC` — 1,284 bytes below anything the classic's map
/// claims, which is how it was found: `RomError::Unmapped { vaddr:
/// 0x3ffadafc, memsz: 0x534 }` on the very first build. It is not a memory
/// map bug. Those bytes are not in the mask ROM at all.
///
/// The signature is the ELF magic at the start of the file bytes, which is
/// the definition of `p_offset = 0` and is what `lp-emu-esp-common`'s
/// [`ElfImage`] view exposes (it keeps the bytes, not the offset).
/// `tests/boot.rs` cross-checks that the length is exactly the header table's.
pub fn is_header_map(data: &[u8]) -> bool {
    data.starts_with(b"\x7fELF")
}

/// How many `PT_LOAD`s an image declares with nothing in them. Reported so
/// "39 program headers, 19 placed" is not a discrepancy anyone has to chase.
pub fn empty_segments(image: &ElfImage) -> usize {
    image.segments.iter().filter(|s| s.memsz == 0).count()
}

/// One ROM data section seeded by [`seed_data`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeededSection {
    pub name: String,
    pub address: u32,
    pub len: u32,
}

/// Seed the ROM's initialised data — the `PROGBITS` sections flagged `W` but
/// not `A`, which no `PT_LOAD` covers.
///
/// See the module docs for why this is the load-bearing half. The list is
/// read from the ELF ([`lp_emu_esp_common::elf::InitSection`]) and never
/// hardcoded: a ROM revision that moves `.data_xtos_pro` moves the seed with
/// it, and one that adds a section gets it seeded without a code change.
///
/// Call it after [`load`] and before an application image is placed, so a
/// direct load overwrites what the app owns exactly as the real bootloader
/// does.
pub fn seed_data(bus: &mut SocBus, rom: &ElfImage) -> Result<Vec<SeededSection>, RomError> {
    let mut seeded = Vec::new();
    for section in &rom.init_sections {
        if section.data.is_empty() {
            continue;
        }
        let len = section.data.len() as u32;
        place_spanning(bus, section.address, &section.data, len)?;
        seeded.push(SeededSection {
            name: section.name.clone(),
            address: section.address,
            len,
        });
    }
    Ok(seeded)
}

/// What [`seed_data`] placed, in the shape `boot.rs` and the run report
/// assert on.
///
/// Reporting only on the classic — see the module docs.
///
/// There is no "skipped empty" count, and the reason is worth one line: the
/// ROM ELF really does declare a family of zero-length non-alloc sections
/// (`.data_hal`, `.data_ets`, `.data_uart_*`, `.data_all_*` and the rest),
/// but `lp-emu-esp-common`'s ELF view drops them before this crate sees them
/// (`elf.rs:174`). Counting them here would mean re-parsing the ELF for a
/// number that is a fact about the file, not about the run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DataImage {
    pub sections: usize,
    pub bytes: u32,
}

/// Report what [`seed_data`] did. Takes the seeded list rather than the bus
/// so it cannot disagree with it.
pub fn seed_data_image(seeded: &[SeededSection]) -> DataImage {
    DataImage {
        sections: seeded.len(),
        bytes: seeded.iter().map(|s| s.len).sum(),
    }
}

/// Write `data` at `address`, then zero out to `memsz`, splitting the write
/// at region boundaries and naming every region touched.
///
/// Host-side placement writes **bytes**, including into SRAM0, whose
/// word-only rule is a *guest* rule (`memmap`'s module docs). This is the
/// emulator putting memory into the state silicon was handed, not the guest
/// reaching through the instruction bus.
pub(crate) fn place_spanning(
    bus: &mut SocBus,
    address: u32,
    data: &[u8],
    memsz: u32,
) -> Result<Vec<&'static str>, RomError> {
    let total = memsz.max(data.len() as u32);
    let mut names: Vec<&'static str> = Vec::new();
    let mut at = address;
    let mut done = 0u32;

    while done < total {
        let (name, room) = bus
            .regions()
            .iter()
            .find(|r| r.contains(at))
            .map(|r| (r.name, r.end() - at))
            .ok_or(RomError::Unmapped {
                vaddr: address,
                memsz: total,
                at,
            })?;
        let chunk = room.min(total - done);

        // The file half of this chunk, then the zero-fill half.
        let from_file = (data.len() as u32).saturating_sub(done).min(chunk);
        if from_file > 0 {
            let start = done as usize;
            bus.load_image(at, &data[start..start + from_file as usize])
                .map_err(|e| RomError::Elf(format!("placing at {at:#010x}: {e}")))?;
        }
        if chunk > from_file {
            let zeros = vec![0u8; (chunk - from_file) as usize];
            bus.load_image(at + from_file, &zeros)
                .map_err(|e| RomError::Elf(format!("zero-filling at {at:#010x}: {e}")))?;
        }

        if names.last() != Some(&name) {
            names.push(name);
        }
        at += chunk;
        done += chunk;
    }
    Ok(names)
}
