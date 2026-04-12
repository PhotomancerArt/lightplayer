//! Linking: relocation resolution and output generation (JIT / ELF).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::compile::CompiledModule;
use crate::error::NativeError;

/// Linked JIT image with entry offsets.
#[derive(Clone, Debug)]
pub struct LinkedJitImage {
    /// Executable machine code bytes.
    pub code: Vec<u8>,
    /// Function name → offset in code.
    pub entries: BTreeMap<String, usize>,
}

/// Patch auipc+jalr call sequence at given offset.
///
/// Standard RISC-V `R_RISCV_CALL_PLT` style fixup.
fn patch_call_plt(
    code: &mut [u8],
    auipc_offset: usize,
    image_base: usize,
    target_addr: u32,
) -> Result<(), NativeError> {
    let off = auipc_offset;
    if off.saturating_add(8) > code.len() {
        return Err(NativeError::Internal(String::from(
            "relocation overruns code buffer",
        )));
    }

    let pc = image_base.wrapping_add(off) as u32;

    let auipc_word = u32::from_le_bytes(
        code[off..off + 4]
            .try_into()
            .map_err(|_| NativeError::Internal(String::from("auipc read")))?,
    );
    let jalr_word = u32::from_le_bytes(
        code[off + 4..off + 8]
            .try_into()
            .map_err(|_| NativeError::Internal(String::from("jalr read")))?,
    );

    // Verify auipc+jalr encoding
    if (auipc_word & 0x7f) != 0x17 || (jalr_word & 0x7f) != 0x67 {
        return Err(NativeError::Internal(format!(
            "expected auipc+jalr at offset {off}, got 0x{auipc_word:08x} 0x{jalr_word:08x}"
        )));
    }

    let pcrel = target_addr.wrapping_sub(pc);
    let new_hi20 = ((pcrel >> 12).wrapping_add(u32::from((pcrel & 0x800) != 0))) & 0xFFFFF;
    let new_lo12 = pcrel & 0xFFF;

    let new_auipc = (auipc_word & 0xFFF) | (new_hi20 << 12);
    let new_jalr = (jalr_word & 0xFFFFF) | (new_lo12 << 20);

    code[off..off + 4].copy_from_slice(&new_auipc.to_le_bytes());
    code[off + 4..off + 8].copy_from_slice(&new_jalr.to_le_bytes());

    Ok(())
}

/// Resolve all relocations and produce a JIT-ready image.
///
/// # Arguments
/// * `module` - Compiled module with functions and relocations
/// * `resolve_symbol` - Callback to resolve symbol names to addresses
///
/// # Returns
/// Linked JIT image with all call sites patched.
pub fn link_jit<F>(
    module: &CompiledModule,
    mut resolve_symbol: F,
) -> Result<LinkedJitImage, NativeError>
where
    F: FnMut(&str) -> Option<u32>,
{
    // Concatenate all function code
    let mut code = Vec::new();
    let mut entries = BTreeMap::new();
    let mut func_offsets = Vec::with_capacity(module.functions.len());

    for func in &module.functions {
        let offset = code.len();
        entries.insert(func.name.clone(), offset);
        func_offsets.push(offset);
        code.extend_from_slice(&func.code);
    }

    let image_base = code.as_ptr() as usize;

    // Resolve relocations
    for (func_idx, func) in module.functions.iter().enumerate() {
        let func_base = func_offsets[func_idx];

        for reloc in &func.relocs {
            let target = resolve_symbol(&reloc.symbol).ok_or_else(|| {
                NativeError::Internal(format!(
                    "unresolved symbol `{}` for JIT relocation at offset {}",
                    reloc.symbol, reloc.offset
                ))
            })?;

            let absolute_offset = func_base + reloc.offset;
            patch_call_plt(&mut code, absolute_offset, image_base, target)?;
        }
    }

    Ok(LinkedJitImage { code, entries })
}

/// Minimal ELF header for RV32 (little-endian, soft-float).
const ELFMAG: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS32: u8 = 1;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ET_REL: u16 = 1;
const EM_RISCV: u16 = 243;

/// ELF section header types.
const SHT_NULL: u32 = 0;
const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;

/// ELF relocation type for RISC-V CALL_PLT.
const R_RISCV_CALL_PLT: u32 = 18;

