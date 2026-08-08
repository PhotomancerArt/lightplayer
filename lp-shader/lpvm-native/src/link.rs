//! Linking: relocation resolution and output generation (JIT / ELF).

use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;
use object::write::{Object, Relocation, StandardSection, Symbol, SymbolId, SymbolSection};
use object::{BinaryFormat, Endianness, FileFlags, SymbolFlags, SymbolKind, SymbolScope, elf};

use crate::compile::CompiledModule;
#[cfg(feature = "isa-rv32")]
use crate::compile::NativeReloc;
use crate::error::NativeError;
use crate::isa::IsaTarget;

/// Linked JIT image with entry offsets.
#[derive(Clone, Debug)]
pub struct LinkedJitImage {
    /// Executable machine code bytes.
    pub code: Vec<u8>,
    /// Function name → offset in code.
    pub entries: VecMap<String, usize>,
}

/// Resolve all relocations and produce a JIT-ready image that executes **in
/// place** — from the returned `code` Vec itself, through the target's
/// write→execute rule ([`crate::exec_addr`]).
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
    resolve_symbol: F,
) -> Result<LinkedJitImage, NativeError>
where
    F: FnMut(&str) -> Option<u32>,
{
    let (code, entries, func_offsets) = concat_borrowed(&module.functions);
    link_jit_patch(
        &module.functions,
        code,
        entries,
        func_offsets,
        isa,
        None,
        resolve_symbol,
    )
}

/// [`link_jit`] that **empties** the module as it goes: each function's code
/// is moved into the image and its buffer freed immediately, instead of the
/// whole module staying resident beside a full second copy of itself.
///
/// This is the shape every JIT caller wants — nothing reads `module` after a
/// link — and it is the difference between a 1x and a 2x code peak at the
/// worst moment of a device compile. Names, relocs and symbols survive; only
/// `CompiledFunction::code` is taken (left empty).
pub fn link_jit_taking<F>(
    module: &mut CompiledModule,
    isa: IsaTarget,
    resolve_symbol: F,
) -> Result<LinkedJitImage, NativeError>
where
    F: FnMut(&str) -> Option<u32>,
{
    let (code, entries, func_offsets) = concat_taking(&mut module.functions);
    link_jit_patch(
        &module.functions,
        code,
        entries,
        func_offsets,
        isa,
        None,
        resolve_symbol,
    )
}

/// [`link_jit_at`] with [`link_jit_taking`]'s move-out-the-code behavior.
pub fn link_jit_at_taking<F>(
    module: &mut CompiledModule,
    isa: IsaTarget,
    exec_base: u32,
    resolve_symbol: F,
) -> Result<LinkedJitImage, NativeError>
where
    F: FnMut(&str) -> Option<u32>,
{
    let (code, entries, func_offsets) = concat_taking(&mut module.functions);
    link_jit_patch(
        &module.functions,
        code,
        entries,
        func_offsets,
        isa,
        Some(exec_base),
        resolve_symbol,
    )
}

/// [`link_jit`] for an image that will be **copied elsewhere before
/// execution**: intra-module call targets are patched against `exec_base`,
/// the address byte 0 of the image will be *fetched* at after installation —
/// not against the staging Vec's own address.
///
/// This is the classic-ESP32 path ([`crate::codemem_esp32`]): the staging Vec
/// lives in non-executable heap, so the in-place rule has no valid answer for
/// it; the caller reserves a span in the fixed code region first and links
/// against that. It is also how the host tests link images for execution at
/// an emulator's code base.
pub fn link_jit_at<F>(
    module: &CompiledModule,
    isa: IsaTarget,
    exec_base: u32,
    resolve_symbol: F,
) -> Result<LinkedJitImage, NativeError>
where
    F: FnMut(&str) -> Option<u32>,
{
    let (code, entries, func_offsets) = concat_borrowed(&module.functions);
    link_jit_patch(
        &module.functions,
        code,
        entries,
        func_offsets,
        isa,
        Some(exec_base),
        resolve_symbol,
    )
}

