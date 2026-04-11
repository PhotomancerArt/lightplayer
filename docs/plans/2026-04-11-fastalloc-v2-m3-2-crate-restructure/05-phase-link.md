# Phase 5: Create `link.rs`

## Scope

Create `src/link.rs` with two linking backends:
- `link_jit()` — concatenate compiled functions, patch `auipc+jalr` relocations
  in-place, return executable buffer + entry map
- `link_elf()` — build an ELF relocatable object using the `object` crate

Both functions take a `CompiledModule` as input.

The JIT linker logic comes from `rt_jit/compiler.rs` (`JitEmitContext::finalize`
+ `patch_call_plt`). The ELF linker logic comes from the old `emit_module_elf`
in the deleted `rv32/emit.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Create `src/link.rs`

```rust
//! Linking: resolve relocations in compiled modules.
//!
//! - [`link_elf`]: produce a relocatable ELF object (emu path, `object` crate)
//! - [`link_jit`]: concatenate function code, resolve relocations in-place (JIT path)

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::compile::{CompiledModule, NativeReloc};
use crate::error::NativeError;

// ---- ELF linking (emu path) ----

/// Build a relocatable ELF object from a compiled module.
///
/// Each function becomes a symbol in `.text`. Cross-function calls and
/// external (builtin) calls become `R_RISCV_CALL_PLT` relocations.
pub fn link_elf(module: &CompiledModule, entry_flags: &[bool]) -> Result<Vec<u8>, NativeError> {
    use object::write::{Object, Relocation, StandardSection, Symbol, SymbolSection};
    use object::{
        Architecture, BinaryFormat, Endianness, FileFlags, SymbolFlags, SymbolKind, SymbolScope,
        elf,
    };

    let mut obj = Object::new(BinaryFormat::Elf, Architecture::Riscv32, Endianness::Little);
    obj.flags = FileFlags::Elf {
        os_abi: elf::ELFOSABI_NONE,
        abi_version: 0,
        e_flags: elf::EF_RISCV_FLOAT_ABI_SOFT,
    };

    let text = obj.section_id(StandardSection::Text);
    let mut undefined_syms: BTreeMap<String, object::write::SymbolId> = BTreeMap::new();

    for (i, cf) in module.functions.iter().enumerate() {
        let func_off = obj.append_section_data(text, &cf.code, 4);
        let scope = if entry_flags.get(i).copied().unwrap_or(false) {
            SymbolScope::Linkage
        } else {
            SymbolScope::Compilation
        };
        obj.add_symbol(Symbol {
            name: cf.name.as_bytes().to_vec(),
            value: func_off,
            size: cf.code.len() as u64,
            kind: SymbolKind::Text,
            scope,
            weak: false,
            section: SymbolSection::Section(text),
            flags: SymbolFlags::None,
        });

        for r in &cf.relocs {
            let sym_id = if let Some(id) = undefined_syms.get(&r.symbol) {
                *id
            } else {
                let id = obj.add_symbol(Symbol {
                    name: r.symbol.as_bytes().to_vec(),
                    value: 0,
                    size: 0,
                    kind: SymbolKind::Text,
                    scope: SymbolScope::Linkage,
                    weak: false,
                    section: SymbolSection::Undefined,
                    flags: SymbolFlags::None,
                });
                undefined_syms.insert(r.symbol.clone(), id);
                id
            };
            obj.add_relocation(
                text,
                Relocation {
                    offset: func_off + r.offset as u64,
                    symbol: sym_id,
                    addend: 0,
                    flags: object::RelocationFlags::Elf { r_type: 17 },
                },
            )
            .map_err(|e| NativeError::ObjectWrite(e.to_string()))?;
        }
    }

    obj.write()
        .map_err(|e| NativeError::ObjectWrite(e.to_string()))
}

// ---- JIT linking ----