/// Minimal ELF writer state.
struct ElfWriter {
    #[allow(dead_code)]
    data: Vec<u8>,
    shstrtab: Vec<u8>,
    strtab: Vec<u8>,
    symtab: Vec<u8>,
    #[allow(dead_code)]
    sections: Vec<ElfSection>,
    #[allow(dead_code)]
    relocs: Vec<ElfReloc>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct ElfSection {
    name_idx: u32,
    sh_type: u32,
    flags: u64,
    offset: u64,
    size: u64,
    data: Vec<u8>,
}

#[derive(Clone, Debug)]
struct ElfReloc {
    offset: u64,
    sym_idx: u32,
    r_type: u32,
    addend: i64,
}

impl ElfWriter {
    fn new() -> Self {
        let mut shstrtab = vec![0u8]; // First byte is null
        let strtab = vec![0u8];

        // Reserve name indices
        let _sh_null = 0u32;
        let _sh_name_text = Self::add_str(&mut shstrtab, ".text\0");
        let _sh_name_symtab = Self::add_str(&mut shstrtab, ".symtab\0");
        let _sh_name_strtab = Self::add_str(&mut shstrtab, ".strtab\0");
        let _sh_name_shstrtab = Self::add_str(&mut shstrtab, ".shstrtab\0");
        let _sh_name_rela = Self::add_str(&mut shstrtab, ".rela.text\0");

        let sections = vec![
            ElfSection::null(), // SHN_UNDEF
        ];

        Self {
            data: Vec::new(),
            shstrtab,
            strtab,
            symtab: Vec::new(),
            sections,
            relocs: Vec::new(),
        }
    }

    fn add_str(tab: &mut Vec<u8>, s: &str) -> u32 {
        let idx = tab.len() as u32;
        tab.extend_from_slice(s.as_bytes());
        idx
    }

