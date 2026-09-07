//! `ElfImage` — the PT_LOAD view of a firmware image, plus its symbols.
//!
//! The repo already has an rv32 ELF loader (`lp-riscv-elf`), but it presents
//! an emulator-guest memory image, not the program headers: no `paddr`, no
//! per-segment flags, no "this segment is `.rtc_fast` and belongs at a
//! different physical address than it links to". A machine that places
//! segments into named regions and later has to answer "what was at
//! `0x4000_0058`?" needs both, so this reads the ELF directly through
//! `object`.
//!
//! Two consumers, both from P4 on: the ROM loader (place `esp32c6_rev0_rom.elf`
//! and resolve the intercept table's symbols) and the direct loader (place
//! the app's segments and jump to `entry`).

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use object::Endianness;
use object::read::elf::{ElfFile32, FileHeader, ProgramHeader};
use object::{Object, ObjectSection, ObjectSymbol, SectionFlags, SectionKind};

/// What went wrong reading an ELF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElfError {
    /// `object` could not parse it.
    Parse(String),
    /// Parsed, but not a 32-bit little-endian RISC-V image.
    NotRv32(String),
}

impl core::fmt::Display for ElfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ElfError::Parse(m) => write!(f, "ELF parse failed: {m}"),
            ElfError::NotRv32(m) => write!(f, "not an rv32 ELF: {m}"),
        }
    }
}

impl std::error::Error for ElfError {}

/// One `PT_LOAD` program header, with its file bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadSegment {
    /// Where the linker says it lives.
    pub vaddr: u32,
    /// Where it is loaded from. On ESP images these differ for the RTC-fast
    /// and flash-mapped sections, and using `vaddr` for both is how a loader
    /// quietly puts `.rtc_fast` in the wrong place.
    pub paddr: u32,
    /// Bytes present in the file. `memsz - filesz` is `.bss`, zero-filled.
    pub memsz: u32,
    pub read: bool,
    pub write: bool,
    pub execute: bool,
    pub data: Vec<u8>,
}

impl LoadSegment {
    pub fn filesz(&self) -> u32 {
        self.data.len() as u32
    }

    /// The zero-fill tail: `memsz - filesz`.
    pub fn zero_fill(&self) -> u32 {
        self.memsz.saturating_sub(self.filesz())
    }
}

/// A named symbol with its address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub address: u32,
    pub size: u32,
}

/// A section the ELF carries bytes for but asks no loader to place:
/// `PROGBITS`, writable, **not** `SHF_ALLOC`.
///
/// The ESP32-C6 mask ROM's ELF describes its `.data_*` and
/// `.data.interface.*` this way — `.data_pp_rom` at `0x4087_F5A8`, say, with
/// `0x298` bytes of initial values in the file and no `PT_LOAD` covering
/// them (the only `PT_LOAD` there is the `.bss`, `filesz = 0`). On the chip
/// the ROM's startup copies those values into HP SRAM from an image inside
/// the mask ROM; a direct load that skips the startup has to seed them from
/// here, or every ROM global that was not zero at boot — `pp_rom_version`,
/// the interface tables — reads as zero. An application ELF has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitSection {
    pub name: String,
    pub address: u32,
    pub data: Vec<u8>,
}

/// A parsed ELF image: entry, `PT_LOAD` segments, symbols.
#[derive(Clone, Debug, Default)]
pub struct ElfImage {
    pub entry: u32,
    pub segments: Vec<LoadSegment>,
    /// See [`InitSection`]. In section order.
    pub init_sections: Vec<InitSection>,
    /// Sorted by address, so `symbol_at` is a binary search.
    symbols: Vec<Symbol>,
}

impl ElfImage {
    /// Parse an rv32 little-endian ELF.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        let file =
            ElfFile32::<Endianness>::parse(bytes).map_err(|e| ElfError::Parse(e.to_string()))?;
        let endian = file.endian();
        let header = file.elf_header();
        if header.e_machine(endian) != object::elf::EM_RISCV {
            return Err(ElfError::NotRv32(alloc::format!(
                "e_machine = {}",
                header.e_machine(endian)
            )));
        }

