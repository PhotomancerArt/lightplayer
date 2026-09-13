//! The mask ROM: where its bytes go, and the hook table that is deliberately
//! empty.
//!
//! # The ROM is loaded in every configuration, and on this chip that is not a
//! formality
//!
//! Vision D6, plan PD7. On the C6 and the classic the rule is about a handful
//! of calls the application makes whatever booted it. On the S3 the ROM is
//! **most of the dynamic instruction count**: `m6/notes.md` §2.5 counts
//! `memcpy` alone at **4,769 call sites across 800 caller symbols**, plus
//! `__divsf3` (191), `memmove` (162), `memset` (150) and about twenty more
//! soft-float helpers. The four cache/MMU entry points, the five flash ones,
//! `ets_delay_us`, `rom_i2c_{read,write}Reg` and the MD5 family are on top of
//! that.
//!
//! So the bring-up order this phase follows is: hart → memory map → **ROM** →
//! direct load → first MMIO stop. A machine that cannot execute ROM `memcpy`
//! never reaches a peripheral, and `tests/boot.rs` asserts the ROM really
//! executes rather than merely being present.
//!
//! ⚠️ **The S3 ROM has no UART entry point on the application path and no SHA
//! one** (§2.5): the census found zero references to `uart_tx_one_char`,
//! `uartAttach` or `ets_sha_*`. The S3's console is USB-Serial-JTAG. That is
//! the shape of P05, not of this file.
//!
//! ⚠️ **A nearest-symbol alias trap, and it bit the inventory.** Multiple
//! `PROVIDE`s share addresses in the S3 ROM's linker scripts, so
//! `0x4000_1C68` first resolved to `r_llc_rem_phy_upd_proc_continue_hook`
//! when the real target is `esp_rom_md5_update` (§2.5). Anything that reports
//! a ROM address by name reports the *group*, never a single "nearest" one —
//! which is what [`crate::machine::Machine::symbolize`]'s `~` prefix says.
//!
//! # Hooks, and why the table starts empty
//!
//! A hook replaces the first instruction at an address with `break 1, 15` and
//! keeps the original three bytes. The hart returns `SliceEnd::Ebreak` with
//! the `pc` unadvanced and uncharged; the machine gets first refusal, asks
//! this table, and if a hook claims the `pc` it runs the host function.
//! Every other instruction in the ROM costs nothing — there is no
//! per-instruction check.
//!
//! The table ships **empty and stays empty**, and the rule for adding to it
//! is the classic's and the C6's: try the real ROM path first, and add a hook
//! only when the real path cannot be made to work by giving a register a
//! peripheral model owns its documented reset value. In P03 the table has
//! exactly one user — `--break-at`, which installs a stop at an address the
//! caller named.
//!
//! # What is NOT here, and which phase owns it
//!
//! - **The ROM's own unpack/bss tables.** The classic seeds the source
//!   addresses its `_ResetVector`'s `unpcopy` reads, because a ROM-up boot
//!   really runs that copy. The S3 has no ROM-up path until **P06**, and
//!   seeding a table nothing reads would be a claim about a path this phase
//!   has not walked. [`seed_data`] places the non-alloc `.data_*` sections a
//!   *running* image reads, which is the half a direct load needs.
//! - **The flash chip description.** The classic tells the ROM how big the
//!   chip is before `esp_rom_spiflash_*` runs. That is **P06**'s, with the
//!   cache MMU; this phase's direct load reaches its first MMIO stop long
//!   before any flash routine.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use lp_emu_esp_common::{ElfImage, SocBus};

/// Force an `include_bytes!` blob to 8-byte alignment.
///
/// `include_bytes!` promises nothing about alignment, and `object`'s ELF
/// reader casts into the buffer, so an unaligned blob fails to parse with
/// "Invalid ELF header size or alignment" — an error that reads like a
/// corrupt file and is not one. The zero-length leading array is the standard
/// way to raise a struct's alignment without adding a byte to it. Both other
/// machines carry the same shim.
#[repr(C)]
struct Aligned<A, B: ?Sized> {
    _align: [A; 0],
    bytes: B,
}

static VENDORED_S3_ROM_ALIGNED: &Aligned<u64, [u8; 949_552]> = &Aligned {
    _align: [],
    bytes: *include_bytes!("../../roms/esp32s3_rev0_rom.elf"),
};