    fn add_symbol(&mut self, name: &str, value: u64, size: u64, info: u8) -> u32 {
        let name_idx = Self::add_str(&mut self.strtab, &format!("{}\0", name));
        let idx = self.symtab.len() / 16; // Each symtab entry is 16 bytes for 32-bit

        // Symbol table entry (Elf32_Sym)
        self.symtab.extend_from_slice(&name_idx.to_le_bytes()); // st_name
        self.symtab.extend_from_slice(&value.to_le_bytes()); // st_value
        self.symtab.extend_from_slice(&size.to_le_bytes()); // st_size
        self.symtab.push(info); // st_info
        self.symtab.push(0); // st_other
        self.symtab.extend_from_slice(&1u16.to_le_bytes()); // st_shndx (text section)

        idx as u32
    }
}

impl ElfSection {
    fn null() -> Self {
        Self {
            name_idx: 0,
            sh_type: SHT_NULL,
            flags: 0,
            offset: 0,
            size: 0,
            data: Vec::new(),
        }
    }
}

/// Link compiled module into an ELF relocatable object.
///
/// This produces a standard ELF file that can be:
/// - Linked with other objects
/// - Loaded by the emulation runtime
/// - Inspected with standard tools (readelf, objdump)
///
/// # Arguments
/// * `module` - Compiled module with functions and relocations
///
/// # Returns
/// ELF object file as bytes.
pub fn link_elf(module: &CompiledModule) -> Result<Vec<u8>, NativeError> {
    let mut writer = ElfWriter::new();

    // Add function symbols
    let mut func_sym_indices = Vec::with_capacity(module.functions.len());
    for (idx, func) in module.functions.iter().enumerate() {
        let is_entry = idx == 0; // First function is entry
        let info = if is_entry { 0x12 } else { 0x02 }; // STB_GLOBAL | STT_FUNC or STT_FUNC
        let sym_idx = writer.add_symbol(&func.name, 0, func.code.len() as u64, info);
        func_sym_indices.push(sym_idx);
    }

    // Collect all code and create relocations
    let mut text_data = Vec::new();
    let mut relocs = Vec::new();

    for func in &module.functions {
        let func_offset = text_data.len() as u64;

        // Copy code
        text_data.extend_from_slice(&func.code);

        // Convert relocations to ELF relocations
        for reloc in &func.relocs {
            // Find target symbol index
            let target_sym = module
                .functions
                .iter()
                .position(|f| f.name == reloc.symbol)
                .map(|idx| func_sym_indices[idx])
                .unwrap_or(0); // External symbol (TODO: handle builtins)

            relocs.push(ElfReloc {
                offset: func_offset + reloc.offset as u64,
                sym_idx: target_sym,
                r_type: R_RISCV_CALL_PLT,
                addend: 0,
            });
        }
    }

    // Build ELF file
    let text_offset = 52u64; // After ELF header
    let shstrtab_offset = text_offset + text_data.len() as u64;
    let strtab_offset = shstrtab_offset + writer.shstrtab.len() as u64;

    // Calculate symtab size (16 bytes per entry for 32-bit)
    let symtab_size = writer.symtab.len() as u64;
    let symtab_offset = strtab_offset + writer.strtab.len() as u64;

    // Rela section
    let rela_size = relocs.len() as u64 * 12; // Elf32_Rela = 12 bytes
    let rela_offset = symtab_offset + symtab_size;

    // Section headers offset (must be aligned)
    let sh_offset = rela_offset + rela_size;
    let sh_offset_aligned = (sh_offset + 3) & !3;

    // Build header
    let mut result = Vec::with_capacity(sh_offset_aligned as usize + 40 * 10);

    // ELF32 header (52 bytes)
    result.extend_from_slice(&ELFMAG); // e_ident[0..4]
    result.push(ELFCLASS32); // e_ident[4]
    result.push(ELFDATA2LSB); // e_ident[5]
    result.push(EV_CURRENT); // e_ident[6]
    result.push(1u8); // e_ident[7] - OS/ABI (System V)
    result.extend_from_slice(&[0u8; 8]); // e_ident[8..16]
    result.extend_from_slice(&ET_REL.to_le_bytes()); // e_type
    result.extend_from_slice(&EM_RISCV.to_le_bytes()); // e_machine
    result.extend_from_slice(&1u32.to_le_bytes()); // e_version
    result.extend_from_slice(&0u32.to_le_bytes()); // e_entry
    result.extend_from_slice(&0u32.to_le_bytes()); // e_phoff
    result.extend_from_slice(&(sh_offset_aligned as u32).to_le_bytes()); // e_shoff
    result.extend_from_slice(&0u32.to_le_bytes()); // e_flags (no specific flags)
    result.extend_from_slice(&52u16.to_le_bytes()); // e_ehsize
    result.extend_from_slice(&0u16.to_le_bytes()); // e_phentsize
    result.extend_from_slice(&0u16.to_le_bytes()); // e_phnum
    result.extend_from_slice(&40u16.to_le_bytes()); // e_shentsize
    result.extend_from_slice(&6u16.to_le_bytes()); // e_shnum (null + text + symtab + strtab + shstrtab + rela)
    result.extend_from_slice(&4u16.to_le_bytes()); // e_shstrndx

    // .text section
    result.extend_from_slice(&text_data);

    // .shstrtab section
    result.extend_from_slice(&writer.shstrtab);

    // .strtab section
    result.extend_from_slice(&writer.strtab);

    // .symtab section
    result.extend_from_slice(&writer.symtab);

    // .rela.text section
    for reloc in &relocs {
        result.extend_from_slice(&(reloc.offset as u32).to_le_bytes());
        result.extend_from_slice(&((reloc.sym_idx << 8) | reloc.r_type).to_le_bytes());
        result.extend_from_slice(&(reloc.addend as i32).to_le_bytes());
    }

    // Pad to section header alignment
    while result.len() < sh_offset_aligned as usize {
        result.push(0);
    }

    // Section headers (40 bytes each for 32-bit)
    // 0: NULL
    result.extend_from_slice(&ElfSection::null_section_header());

    // 1: .text (sh_name, sh_type, sh_flags, sh_addr, sh_offset, sh_size, sh_link, sh_info, sh_addralign, sh_entsize)
    result.extend_from_slice(&1u32.to_le_bytes()); // sh_name
    result.extend_from_slice(&SHT_PROGBITS.to_le_bytes()); // sh_type
    result.extend_from_slice(&6u64.to_le_bytes()); // sh_flags (SHF_ALLOC | SHF_EXECINSTR)
    result.extend_from_slice(&0u64.to_le_bytes()); // sh_addr
    result.extend_from_slice(&text_offset.to_le_bytes()); // sh_offset
    result.extend_from_slice(&(text_data.len() as u32).to_le_bytes()); // sh_size
    result.extend_from_slice(&0u32.to_le_bytes()); // sh_link
    result.extend_from_slice(&0u32.to_le_bytes()); // sh_info
    result.extend_from_slice(&4u32.to_le_bytes()); // sh_addralign
    result.extend_from_slice(&0u32.to_le_bytes()); // sh_entsize

    // 2: .symtab
    result.extend_from_slice(&7u32.to_le_bytes());
    result.extend_from_slice(&SHT_SYMTAB.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&symtab_offset.to_le_bytes());
    result.extend_from_slice(&(writer.strtab.len() as u32).to_le_bytes()); // sh_size
    result.extend_from_slice(&3u32.to_le_bytes()); // sh_link -> .strtab
    result.extend_from_slice(&(func_sym_indices.len() as u32 + 1).to_le_bytes()); // sh_info (first global)
    result.extend_from_slice(&4u32.to_le_bytes()); // sh_addralign
    result.extend_from_slice(&16u32.to_le_bytes()); // sh_entsize

    // 3: .strtab
    result.extend_from_slice(&15u32.to_le_bytes());
    result.extend_from_slice(&SHT_STRTAB.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&strtab_offset.to_le_bytes());
    result.extend_from_slice(&(writer.strtab.len() as u32).to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&1u32.to_le_bytes()); // sh_addralign
    result.extend_from_slice(&0u32.to_le_bytes());

    // 4: .shstrtab
    result.extend_from_slice(&24u32.to_le_bytes());
    result.extend_from_slice(&SHT_STRTAB.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&shstrtab_offset.to_le_bytes());
    result.extend_from_slice(&(writer.shstrtab.len() as u32).to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());
    result.extend_from_slice(&1u32.to_le_bytes());
    result.extend_from_slice(&0u32.to_le_bytes());

    // 5: .rela.text
    result.extend_from_slice(&35u32.to_le_bytes());
    result.extend_from_slice(&SHT_RELA.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&0u64.to_le_bytes());
    result.extend_from_slice(&rela_offset.to_le_bytes());
    result.extend_from_slice(&(relocs.len() as u32 * 12).to_le_bytes()); // sh_size
    result.extend_from_slice(&2u32.to_le_bytes()); // sh_link -> .symtab
    result.extend_from_slice(&(func_sym_indices.len() as u32 + 1).to_le_bytes()); // sh_info
    result.extend_from_slice(&4u32.to_le_bytes());
    result.extend_from_slice(&12u32.to_le_bytes());

    Ok(result)
}

trait ElfSectionExt {
    fn null_section_header() -> Vec<u8>;
}

impl ElfSectionExt for ElfSection {
    fn null_section_header() -> Vec<u8> {
        vec![0u8; 40]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::NativeReloc;
    use alloc::string::String;
    use alloc::vec;

    fn simple_compiled_module() -> CompiledModule {
        CompiledModule {
            functions: vec![crate::compile::CompiledFunction {
                name: String::from("test"),
                code: vec![0x13, 0x00, 0x00, 0x00], // nop
                relocs: vec![],
                debug_lines: vec![],
            }],
            symbols: crate::vinst::ModuleSymbols::default(),
        }
    }

    #[test]
    fn test_link_jit_simple() {
        let module = simple_compiled_module();

        // Resolver returns a fixed address
        let linked = link_jit(&module, |_sym| Some(0x1000)).unwrap();

        assert!(!linked.code.is_empty());
        assert_eq!(linked.entries.len(), 1);
        assert!(linked.entries.contains_key("test"));
    }

    #[test]
    fn test_link_elf_basic() {
        let module = simple_compiled_module();
        let elf = link_elf(&module).unwrap();

        // Check ELF magic
        assert_eq!(&elf[0..4], &[0x7f, b'E', b'L', b'F']);
        // Check 32-bit
        assert_eq!(elf[4], 1);
        // Check little-endian
        assert_eq!(elf[5], 1);
        // Check RISC-V machine
        let machine = u16::from_le_bytes([elf[18], elf[19]]);
        assert_eq!(machine, 243);
    }

    #[test]
    fn test_link_jit_with_call() {
        // Module with two functions where one calls the other
        let module = CompiledModule {
            functions: vec![
                crate::compile::CompiledFunction {
                    name: String::from("caller"),
                    // auipc + jalr for call (8 bytes) + ret (4 bytes)
                    code: vec![
                        0x97, 0x02, 0x00, 0x00, // auipc t0, 0
                        0x67, 0x00, 0x02, 0x00, // jalr x0, t0, 0
                        0x67, 0x80, 0x00, 0x00, // ret
                    ],
                    relocs: vec![NativeReloc {
                        offset: 0,
                        symbol: String::from("callee"),
                    }],
                    debug_lines: vec![],
                },
                crate::compile::CompiledFunction {
                    name: String::from("callee"),
                    code: vec![0x67, 0x80, 0x00, 0x00], // ret
                    relocs: vec![],
                    debug_lines: vec![],
                },
            ],
            symbols: crate::vinst::ModuleSymbols::default(),
        };

        // Custom resolver that returns the offset of "callee"
        let linked = link_jit(&module, |sym| {
            if sym == "caller" {
                Some(0x1000)
            } else if sym == "callee" {
                Some(0x1010) // callee starts 16 bytes after caller
            } else {
                None
            }
        })
        .unwrap();

        // Code should be concatenated
        assert_eq!(linked.code.len(), 12 + 4); // caller + callee
        assert_eq!(linked.entries["caller"], 0);
        assert_eq!(linked.entries["callee"], 12);
    }
}