/// Sizing rule shared by both concatenation halves: the total is known up
/// front, and sizing exactly avoids grow-reallocs of the largest buffer in
/// the pipeline AND is load-bearing for correctness — `image_base` is taken
/// from the buffer's address in [`link_jit_patch`] and patched into the code,
/// so the Vec must not reallocate (and thus move) after it is filled.
fn image_capacity(functions: &[crate::compile::CompiledFunction]) -> usize {
    functions.iter().map(|f| f.code.len()).sum()
}

/// Concatenate every function's code, copying (the module stays whole).
fn concat_borrowed(
    functions: &[crate::compile::CompiledFunction],
) -> (Vec<u8>, VecMap<String, usize>, Vec<usize>) {
    let total_code = image_capacity(functions);
    let mut code = Vec::with_capacity(total_code);
    let mut entries = VecMap::new();
    let mut func_offsets = Vec::with_capacity(functions.len());
    for func in functions {
        let offset = code.len();
        entries.insert(func.name.clone(), offset);
        func_offsets.push(offset);
        code.extend_from_slice(&func.code);
    }
    debug_assert_eq!(
        code.len(),
        total_code,
        "image fully written before patching"
    );
    (code, entries, func_offsets)
}

/// Concatenate every function's code, **moving** it: each source buffer is
/// dropped as soon as its bytes are in the image, so the peak is one image
/// plus one function — not two whole copies of the module's code.
fn concat_taking(
    functions: &mut [crate::compile::CompiledFunction],
) -> (Vec<u8>, VecMap<String, usize>, Vec<usize>) {
    let total_code = image_capacity(functions);
    let mut code = Vec::with_capacity(total_code);
    let mut entries = VecMap::new();
    let mut func_offsets = Vec::with_capacity(functions.len());
    for func in functions {
        let offset = code.len();
        entries.insert(func.name.clone(), offset);
        func_offsets.push(offset);
        let owned = core::mem::take(&mut func.code);
        code.extend_from_slice(&owned);
        drop(owned);
    }
    debug_assert_eq!(
        code.len(),
        total_code,
        "image fully written before patching"
    );
    (code, entries, func_offsets)
}