/// The vendored ESP32-S3 revision-0 mask ROM, compiled in.
///
/// Embedded rather than read from a path so that a test, an installed binary
/// and a CI runner all load the same 949,552 bytes without agreeing on a
/// working directory. `--rom <path>` overrides it; the sha256 of these bytes
/// is checked against `lp-emu/esp/roms/SHA256SUMS` by `tests/rom_vendoring.rs`,
/// **in process**, from the bytes the machine will actually load.
///
/// The length is spelled out in the type so a ROM swapped for a different
/// chip's fails to **compile** here, before any checksum test runs.
pub static VENDORED_S3_ROM: &[u8] = &VENDORED_S3_ROM_ALIGNED.bytes;

/// `break 1, 15`, the three bytes a hook writes.
///
/// ⚠️ Not transcribed: `tests/boot.rs` re-derives it with
/// `lp_xt_inst::encode(&Inst::Break(1, 15))` and asserts these bytes, so the
/// constant is checked against the assembler rather than against a comment.
/// M0's `rev8` lesson in three bytes.
pub const BREAK_1_15_BYTES: [u8; 3] = [0xF0, 0x41, 0x00];

/// Where a ROM segment's bytes went. The build report lists these.
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
    /// `p_vaddr != p_paddr`: placed by **vaddr**, and this says so rather
    /// than leaving a reader to wonder which was picked. The S3 ROM's `.data`
    /// segments are all like this — vaddr `0x3FCD_7E00`, paddr
    /// `0x4005_77A8`.
    pub const fn relocated(&self) -> bool {
        self.vaddr != self.paddr
    }
}

/// Something the ROM image and this memory map disagree about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RomError {
    Elf(String),
    /// A `PT_LOAD` or a seeded section covers an address no region claims.
    /// The stop-and-report case: either the ROM is not the chip we think it
    /// is, or [`crate::memmap`] is wrong.
    Unmapped {
        vaddr: u32,
        memsz: u32,
        at: u32,
    },
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
                 which no region of the ESP32-S3 map claims"
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
    /// address, every register as the caller left it. `--break-at`'s answer.
    Stop,
}

/// A host stand-in for a ROM routine.
///
/// It sees the whole machine, because a hook that could not read guest memory
/// (a string to print, a buffer to fill) could only stand in for routines
/// that take no pointers — which is none of the interesting ones.
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

    /// Patch `symbol`'s first instruction to [`BREAK_1_15_BYTES`] and claim
    /// its address.
    ///
    /// Fails when the ROM has no such symbol — a hook aimed at a name that
    /// moved between ROM revisions must not silently do nothing.
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

    /// [`install`](Self::install) at an address already resolved.
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

