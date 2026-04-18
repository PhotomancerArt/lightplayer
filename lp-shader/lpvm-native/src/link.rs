//! Linking: relocation resolution and output generation (JIT / ELF).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use object::write::{Object, Relocation, StandardSection, Symbol, SymbolId, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, FileFlags, SymbolFlags, SymbolKind, SymbolScope, elf,
};

use crate::compile::CompiledModule;
use crate::error::NativeError;
use crate::isa::IsaTarget;

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
    isa: IsaTarget,
    mut resolve_symbol: F,
) -> Result<LinkedJitImage, NativeError>
where
    F: FnMut(&str) -> Option<u32>,
{
    let _ = isa;
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
            // First try the external resolver (for builtins)
            let target = if let Some(addr) = resolve_symbol(&reloc.symbol) {
                addr
            } else {
                // Fall back to intra-module function resolution
                let target_offset = entries.get(&reloc.symbol).ok_or_else(|| {
                    NativeError::Internal(format!(
                        "unresolved symbol `{}` for JIT relocation at offset {}",
                        reloc.symbol, reloc.offset
                    ))
                })?;
                image_base.wrapping_add(*target_offset) as u32
            };

            let absolute_offset = func_base + reloc.offset;
            patch_call_plt(&mut code, absolute_offset, image_base, target)?;
        }
    }

    Ok(LinkedJitImage { code, entries })
}

/// Link compiled module into an ELF relocatable object using the `object` crate.
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
pub fn link_elf(module: &CompiledModule, isa: IsaTarget) -> Result<Vec<u8>, NativeError> {
    let _ = isa;
    let mut obj = Object::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);
    obj.flags = FileFlags::Elf {
        os_abi: elf::ELFOSABI_NONE,
        abi_version: 0,
        e_flags: elf::EF_RISCV_FLOAT_ABI_SOFT,
    };

    let text = obj.section_id(StandardSection::Text);
    let mut symbol_ids: BTreeMap<String, SymbolId> = BTreeMap::new();

    // Add all function symbols first (before appending section data)
    for (idx, func) in module.functions.iter().enumerate() {
        let scope = if idx == 0 {
            SymbolScope::Linkage // First function is entry, make it global
        } else {
            SymbolScope::Compilation
        };

        let sym_id = obj.add_symbol(Symbol {
            name: func.name.as_bytes().to_vec(),
            value: 0, // Will be updated after section data is appended
            size: func.code.len() as u64,
            kind: SymbolKind::Text,
            scope,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });
        symbol_ids.insert(func.name.clone(), sym_id);
    }

    // Append code for each function and update symbol values
    for func in &module.functions {
        let func_off = obj.append_section_data(text, &func.code, 4);

        // Update symbol value to point to the actual offset
        let sym_id = *symbol_ids.get(&func.name).unwrap();
        obj.symbol_mut(sym_id).value = func_off;

        // Add relocations for this function
        for reloc in &func.relocs {
            // Get or create symbol for relocation target
            let target_sym_id = if let Some(id) = symbol_ids.get(&reloc.symbol) {
                *id
            } else {
                // External symbol (e.g., builtin)
                let id = obj.add_symbol(Symbol {
                    name: reloc.symbol.as_bytes().to_vec(),
                    value: 0,
                    size: 0,
                    kind: SymbolKind::Text,
                    scope: SymbolScope::Linkage,
                    weak: false,
                    section: SymbolSection::Undefined,
                    flags: SymbolFlags::None,
                });
                symbol_ids.insert(reloc.symbol.clone(), id);
                id
            };

            // Add R_RISCV_CALL_PLT relocation at the auipc instruction
            // The offset is relative to the function's start in the section
            // Use ELF-specific flags since lp-riscv-elf only understands those
            obj.add_relocation(
                text,
                Relocation {
                    offset: func_off + reloc.offset as u64,
                    symbol: target_sym_id,
                    flags: object::RelocationFlags::Elf {
                        // Standard R_RISCV_CALL_PLT is 17
                        r_type: 17,
                    },
                    addend: 0,
                },
            )
            .map_err(|e| NativeError::Internal(format!("Failed to add relocation: {e}")))?;
        }
    }

    obj.write()
        .map_err(|e| NativeError::Internal(format!("ELF write failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile::NativeReloc;
    use crate::isa::IsaTarget;
    use alloc::string::String;
    use alloc::vec;

    fn simple_compiled_module() -> CompiledModule {
        CompiledModule {
            functions: vec![crate::compile::CompiledFunction {
                name: String::from("test"),
                code: vec![0x13, 0x00, 0x00, 0x00], // nop
                relocs: vec![],
                debug_lines: vec![],
                debug_info: lpvm::FunctionDebugInfo::new("test"),
            }],
            symbols: crate::vinst::ModuleSymbols::default(),
        }
    }

    #[test]
    fn test_link_jit_simple() {
        let module = simple_compiled_module();

        // Resolver returns a fixed address
        let linked = link_jit(&module, IsaTarget::Rv32imac, |_sym| Some(0x1000)).unwrap();

        assert!(!linked.code.is_empty());
        assert_eq!(linked.entries.len(), 1);
        assert!(linked.entries.contains_key("test"));
    }

    #[test]
    fn test_link_elf_basic() {
        let module = simple_compiled_module();
        let elf = link_elf(&module, IsaTarget::Rv32imac).unwrap();

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
                    debug_info: lpvm::FunctionDebugInfo::new("caller"),
                },
                crate::compile::CompiledFunction {
                    name: String::from("callee"),
                    code: vec![0x67, 0x80, 0x00, 0x00], // ret
                    relocs: vec![],
                    debug_lines: vec![],
                    debug_info: lpvm::FunctionDebugInfo::new("callee"),
                },
            ],
            symbols: crate::vinst::ModuleSymbols::default(),
        };

        // Custom resolver that returns the offset of "callee"
        let linked = link_jit(&module, IsaTarget::Rv32imac, |sym| {
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
