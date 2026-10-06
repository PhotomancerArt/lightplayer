//! A named static's bytes in an ELF32 file: where they are, so the packager
//! can read the core's build id and patch its engine digest slot.

use anyhow::{Context, Result, bail};
use object::Endianness;
use object::read::elf::{FileHeader, SectionHeader, Sym};

/// Where a defined symbol's bytes are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolBytes {
    pub vaddr: u32,
    pub size: u32,
    /// Offset of the bytes in the ELF file.
    pub file_offset: usize,
}

impl SymbolBytes {
    pub fn get<'a>(&self, elf: &'a [u8]) -> &'a [u8] {
        &elf[self.file_offset..self.file_offset + self.size as usize]
    }

    pub fn get_mut<'a>(&self, elf: &'a mut [u8]) -> &'a mut [u8] {
        &mut elf[self.file_offset..self.file_offset + self.size as usize]
    }
}

/// The defined, file-backed symbol `name`, or `None` when the ELF has no
/// symbol of that name at all (a symbol that exists but is not file-backed
/// is still an error).
pub fn find_opt(elf: &[u8], name: &str) -> Result<Option<SymbolBytes>> {
    let header = object::elf::FileHeader32::<Endianness>::parse(elf)?;
    let endian = header.endian()?;
    let sections = header.sections(endian, elf)?;
    let symbols = sections.symbols(endian, elf, object::elf::SHT_SYMTAB)?;
    let present = symbols
        .iter()
        .any(|s| s.name(endian, symbols.strings()).ok() == Some(name.as_bytes()));
    if !present {
        return Ok(None);
    }
    find(elf, name).map(Some)
}

/// The defined, file-backed symbol `name`.
pub fn find(elf: &[u8], name: &str) -> Result<SymbolBytes> {
    let header = object::elf::FileHeader32::<Endianness>::parse(elf)?;
    let endian = header.endian()?;
    let sections = header.sections(endian, elf)?;
    let symbols = sections.symbols(endian, elf, object::elf::SHT_SYMTAB)?;
    let sym = symbols
        .iter()
        .find(|s| s.name(endian, symbols.strings()).ok() == Some(name.as_bytes()))
        .with_context(|| format!("no symbol `{name}` in the ELF"))?;
    let index = sym.st_shndx(endian);
    if index == object::elf::SHN_UNDEF || index >= object::elf::SHN_LORESERVE {
        bail!("`{name}` is not defined in a section");
    }
    let section = sections.section(object::SectionIndex(index as usize))?;
    if section.sh_type(endian) != object::elf::SHT_PROGBITS {
        bail!("`{name}` is not in a section with file contents");
    }
    let vaddr = sym.st_value(endian);
    let size = sym.st_size(endian);
    let start = section.sh_addr(endian);
    if vaddr < start || vaddr + size > start + section.sh_size(endian) {
        bail!("`{name}` lies outside its section");
    }
    Ok(SymbolBytes {
        vaddr,
        size,
        file_offset: (section.sh_offset(endian) + (vaddr - start)) as usize,
    })
}