/// The three bytes at `address`, read through the bus's own decode so a patch
/// cannot land somewhere the guest would not fetch from.
///
/// Read as aligned **words**, not as three bytes. The classic has to do this
/// because SRAM0 refuses byte access there (DD37); this machine declares no
/// such rule, and reads words anyway for the second reason that argument
/// gives: a word read is what the instruction fetch that will run these bytes
/// does, so the two cannot disagree about what is readable.
pub fn read_three(bus: &mut SocBus, address: u32) -> Result<[u8; 3], RomError> {
    use lp_emu_core::Bus;
    let mut out = [0u8; 3];
    // At most two words: three bytes straddle a boundary when the address is
    // one byte short of one.
    let mut word_at = address & !3;
    let mut word = bus
        .read_word(word_at)
        .map_err(|e| RomError::Elf(format!("reading {word_at:#010x}: {e}")))?
        .to_le_bytes();
    for (i, byte) in out.iter_mut().enumerate() {
        let at = address + i as u32;
        if at & !3 != word_at {
            word_at = at & !3;
            word = bus
                .read_word(word_at)
                .map_err(|e| RomError::Elf(format!("reading {word_at:#010x}: {e}")))?
                .to_le_bytes();
        }
        *byte = word[(at & 3) as usize];
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
    parse(VENDORED_S3_ROM)
}

/// Place every `PT_LOAD` of the ROM into the bus.
///
/// The ELF says where; nothing here assumes. Empty segments are skipped and
/// counted ([`empty_segments`]) — this ROM declares 44 program headers of
/// which most are zero-length — a segment that straddles a region boundary is
/// chunked rather than refused, and a segment whose bytes fall outside every
/// region is [`RomError::Unmapped`], the stop-and-report case.
pub fn load(bus: &mut SocBus, rom: &ElfImage) -> Result<Vec<PlacedSegment>, RomError> {
    let mut placed = Vec::new();
    for seg in &rom.segments {
        if seg.memsz == 0 {
            continue;
        }
        let header_mapped = is_header_map(&seg.data);
        let (at, data, memsz) = if header_mapped {
            // The file half is the ELF's own headers; only the zero-fill tail
            // is guest memory. See `is_header_map`.
            let skip = seg.filesz();
            (seg.vaddr + skip, &[][..], seg.memsz.saturating_sub(skip))
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
/// The classic ROM has one such segment and so does this one: the vendored
/// `esp32s3_rev0_rom.elf`'s first `PT_LOAD` is
///
/// ```text
/// LOAD  Offset 0x000000  VirtAddr 0x3fcd7000  FileSiz 0x005b4  MemSiz 0x18770  RW
///       .bss_shared_bufs .stack_pro .stack_app .bss_ets …
/// ```
///
/// — `p_offset = 0`, so its file bytes are the 52-byte ELF header plus the
/// 44 × 32-byte program headers, exactly `0x5B4`, and every section it covers
/// is `NOBITS`. Placing it by vaddr the ordinary way would write the ELF's
/// own magic into the guest's `.bss`.
///
/// The signature is the ELF magic at the start of the file bytes, which is
/// the definition of `p_offset = 0` and is what `lp-emu-esp-common`'s
/// [`ElfImage`] view exposes (it keeps the bytes, not the offset).
/// `tests/boot.rs` cross-checks that the length is exactly this ELF's header
/// table's.
pub fn is_header_map(data: &[u8]) -> bool {
    data.starts_with(b"\x7fELF")
}

/// How many `PT_LOAD`s an image declares with nothing in them. Reported so
/// "44 program headers, N placed" is not a discrepancy anyone has to chase.
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

/// What [`seed_data`] placed, for the build report and the tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DataImage {
    pub sections: usize,
    pub bytes: u32,
}

/// Seed the ROM's initialised data — the `PROGBITS` sections flagged `W` but
/// not `A`, which no `PT_LOAD` covers.
///
/// The list is read from the ELF ([`lp_emu_esp_common::elf::InitSection`]) and
/// never hardcoded: a ROM revision that moves a section moves the seed with
/// it, and one that adds a section gets it seeded without a code change.
///
/// Call it after [`load`] and before an application image is placed, so a
/// direct load overwrites what the app owns exactly as a real bootloader
/// would.
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

/// Place `data` (then zero-fill to `memsz`) from `address`, across as many
/// regions as it spans.
///
/// Host placement, not a guest store: the bytes go in through
/// [`SocBus::load_image`], which is the emulator putting memory in the state
/// silicon was handed. A read-only region still accepts them, which is how
/// the mask ROM gets its own contents.
///
/// ⚠️ **The region walk resolves a RAM alias first, exactly as the bus does.**
/// The shipped image's `.vectors` and `.rwtext` link at `0x4037_8000` and
/// `0x4037_8400` — the SRAM1 **I-bus** view, which is an alias and not a
/// region — so a walk that asked `regions()` about the raw address would
/// refuse an image that is perfectly well placed.
/// [`SocBus::load_image`] already applies `canonical()` before it writes
/// (P02, ruling DD81); this loop has to apply the same translation to the
/// *lookup* it does for chunking and naming, and the S3 is the first machine
/// where the two differ. That is why the placed regions of an executable
/// segment at `0x4037_8000` read `sram1-dbus`: the region a byte lives in is
/// the canonical one, whichever door it arrived through.
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
        // The canonical address and how far the alias window itself runs: a
        // span that ran off the end of an alias would have to be looked up
        // again, so the alias's own remaining room bounds this chunk too.
        let (canonical, alias_room) = match crate::bus_setup::ram_alias_of(at) {
            Some((span, canonical)) => (canonical, span.end() - at),
            None => (at, u32::MAX),
        };
        let (name, room) = bus
            .regions()
            .iter()
            .find(|r| r.contains(canonical))
            .map(|r| (r.name, r.end() - canonical))
            .ok_or(RomError::Unmapped {
                vaddr: address,
                memsz: total,
                at,
            })?;
        let chunk = room.min(alias_room).min(total - done);

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