/// Concatenate function code and patch relocations for JIT execution.
///
/// Returns `(code_buffer, entry_offsets)` where entry_offsets maps function
/// names to byte offsets in the buffer.
#[cfg(target_arch = "riscv32")]
pub fn link_jit(
    module: &CompiledModule,
    builtin_table: &crate::rt_jit::builtins::BuiltinTable,
) -> Result<(crate::rt_jit::buffer::JitBuffer, BTreeMap<String, usize>), NativeError> {
    let mut code = Vec::new();
    let mut entries = BTreeMap::new();
    let mut relocs: Vec<NativeReloc> = Vec::new();

    for cf in &module.functions {
        let base = code.len();
        entries.insert(cf.name.clone(), base);
        for r in &cf.relocs {
            relocs.push(NativeReloc {
                offset: r.offset + base,
                symbol: r.symbol.clone(),
            });
        }
        code.extend_from_slice(&cf.code);
    }

    let image_base = code.as_ptr() as usize;
    for r in &relocs {
        let target = resolve_jit_symbol(&entries, builtin_table, &r.symbol, image_base)
            .ok_or_else(|| {
                NativeError::JitLink(alloc::format!(
                    "unresolved symbol `{}` at offset {}",
                    r.symbol, r.offset
                ))
            })?;
        patch_call_plt(&mut code, r.offset, image_base, target)?;
    }

    Ok((crate::rt_jit::buffer::JitBuffer::from_code(code), entries))
}

#[cfg(target_arch = "riscv32")]
fn resolve_jit_symbol(
    entries: &BTreeMap<String, usize>,
    builtin_table: &crate::rt_jit::builtins::BuiltinTable,
    sym: &str,
    image_base: usize,
) -> Option<u32> {
    if let Some(addr) = builtin_table.lookup(sym) {
        return Some(addr as u32);
    }
    entries
        .get(sym)
        .map(|off| image_base.wrapping_add(*off) as u32)
}

/// Patch auipc+jalr pair for RISC-V CALL_PLT relocation.
fn patch_call_plt(
    code: &mut [u8],
    auipc_offset: usize,
    image_base: usize,
    target_addr: u32,
) -> Result<(), NativeError> {
    let off = auipc_offset;
    if off + 8 > code.len() {
        return Err(NativeError::JitLink(String::from(
            "relocation overruns code buffer",
        )));
    }
    let pc = image_base.wrapping_add(off) as u32;
    let auipc_word = u32::from_le_bytes(
        code[off..off + 4]
            .try_into()
            .map_err(|_| NativeError::JitLink(String::from("auipc read")))?,
    );
    let jalr_word = u32::from_le_bytes(
        code[off + 4..off + 8]
            .try_into()
            .map_err(|_| NativeError::JitLink(String::from("jalr read")))?,
    );
    if (auipc_word & 0x7f) != 0x17 || (jalr_word & 0x7f) != 0x67 {
        return Err(NativeError::JitLink(alloc::format!(
            "expected auipc+jalr at offset {off}, got 0x{auipc_word:08x} 0x{jalr_word:08x}"
        )));
    }
    let pcrel = target_addr.wrapping_sub(pc);
    let new_hi20 = ((pcrel >> 12) + u32::from((pcrel & 0x800) != 0)) & 0xFFFFF;
    let new_lo12 = pcrel & 0xFFF;
    let new_auipc = (auipc_word & 0xFFF) | (new_hi20 << 12);
    let new_jalr = (jalr_word & 0xFFFFF) | (new_lo12 << 20);
    code[off..off + 4].copy_from_slice(&new_auipc.to_le_bytes());
    code[off + 4..off + 8].copy_from_slice(&new_jalr.to_le_bytes());
    Ok(())
}
```

### 2. Add `pub mod link;` to `lib.rs`

### 3. Add re-exports

```rust
pub use link::link_elf;
```

## Validate

```bash
cargo check -p lpvm-native-fa
cargo check -p lpvm-native-fa --features emu
```