        let mut segments = Vec::new();
        for ph in file.elf_program_headers() {
            if ph.p_type(endian) != object::elf::PT_LOAD {
                continue;
            }
            let data = ph
                .data(endian, bytes)
                .map_err(|()| ElfError::Parse("PT_LOAD data out of bounds".to_string()))?;
            let flags = ph.p_flags(endian);
            segments.push(LoadSegment {
                vaddr: ph.p_vaddr(endian),
                paddr: ph.p_paddr(endian),
                memsz: ph.p_memsz(endian),
                read: flags & object::elf::PF_R != 0,
                write: flags & object::elf::PF_W != 0,
                execute: flags & object::elf::PF_X != 0,
                data: data.to_vec(),
            });
        }

        let mut init_sections = Vec::new();
        for section in file.sections() {
            if section.kind() != SectionKind::Data && section.kind() != SectionKind::Other {
                continue;
            }
            let SectionFlags::Elf { sh_flags } = section.flags() else {
                continue;
            };
            let writable = sh_flags & u64::from(object::elf::SHF_WRITE) != 0;
            let alloc = sh_flags & u64::from(object::elf::SHF_ALLOC) != 0;
            if !writable || alloc || section.size() == 0 {
                continue;
            }
            let Ok(data) = section.data() else {
                continue;
            };
            if data.is_empty() {
                continue;
            }
            init_sections.push(InitSection {
                name: section.name().unwrap_or("?").to_string(),
                address: section.address() as u32,
                data: data.to_vec(),
            });
        }

        let mut symbols: Vec<Symbol> = file
            .symbols()
            .filter_map(|s| {
                let name = s.name().ok()?;
                if name.is_empty() {
                    return None;
                }
                // Not symbols in the sense a backtrace or a probe means:
                // `STT_FILE` (a source path at address 0), `STT_SECTION`,
                // and RISC-V's `$x`/`$d` mapping symbols, which mark every
                // instruction/data boundary and sort before every real
                // name — a lookup "first symbol at this address" would
                // answer `$x`, and `symbol("$x")` is the first one in the
                // image. `llvm-nm` hides them for the same reason.
                // `.L…` are assembler-local labels (`.LVL410`,
                // `.Lswitch.table.…`) that survive into the table on this
                // toolchain; a backtrace naming one names nothing.
                if matches!(
                    s.kind(),
                    object::SymbolKind::File | object::SymbolKind::Section
                ) || name.starts_with('$')
                    || name.starts_with(".L")
                {
                    return None;
                }
                Some(Symbol {
                    name: name.to_string(),
                    address: s.address() as u32,
                    size: s.size() as u32,
                })
            })
            .collect();
        symbols.sort_by(|a, b| a.address.cmp(&b.address).then_with(|| a.name.cmp(&b.name)));

