//! The mask ROM: where its segments go, and the hook table that is
//! deliberately empty.
//!
//! # The ROM is loaded in every configuration
//!
//! Vision D6, plan PD7. The application calls into the mask ROM at runtime
//! whatever booted it — `rtc_get_reset_reason` from `__pre_init` *before
//! `.bss` is zeroed*, `ets_delay_us` from every clock path, `memcpy` and the
//! `str*` family because the linker resolves them there. So the ROM image is
//! part of the memory map, not an extra for a ROM-up boot. What is optional
//! is running the boot chain from reset, which is M7.
//!
//! # Hooks, and why the table starts empty
//!
//! A hook replaces the first instruction at a symbol with `ebreak` and keeps
//! the original word. The hart returns [`SliceEnd::Ebreak`] with the `pc`
//! unadvanced and uncharged; the machine gets first refusal, asks this table,
//! and if a hook claims the `pc` it runs the host function and performs the
//! `ret` (`pc = ra`) itself. Every other instruction in the ROM costs
//! nothing — there is no per-instruction check.
//!
//! The table ships **empty**, and the rule for adding to it is: try the real
//! ROM path first, and add a hook only when the real path cannot be made to
//! work by seeding a register a peripheral model owns. The candidates, in the
//! order the boot meets them:
//!
//! 1. `rtc_get_reset_reason` (the `0x4000_0018` slot) — reads
//!    `LP_CLKRST.reset_cause` at `0x600B_0410` and masks it to five bits.
//!    **Answered: no hook.** P5 gives that register a reset value of 1 and
//!    the real ROM returns `POWERON` on its own, which is what makes
//!    `[RECOVERY] boot: cause=power-on` hold. See
//!    `tests/rom_reset_reason.rs`.
//! 2. `ets_delay_us` (`0x4000_0040`) — spins on a counter the hart provides.
//!    Runs for real.
//! 3. `uart_tx_one_char` / `uart_tx_flush` / `ets_get_printf_channel` — P6.
//! 4. `esp_rom_spiflash_*` / `spi_flash_*` — M4.
//!
//! [`SliceEnd::Ebreak`]: lp_riscv_emu::mach::SliceEnd::Ebreak

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
#[repr(C)]
struct Aligned<A, B: ?Sized> {
    _align: [A; 0],
    bytes: B,
}

static VENDORED_C6_ROM_ALIGNED: &Aligned<u64, [u8; 489_768]> = &Aligned {
    _align: [],
    bytes: *include_bytes!("../../roms/esp32c6_rev0_rom.elf"),
};

/// The vendored ESP32-C6 revision-0 mask ROM, compiled in.
///
/// Embedded rather than read from a path so that a test, a `cargo install`ed
/// binary and a CI runner all load the same 489,768 bytes without agreeing on
/// a working directory. `--rom <path>` overrides it; the sha256 of these
/// bytes is checked against `lp-emu/esp/roms/SHA256SUMS` by a unit test.
///
/// The length is spelled out in the type so a ROM swapped for a different
/// chip's fails to **compile** here, before any checksum test runs.
pub static VENDORED_C6_ROM: &[u8] = &VENDORED_C6_ROM_ALIGNED.bytes;

/// `ebreak` — the 4-byte form (`funct12 = 1`, opcode `SYSTEM`).
///
/// esp-emulator patches `c.ebreak` (2 bytes) for the same purpose; the 4-byte
/// form is used here because every ROM entry point this could ever hook
/// begins with a 4-byte instruction, and a 2-byte patch would leave the
/// second half of one behind for the `--hooks` listing to misreport.
pub const EBREAK: u32 = 0x0010_0073;

/// Where the ROM's bytes went. The phase report lists these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedSegment {
    pub vaddr: u32,
    pub paddr: u32,
    pub filesz: u32,
    pub memsz: u32,
    pub execute: bool,
    /// The region names the bytes landed in, in address order. More than one
    /// when a segment spans a region boundary — which the real C6 ROM does.
    pub regions: Vec<&'static str>,
}