/// Patch relocations into an already-concatenated image. Reads only the
/// functions' names, relocs and offsets — never their `code`, which may
/// already have been moved into `code` by [`concat_taking`].
#[allow(
    clippy::too_many_arguments,
    reason = "the concatenation half's three outputs are threaded through explicitly"
)]
fn link_jit_patch<F>(
    functions: &[crate::compile::CompiledFunction],
    mut code: Vec<u8>,
    entries: VecMap<String, usize>,
    func_offsets: Vec<usize>,
    isa: IsaTarget,
    exec_base: Option<u32>,
    mut resolve_symbol: F,
) -> Result<LinkedJitImage, NativeError>
where
    F: FnMut(&str) -> Option<u32>,
{
    let image_base = code.as_ptr() as usize;

    // Resolve relocations
    for (func_idx, func) in functions.iter().enumerate() {
        let func_base = func_offsets[func_idx];

        for reloc in &func.relocs {
            // First try the external resolver (for builtins)
            let target = if let Some(addr) = resolve_symbol(&reloc.symbol) {
                addr
            } else {
                // Fall back to intra-module function resolution.
                //
                // The resolver's answers (builtins) are addresses of code
                // linked into the firmware, so they are already execute
                // addresses. Intra-module targets are not: the emitted code
                // jumps to whatever goes in here, so it must be the address
                // the fetch path will name. In-place (`exec_base == None`)
                // that is the staging Vec's own address through the target's
                // write→execute rule — on the S3 the I-bus alias, not the
                // D-bus address the bytes were stored through (see
                // `crate::exec_addr`). For a placed image it is the caller's
                // `exec_base`, where the bytes will be installed.
                let target_offset = entries.get(&reloc.symbol).ok_or_else(|| {
                    NativeError::Internal(format!(
                        "unresolved symbol `{}` for JIT relocation at offset {}",
                        reloc.symbol, reloc.offset
                    ))
                })?;
                match exec_base {
                    None => {
                        crate::exec_addr::exec_addr(image_base.wrapping_add(*target_offset)) as u32
                    }
                    Some(base) => base.wrapping_add(*target_offset as u32),
                }
            };

            let absolute_offset = func_base + reloc.offset;
            match isa {
                #[cfg(feature = "isa-rv32")]
                IsaTarget::Rv32imac => {
                    if reloc.r_type == isa.call_reloc_type() {
                        let abs_reloc = NativeReloc {
                            offset: absolute_offset,
                            symbol: String::new(),
                            r_type: reloc.r_type,
                        };
                        // PC-relative patching needs the address the code
                        // will EXECUTE at: in place that is the write base
                        // (identity rule on every rv32 target), for a placed
                        // image it is the caller's exec_base.
                        let pc_base = match exec_base {
                            None => image_base,
                            Some(base) => base as usize,
                        };
                        crate::isa::rv32::link::patch_call_plt(
                            &mut code, &abs_reloc, pc_base, target,
                        )?;
                    } else {
                        return Err(NativeError::Internal(alloc::format!(
                            "unsupported JIT relocation r_type {} for ISA {:?}",
                            reloc.r_type,
                            isa
                        )));
                    }
                }
                #[cfg(feature = "isa-xt")]
                IsaTarget::Xtensa => {
                    if reloc.r_type == isa.call_reloc_type() {
                        // R_XTENSA_32 on a literal-pool slot: the emitted call
                        // sequence is `l32r SCRATCH, <slot>; callx8 SCRATCH`,
                        // so patching is writing the callee's absolute execute
                        // address into the 4-byte slot.
                        let off = absolute_offset;
                        let slot = code.get_mut(off..off + 4).ok_or_else(|| {
                            NativeError::Internal(alloc::format!(
                                "Xtensa literal-slot relocation at {off:#x} out of code bounds"
                            ))
                        })?;
                        slot.copy_from_slice(&target.to_le_bytes());
                    } else {
                        return Err(NativeError::Internal(alloc::format!(
                            "unsupported JIT relocation r_type {} for ISA {:?}",
                            reloc.r_type,
                            isa
                        )));
                    }
                }
            }
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
    let arch = isa.elf_architecture();
    let e_flags = isa.elf_e_flags();
    let mut obj = Object::new(BinaryFormat::Elf, arch, Endianness::Little);
    obj.flags = FileFlags::Elf {
        os_abi: elf::ELFOSABI_NONE,
        abi_version: 0,
        e_flags,
    };

    let text = obj.section_id(StandardSection::Text);
    let mut symbol_ids: VecMap<String, SymbolId> = VecMap::new();

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

            // Add the target's direct-call relocation at the call instruction
            // The offset is relative to the function's start in the section
            // Use ELF-specific flags since lp-riscv-elf only understands those
            obj.add_relocation(
                text,
                Relocation {
                    offset: func_off + reloc.offset as u64,
                    symbol: target_sym_id,
                    flags: object::RelocationFlags::Elf {
                        r_type: isa.call_reloc_type(),
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
                debug_lines: None,
                debug_info: None,
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
                        r_type: crate::isa::rv32::link::R_RISCV_CALL_PLT,
                    }],
                    debug_lines: None,
                    debug_info: None,
                },
                crate::compile::CompiledFunction {
                    name: String::from("callee"),
                    code: vec![0x67, 0x80, 0x00, 0x00], // ret
                    relocs: vec![],
                    debug_lines: None,
                    debug_info: None,
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