        Ok(Self {
            entry: header.e_entry(endian),
            segments,
            init_sections,
            symbols,
        })
    }

    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }

    /// Look a symbol up by exact name. The ROM intercept table's question.
    pub fn symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name == name)
    }

    /// The symbol whose `[address, address + size)` contains `address`, or
    /// — for a zero-sized symbol — one starting exactly there.
    ///
    /// What `--probe` and a backtrace want: "what is at this PC?"
    pub fn symbol_at(&self, address: u32) -> Option<&Symbol> {
        // The last symbol starting at or before `address`.
        let i = self.symbols.partition_point(|s| s.address <= address);
        let i = i.checked_sub(1)?;
        // Walk back over same-address symbols to find one that covers it.
        let start = self.symbols[i].address;
        self.symbols[..=i]
            .iter()
            .rev()
            .take_while(|s| s.address == start)
            .find(|s| {
                if s.size == 0 {
                    s.address == address
                } else {
                    address < s.address.wrapping_add(s.size)
                }
            })
    }

    /// Total bytes the segments occupy in memory, `.bss` included.
    pub fn memory_footprint(&self) -> u64 {
        self.segments.iter().map(|s| u64::from(s.memsz)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built rv32 ELF: one PT_LOAD segment, no sections.
    ///
    /// Built by hand rather than with `object`'s writer so the crate needs
    /// only `object`'s read half — a smaller surface inside the MIT fence.
    fn tiny_elf(entry: u32, vaddr: u32, paddr: u32, memsz: u32, body: &[u8]) -> Vec<u8> {
        const EHSIZE: u32 = 52;
        const PHENTSIZE: u32 = 32;
        let phoff = EHSIZE;
        let body_off = EHSIZE + PHENTSIZE;

        let mut out = Vec::new();
        out.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
        out.push(1); // ELFCLASS32
        out.push(1); // ELFDATA2LSB
        out.push(1); // EV_CURRENT
        out.extend_from_slice(&[0; 9]); // padding to 16
        out.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        out.extend_from_slice(&243u16.to_le_bytes()); // EM_RISCV
        out.extend_from_slice(&1u32.to_le_bytes()); // version
        out.extend_from_slice(&entry.to_le_bytes());
        out.extend_from_slice(&phoff.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // shoff
        out.extend_from_slice(&0u32.to_le_bytes()); // flags
        out.extend_from_slice(&(EHSIZE as u16).to_le_bytes());
        out.extend_from_slice(&(PHENTSIZE as u16).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // phnum
        out.extend_from_slice(&0u16.to_le_bytes()); // shentsize
        out.extend_from_slice(&0u16.to_le_bytes()); // shnum
        out.extend_from_slice(&0u16.to_le_bytes()); // shstrndx
        assert_eq!(out.len() as u32, EHSIZE);

        out.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
        out.extend_from_slice(&body_off.to_le_bytes());
        out.extend_from_slice(&vaddr.to_le_bytes());
        out.extend_from_slice(&paddr.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes()); // filesz
        out.extend_from_slice(&memsz.to_le_bytes());
        out.extend_from_slice(&0x5u32.to_le_bytes()); // PF_R | PF_X
        out.extend_from_slice(&4u32.to_le_bytes()); // align
        assert_eq!(out.len() as u32, body_off);

        out.extend_from_slice(body);
        out
    }

    #[test]
    fn parses_entry_and_a_pt_load_segment() {
        let bytes = tiny_elf(0x4080_0010, 0x4080_0000, 0x4200_0000, 0x40, &[1, 2, 3, 4]);
        let img = ElfImage::parse(&bytes).unwrap();
        assert_eq!(img.entry, 0x4080_0010);
        assert_eq!(img.segments.len(), 1);
        let seg = &img.segments[0];
        assert_eq!(seg.vaddr, 0x4080_0000);
        // paddr is kept separately: on ESP images they differ, and using
        // vaddr for both is how `.rtc_fast` ends up in the wrong place.
        assert_eq!(seg.paddr, 0x4200_0000);
        assert_eq!(seg.data, [1, 2, 3, 4]);
        assert_eq!(seg.filesz(), 4);
        assert_eq!(seg.memsz, 0x40);
        assert_eq!(seg.zero_fill(), 0x3c);
        assert!(seg.read && seg.execute && !seg.write);
        assert_eq!(img.memory_footprint(), 0x40);
    }

    #[test]
    fn rejects_a_non_riscv_elf() {
        let mut bytes = tiny_elf(0, 0, 0, 4, &[0; 4]);
        bytes[18] = 0x3e; // EM_X86_64
        assert!(matches!(
            ElfImage::parse(&bytes).unwrap_err(),
            ElfError::NotRv32(_)
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            ElfImage::parse(b"not an elf at all").unwrap_err(),
            ElfError::Parse(_)
        ));
    }

    #[test]
    fn symbol_lookup_by_address_covers_a_sized_symbol() {
        let img = ElfImage {
            entry: 0,
            segments: Vec::new(),
            init_sections: Vec::new(),
            symbols: alloc::vec![
                Symbol {
                    name: "uart_tx_one_char".to_string(),
                    address: 0x4000_0058,
                    size: 0x30,
                },
                Symbol {
                    name: "ets_printf".to_string(),
                    address: 0x4000_0100,
                    size: 0,
                },
            ],
        };
        assert_eq!(
            img.symbol_at(0x4000_0060).map(|s| s.name.as_str()),
            Some("uart_tx_one_char")
        );
        assert_eq!(img.symbol_at(0x4000_0088), None); // past the end
        assert_eq!(
            img.symbol_at(0x4000_0100).map(|s| s.name.as_str()),
            Some("ets_printf")
        );
        // A zero-sized symbol covers only its own address.
        assert_eq!(img.symbol_at(0x4000_0101), None);
        assert_eq!(img.symbol_at(0x0000_0000), None);
        assert_eq!(
            img.symbol("ets_printf").map(|s| s.address),
            Some(0x4000_0100)
        );
        assert_eq!(img.symbol("nope"), None);
        assert_eq!(img.symbols().len(), 2);
    }
}