/// Something the ROM image and this memory map disagree about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RomError {
    Elf(String),
    /// A `PT_LOAD` covers an address no region claims. The brief's
    /// stop-and-report case: either the ROM is not the chip we think it is,
    /// or [`crate::memmap`] is wrong.
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
                 which no region of the C6 map claims"
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
    /// (`pc = ra`), with `a0`/`a1` already set by the hook.
    Ret,
    /// The hook declined after all; give the guest the architectural
    /// breakpoint. Same as having no hook at the address.
    Breakpoint,
    /// Stop the run here, with the guest untouched: `pc` still at the
    /// symbol, every register as the caller left it. `--break-at`'s answer,
    /// and the one a bring-up wants when the question is "who called this
    /// with what".
    Stop,
}

/// A host stand-in for a ROM routine.
///
/// It sees the whole machine, because a hook that could not read guest memory
/// (a string to print, a buffer to fill) would only be able to stand in for
/// routines that take no pointers — which is none of the interesting ones.
pub type HostHook = fn(&mut crate::machine::Esp32C6Machine) -> HookResult;

/// One installed hook.
#[derive(Clone, Copy)]
pub struct Hook {
    pub symbol: &'static str,
    pub address: u32,
    /// The instruction word the `ebreak` replaced, so the table can be
    /// uninstalled and so `--hooks` can show what was displaced.
    pub original: u32,
    pub call: HostHook,
}

impl fmt::Debug for Hook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hook")
            .field("symbol", &self.symbol)
            .field("address", &format_args!("{:#010x}", self.address))
            .field("original", &format_args!("{:#010x}", self.original))
            .finish()
    }
}

/// `pc` → host hook. Empty by default; see the module docs for the rule.
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

    /// Patch `symbol`'s first instruction to `ebreak` and claim its address.
    ///
    /// The word that was there is kept in the returned table entry. Fails
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
    /// symbol found by demangled path (`--break-at`), whose ELF name is not
    /// what the listing should show.
    pub fn install_at(
        &mut self,
        bus: &mut SocBus,
        address: u32,
        symbol: &'static str,
        call: HostHook,
    ) -> Result<u32, RomError> {
        let original = read_word(bus, address)?;
        bus.load_image(address, &EBREAK.to_le_bytes())
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
            if let Err(e) = bus.load_image(hook.address, &hook.original.to_le_bytes()) {
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

fn read_word(bus: &mut SocBus, address: u32) -> Result<u32, RomError> {
    // `Bus::read_word` and not a region peek: the address has to be one the
    // decode agrees is readable, or the patch would go somewhere else.
    use lp_emu_core::Bus;
    bus.read_word(address)
        .map(|v| v as u32)
        .map_err(|e| RomError::Elf(format!("reading {address:#010x}: {e}")))
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

/// Place every `PT_LOAD` of the ROM into the bus.
///
/// The ELF says where; nothing here assumes. Three things the real
/// `esp32c6_rev0_rom.elf` does that a loader written from the memory map
/// alone would get wrong, and which is why this walks region boundaries:
///
/// 1. **Sixteen of its 21 `PT_LOAD`s are empty** (`filesz = memsz = 0`) at
///    `vaddr = 0`. Placing them would write nothing at address zero and
///    report a bogus mapping; they are skipped and counted.
/// 2. **One segment straddles two regions.** `0x4004_8400 + 0x5fa8` starts
///    inside `ROM_MASK` and ends inside `DROM_MASK`, because the
///    `0x4004_AC00` boundary is esp-hal's line, not the ROM linker's. So the
///    placement is chunked per region rather than requiring one.
/// 3. **Its `.bss` reaches into HP SRAM** (`0x4086_ad08 + 0x14da4`), across
///    what the app calls RAM, `dram2_seg` and the ROM's own data. That is
///    correct and it is why the ROM is loaded *before* the app: the direct
///    loader then overwrites what the app owns, exactly as the real
///    bootloader does.
pub fn load(bus: &mut SocBus, rom: &ElfImage) -> Result<Vec<PlacedSegment>, RomError> {
    let mut placed = Vec::new();
    for seg in &rom.segments {
        if seg.memsz == 0 {
            continue;
        }
        let regions = place_spanning(bus, seg.vaddr, &seg.data, seg.memsz)?;
        placed.push(PlacedSegment {
            vaddr: seg.vaddr,
            paddr: seg.paddr,
            filesz: seg.filesz(),
            memsz: seg.memsz,
            execute: seg.execute,
            regions,
        });
    }
    Ok(placed)
}

/// How many `PT_LOAD`s an image declares with nothing in them. Reported so
/// "21 program headers, 5 placed" is not a discrepancy anyone has to chase.
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

/// Seed the ROM's initialised data — what its startup would have copied into
/// HP SRAM before the app ran, taken from the ELF's own bytes.
///
/// The ROM ELF's `.data_*` and `.data.interface.*` sections are writable
/// `PROGBITS` **without** `SHF_ALLOC` ([`lp_emu_esp_common::elf::InitSection`]):
/// no `PT_LOAD` places them, the one covering their addresses is the `.bss`
/// with `filesz = 0`, and on the chip `_start` → `main` copies them in from
/// an image inside the mask ROM. Direct load skips that startup (M7 runs it),
/// so this writes the same bytes at the same addresses. Found the hard way
/// in M3 P6: `esp_pp_rom_version_get` is `lw a0, pp_rom_version` at
/// `0x4087_F83C` (`.data_pp_rom + 0x294`), and a zero there is a NULL the
/// WiFi blob hands `pp_printf("pp rom version: %s")` — a load fault at
/// address 0 inside `_vsnprintf`, 15 ms into the boot.
///
/// The same ELF quirk hides part of the mask ROM itself: `.rodata.interface`
/// (`0x4004_FD40`, `0x2C0` bytes — the tables the app's WiFi blob reads its
/// ROM pointers from, `lmacInitAc`'s `lw a5, -0x20(0x40050000)` among them)
/// is non-allocated too, and no `PT_LOAD` reaches past `0x4004_E3A8`. On the
/// chip those bytes are simply in the ROM. So the rule has no address
/// filter: every non-allocated `PROGBITS` section with bytes is something
/// the mask ROM holds or its startup copies, and it goes where the ELF says.
///
/// Nothing here touches memory the app owns: the SRAM seeds start at
/// `.data_ets` (`0x4087_E610`, the ROM data base, above `dram2_seg`) and the
/// rest lands in the mask ROM's DROM, so the app's `[mem]` figures are
/// unaffected. Call it after [`load`] and before the app is placed.
pub fn seed_data(bus: &mut SocBus, rom: &ElfImage) -> Result<Vec<SeededSection>, RomError> {
    let mut seeded = Vec::new();
    for section in &rom.init_sections {
        place_spanning(
            bus,
            section.address,
            &section.data,
            section.data.len() as u32,
        )?;
        seeded.push(SeededSection {
            name: section.name.clone(),
            address: section.address,
            len: section.data.len() as u32,
        });
    }
    Ok(seeded)
}

/// What [`seed_data_image`] reconstructed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DataImage {
    /// The copy table's bounds, from the ROM's own symbols.
    pub table: (u32, u32),
    /// The lowest and highest source byte written back into the mask ROM.
    pub span: (u32, u32),
    pub entries: usize,
    pub bytes: u32,
}

/// Put the mask ROM's **own data image** back where the ROM's startup will
/// read it — the half of [`seed_data`] a ROM-up boot needs and a direct load
/// never did.
///
/// # The quirk, stated twice
///
/// `_init` ends with a copy loop over a table of `{dst, dst_end, src}`
/// triples between `_data_table_start` and `_bss_start`:
///
/// ```text
/// 40001794 <unpackloop>:
/// 40001794:  lw a3,0(a1)     ; dst
/// 40001796:  lw a4,4(a1)     ; dst_end
/// 40001798:  lw a5,8(a1)     ; src
/// 4000179c:  lw t0,0(a5)     ; the copy
/// ```
///
/// The 37 triples' `src` addresses run from `0x4004_2196` to `0x4004_25a0`,
/// and **no `PT_LOAD` and no section of the ROM ELF reaches them**: the last
/// loadable byte is `0x4004_2196` exclusive, and `.iram1.4` ends exactly
/// there. On silicon those bytes are simply in the mask ROM. In the ELF they
/// exist only as the *result* — the non-allocated `.data_*` and
/// `.data.interface.*` sections [`seed_data`] places at the `dst` addresses.
///
/// So the image is derivable, and this derives it: for each triple, the
/// bytes now at `dst` are written back at `src`. The ROM then runs its own
/// unpack loop, for real, and copies exactly what the ELF says it should.
///
/// # Why it is not optional, and what it cost to find
///
/// Without it the loop copies **zeros** over everything [`seed_data`] had
/// just placed. M7's second ROM-up run died 14,334 instructions in, at
/// `usb_serial_device_tx_one_char+0x2a` — `lw a5, -8(s4)` reads
/// `ets_ops_table_ptr` at `0x4087_fff8`, whose whole four-byte section
/// (`.data.interface.common` = `0x4004_a680`) had been zeroed by the loop, and
/// `lw a5, 0(a5)` then read address zero. The console cannot print its first
/// character without this.
///
/// Called after [`seed_data`], whose output it reads. Harmless on the direct
/// path: nothing there executes `_init`, and every byte written lands in the
/// mask ROM's own address space, above everything the app owns.
pub fn seed_data_image(bus: &mut SocBus, rom: &ElfImage) -> Result<DataImage, RomError> {
    let start = rom
        .symbol("_data_table_start")
        .ok_or_else(|| RomError::NoSuchSymbol("_data_table_start".into()))?
        .address;
    let end = rom
        .symbol("_bss_start")
        .ok_or_else(|| RomError::NoSuchSymbol("_bss_start".into()))?
        .address;

    let mut image = DataImage {
        table: (start, end),
        span: (u32::MAX, 0),
        entries: 0,
        bytes: 0,
    };
    let mut at = start;
    while at + 12 <= end {
        let dst = read_word(bus, at)?;
        let dst_end = read_word(bus, at + 4)?;
        let src = read_word(bus, at + 8)?;
        at += 12;
        image.entries += 1;
        let Some(len) = dst_end.checked_sub(dst) else {
            log::warn!("rom: data table entry at {at:#010x} has dst_end < dst; skipped");
            continue;
        };
        if len == 0 {
            continue;
        }
        let mut bytes = vec![0u8; len as usize];
        read_region_bytes(bus, dst, &mut bytes).ok_or(RomError::Unmapped {
            vaddr: dst,
            memsz: len,
            at: dst,
        })?;
        place_spanning(bus, src, &bytes, len)?;
        image.span.0 = image.span.0.min(src);
        image.span.1 = image.span.1.max(src + len);
        image.bytes += len;
    }
    if image.bytes == 0 {
        image.span = (0, 0);
    }
    Ok(image)
}

/// Read `out.len()` bytes out of whichever RAM region holds `address`.
fn read_region_bytes(bus: &SocBus, address: u32, out: &mut [u8]) -> Option<()> {
    let end = address.checked_add(out.len() as u32)?.checked_sub(1)?;
    for region in bus.regions() {
        if region.contains(address) && region.contains(end) {
            let at = (address - region.base) as usize;
            out.copy_from_slice(&region.data[at..at + out.len()]);
            return Some(());
        }
    }
    None
}

/// Write `data` at `address`, then zero out to `memsz`, splitting the write
/// at region boundaries and naming every region touched.
///
/// A byte that lands outside every region is [`RomError::Unmapped`] — the
/// brief's stop-and-report case.
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

/// The ROM entry points P5/P6/M4 will care about, at the addresses
/// `esp-rom-sys-0.1.4/ld/esp32c6/rom/esp32c6.rom.ld` gives them.
///
/// **These are trampolines, not implementations.** The linker script's
/// addresses are a jump table at the very bottom of the mask ROM: the ELF
/// names the slot at `0x4000_0018` `__call_rtc_get_reset_reason`, and it
/// holds a `jal` to `rtc_get_reset_reason` at `0x4001_9680`. Both symbols
/// are real and neither is the other, which matters twice over — a hook
/// installed by implementation name catches calls arriving through the
/// trampoline, and a backtrace through `0x4000_0018` must not be reported as
/// the function body.
///
/// Not a hook table: a **cross-check**. A test asserts that the vendored ELF
/// agrees with the linker script on every slot, and that each slot's `jal`
/// lands on the same-named implementation — which is how a ROM swapped for
/// another revision is caught by name rather than by a wrong jump.
pub const KNOWN_ROM_ENTRY_POINTS: &[(&str, u32)] = &[
    ("rtc_get_reset_reason", 0x4000_0018),
    ("ets_printf", 0x4000_0028),
    ("ets_get_printf_channel", 0x4000_003c),
    ("ets_delay_us", 0x4000_0040),
    ("ets_update_cpu_frequency", 0x4000_0048),
    ("uart_tx_one_char", 0x4000_0058),
    ("uart_tx_flush", 0x4000_0074),
    ("software_reset", 0x4000_0090),
    ("memset", 0x4000_04a8),
    ("memcpy", 0x4000_04ac),
    ("esp_rom_spiflash_read", 0x4000_0150),
    ("usb_serial_device_tx_one_char", 0x4000_0a8c),
];

/// The target of a `jal` at `pc`, or `None` if `word` is not one.
///
/// The ROM's trampoline table is nothing but these, and decoding one is the
/// difference between "the ELF has a symbol with that name" and "the slot the
/// application actually calls reaches it".
pub fn jal_target(word: u32, pc: u32) -> Option<u32> {
    if word & 0x7f != 0x6f {
        return None;
    }
    // Spec 2.5: imm[20|10:1|11|19:12], sign-extended, always even.
    let imm = (((word >> 31) & 1) << 20)
        | (((word >> 21) & 0x3ff) << 1)
        | (((word >> 20) & 1) << 11)
        | (((word >> 12) & 0xff) << 12);
    let signed = ((imm as i32) << 11) >> 11;
    Some(pc.wrapping_add(signed as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memmap;
    use lp_emu_core::Bus;

    #[test]
    fn the_vendored_rom_is_the_c6_and_its_segments_fit_the_map() {
        let rom = parse(VENDORED_C6_ROM).expect("the vendored ROM parses");
        assert_eq!(rom.entry, memmap::ROM_MASK_BASE, "reset vector");
        assert!(rom.symbols().len() > 3_000);

        let mut bus = crate::machine::Esp32C6Builder::bare_bus();
        let placed = load(&mut bus, &rom).expect("every PT_LOAD lands in the map");

        // Four segments carry bytes; the other seventeen PT_LOADs are empty
        // declarations at vaddr 0.
        assert_eq!(placed.len(), 4, "placed: {placed:#?}");
        assert_eq!(empty_segments(&rom), 17);

        // The straddling segment is the reason `place_spanning` exists.
        let straddle = placed
            .iter()
            .find(|p| p.regions.len() > 1)
            .expect("one segment spans two regions");
        assert_eq!(straddle.vaddr, 0x4004_8400);
        assert_eq!(straddle.regions, ["rom-mask", "drom-mask"]);
    }

    #[test]
    fn the_roms_initialised_data_is_seeded_from_its_unallocated_sections() {
        let rom = parse(VENDORED_C6_ROM).unwrap();
        let mut bus = crate::machine::Esp32C6Builder::bare_bus();
        load(&mut bus, &rom).unwrap();

        // Before: the `.bss` PT_LOAD zero-filled the whole ROM data area.
        let pp_rom_version = rom.symbol("pp_rom_version").unwrap().address;
        assert_eq!(pp_rom_version, 0x4087_f83c);
        assert_eq!(bus.read_word(pp_rom_version).unwrap(), 0);

        // The interface table the WiFi blob reads its ROM pointers from is
        // in the mask ROM's DROM, past every PT_LOAD.
        assert_eq!(
            bus.read_word(0x4004_ffe0).unwrap(),
            0,
            "unreached by any PT_LOAD"
        );

        let seeded = seed_data(&mut bus, &rom).unwrap();
        let names: Vec<&str> = seeded.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&".data_pp_rom"), "{names:?}");
        assert!(names.contains(&".data.interface.rom_pp"), "{names:?}");
        assert!(names.contains(&".rodata.interface"), "{names:?}");
        let drom = memmap::DROM_MASK_BASE..memmap::DROM_MASK_BASE + memmap::DROM_MASK_LEN;
        assert!(
            seeded
                .iter()
                .all(|s| s.address >= memmap::ROM_DATA_BASE || drom.contains(&s.address)),
            "every seed is above dram2_seg or inside the mask ROM's DROM: {seeded:?}"
        );
        let total: u32 = seeded.iter().map(|s| s.len).sum();
        assert!(total > 0x500, "{total} bytes seeded");
        let table_entry = bus.read_word(0x4004_ffe0).unwrap() as u32;
        assert!(
            (memmap::HP_SRAM_BASE..memmap::HP_SRAM_BASE + memmap::HP_SRAM_LEN)
                .contains(&table_entry),
            "the lmac AC table pointer {table_entry:#010x} names ROM data in HP SRAM"
        );

        // After: the three `*_rom_version` pointers name strings in the
        // mask ROM, which is what `esp_pp_rom_version_get` returns on the
        // chip and what the WiFi blob prints.
        for name in [
            "pp_rom_version",
            "net80211_rom_version",
            "coexist_rom_version",
        ] {
            let at = rom.symbol(name).unwrap().address;
            let ptr = bus.read_word(at).unwrap() as u32;
            assert!(
                (memmap::ROM_MASK_BASE..memmap::DROM_MASK_BASE + memmap::DROM_MASK_LEN)
                    .contains(&ptr),
                "{name} = {ptr:#010x} is not in the mask ROM"
            );
            let first = bus.read_u8(ptr).unwrap();
            assert!(first.is_ascii_graphic(), "{name} -> {first:#04x}");
        }
    }

    #[test]
    fn the_roms_own_data_image_is_reconstructed_from_the_sections_it_unpacks_to() {
        let rom = parse(VENDORED_C6_ROM).unwrap();
        let mut bus = crate::machine::Esp32C6Builder::bare_bus();
        load(&mut bus, &rom).unwrap();
        seed_data(&mut bus, &rom).unwrap();

        // `ets_ops_table_ptr`: the four bytes the ROM console dereferences
        // on its way to `ets_delay_us`, and the ones a ROM-up boot lost.
        let dst = rom.symbol("ets_ops_table_ptr").unwrap().address;
        assert_eq!(dst, 0x4087_fff8);
        let seeded = bus.read_word(dst).unwrap() as u32;
        assert_eq!(seeded, 0x4004_a680, "seed_data placed the destination");

        let image = seed_data_image(&mut bus, &rom).unwrap();
        assert_eq!(image.table, (0x4004_1ea8, 0x4004_2064));
        assert_eq!(image.entries, 37, "12 bytes each between those bounds");
        assert!(image.bytes > 0x400, "{} bytes", image.bytes);

        // Every source byte is inside the mask ROM and past the last
        // `PT_LOAD`, which is the whole reason this function exists.
        let last_loaded = 0x4000_0000 + 0x42196;
        assert!(
            image.span.0 >= last_loaded,
            "the sources start at {:#010x}, before the last PT_LOAD byte {last_loaded:#010x} — \
             then they would have been in the ELF and nothing needed rebuilding",
            image.span.0
        );
        assert!(
            image.span.1 <= memmap::DROM_MASK_BASE,
            "{:#010x}",
            image.span.1
        );

        // The table entry for `ets_ops_table_ptr` now reads the same word at
        // its source as at its destination: the ROM's unpack loop copies the
        // ELF's own value.
        let mut found = None;
        let mut at = image.table.0;
        while at + 12 <= image.table.1 {
            let d = bus.read_word(at).unwrap() as u32;
            let e = bus.read_word(at + 4).unwrap() as u32;
            let s = bus.read_word(at + 8).unwrap() as u32;
            if d <= dst && dst < e {
                found = Some(s + (dst - d));
            }
            at += 12;
        }
        let src = found.expect("a triple covers ets_ops_table_ptr");
        assert_eq!(bus.read_word(src).unwrap() as u32, seeded);
    }

    #[test]
    fn every_linker_script_slot_is_a_trampoline_that_jumps_to_its_implementation() {
        let rom = parse(VENDORED_C6_ROM).unwrap();
        let mut bus = crate::machine::Esp32C6Builder::bare_bus();
        load(&mut bus, &rom).unwrap();

        for (name, slot) in KNOWN_ROM_ENTRY_POINTS {
            let trampoline = format!("__call_{name}");
            let stub = rom
                .symbol(&trampoline)
                .unwrap_or_else(|| panic!("the vendored ROM has no `{trampoline}`"));
            assert_eq!(
                stub.address, *slot,
                "`{trampoline}` is at {:#010x}, esp32c6.rom.ld says {slot:#010x}",
                stub.address
            );

            let body = rom
                .symbol(name)
                .unwrap_or_else(|| panic!("the vendored ROM has no `{name}`"));
            let word = bus.read_word(*slot).unwrap() as u32;
            assert_eq!(
                jal_target(word, *slot),
                Some(body.address),
                "the slot at {slot:#010x} does not jump to `{name}` ({:#010x})",
                body.address
            );
        }
    }

    #[test]
    fn jal_decoding_handles_the_backward_case_too() {
        // The one the ROM actually contains: 0x40000018 -> 0x40019680.
        assert_eq!(jal_target(0x6681_906f, 0x4000_0018), Some(0x4001_9680));
        // `jal x0, -4`.
        assert_eq!(jal_target(0xffdf_f06f, 0x1000), Some(0x0ffc));
        assert_eq!(jal_target(0x0000_0013, 0), None, "addi is not a jal");
    }

    #[test]
    fn a_hook_patches_ebreak_and_keeps_the_word_it_displaced() {
        let rom = parse(VENDORED_C6_ROM).unwrap();
        let mut bus = crate::machine::Esp32C6Builder::bare_bus();
        load(&mut bus, &rom).unwrap();

        let before = bus.read_word(0x4001_9680).unwrap() as u32;
        assert_ne!(before, EBREAK, "the ROM does not start with an ebreak");

        let mut table = HookTable::new();
        // Hooks resolve to the *implementation*, so a call arriving through
        // the `0x4000_0018` trampoline is caught too.
        let at = table
            .install(&mut bus, &rom, "rtc_get_reset_reason", |_| HookResult::Ret)
            .unwrap();
        assert_eq!(at, 0x4001_9680);
        assert_eq!(bus.read_word(at).unwrap() as u32, EBREAK);
        assert_eq!(table.get(at).unwrap().original, before);
        assert!(table.get(at + 4).is_none(), "only the first instruction");
        assert_eq!(table.len(), 1);

        table.uninstall_all(&mut bus);
        assert_eq!(bus.read_word(at).unwrap() as u32, before);
        assert!(table.is_empty());
    }

    #[test]
    fn a_hook_on_a_symbol_the_rom_does_not_have_is_an_error_not_a_silent_no_op() {
        let rom = parse(VENDORED_C6_ROM).unwrap();
        let mut bus = crate::machine::Esp32C6Builder::bare_bus();
        load(&mut bus, &rom).unwrap();
        let mut table = HookTable::new();
        assert!(matches!(
            table.install(&mut bus, &rom, "no_such_rom_routine", |_| HookResult::Ret),
            Err(RomError::NoSuchSymbol(_))
        ));
        assert!(table.is_empty());

        // Not the obvious candidate: `rom_i2c_writeReg_Mask` IS in the C6
        // mask ROM (at 0x40004160, with a trampoline at 0x400012cc). The M3
        // discovery's "not present on the C6" is about `esp32c6.rom.ld`,
        // which does not *export* it — the ROM has code the linker script
        // does not name, and a loader that trusted the script for what
        // exists would be wrong.
        assert!(rom.symbol("rom_i2c_writeReg_Mask").is_some());
    }

    #[test]
    fn a_segment_reaching_past_every_region_is_reported_not_dropped() {
        let mut bus = crate::machine::Esp32C6Builder::bare_bus();
        let err = place_spanning(&mut bus, memmap::LP_SRAM_BASE, &[], 0x1_0000).unwrap_err();
        assert!(
            matches!(err, RomError::Unmapped { at, .. } if at == memmap::LP_SRAM_BASE + memmap::LP_SRAM_LEN)
        );
    }
}
