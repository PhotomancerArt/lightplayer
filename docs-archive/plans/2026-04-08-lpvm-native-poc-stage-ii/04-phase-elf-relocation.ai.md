# Phase 4: ELF Generation with Relocations

## Scope

Implement `finish_elf()` using the `object` crate to produce a proper ELF object file with `.text` section, symbol table, and `R_RISCV_CALL_PLT` relocations.

## Code Organization

- ELF generation method in EmitContext
- Symbol and relocation handling
- Tests at bottom

## Implementation Details

Add to `Cargo.toml`:
```toml
[dependencies]
object = { version = "0.38", default-features = false, features = ["write_core"] }
```

```rust
use object::write::{Object, Section, StandardSection, Symbol, SymbolSection};
use object::{Architecture, BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope};
use object::elf;

impl EmitContext {
    /// Generate ELF object file from accumulated code and relocations.
    pub fn finish_elf(self, func_name: &str) -> Result<Vec<u8>, NativeError> {
        let mut obj = Object::new(
            BinaryFormat::Elf,
            Architecture::Riscv32,
            Endianness::Little,
        );
        
        // Set RISC-V ELF flags (soft float ABI for ESP32-C6 without FPU)
        obj.flags = object::FileFlags::Elf {
            os_abi: elf::ELFOSABI_NONE,
            abi_version: 0,
            e_flags: elf::EF_RISCV_FLOAT_ABI_SOFT,
        };
        
        // Add .text section
        let text_section = obj.add_section(
            obj.segment_name(StandardSection::Text).to_vec(),
            b".text".to_vec(),
            SectionKind::Text,
        );
        
        // Add function symbol
        let symbol_id = obj.add_symbol(Symbol {
            name: func_name.as_bytes().to_vec(),
            value: 0,
            size: self.code.len() as u64,
            kind: SymbolKind::Text,
            scope: SymbolScope::Compilation,
            weak: false,
            section: SymbolSection::Section(text_section),
            flags: SymbolFlags::None,
        });
        
        // Append code to section
        obj.section_mut(text_section).append(&self.code, 4);
        
        // Add relocations
        for reloc in &self.relocs {
            let reloc_target = match reloc.kind {
                RelocKind::CallPlt => {
                    // Create external symbol for the call target
                    obj.add_symbol(Symbol {
                        name: reloc.symbol.as_bytes().to_vec(),
                        value: 0,
                        size: 0,
                        kind: SymbolKind::Text,
                        scope: SymbolScope::Linkage,
                        weak: false,
                        section: SymbolSection::Undefined,
                        flags: SymbolFlags::None,
                    })
                }
            };
            
            obj.add_relocation(
                text_section,
                object::write::Relocation {
                    offset: reloc.offset as u64,
                    symbol: reloc_target,
                    flags: object::RelocationFlags::Elf {
                        r_type: elf::R_RISCV_CALL_PLT as u32,
                        r_addend: 0,
                    },
                },
            )
            .map_err(|e| NativeError::EmitError(e.to_string()))?;
        }
        
        // Write out ELF
        obj.write().map_err(|e| NativeError::EmitError(e.to_string()))
    }
}
```

## Error Update

Add to `error.rs`:
```rust
#[derive(Debug, Clone)]
pub enum NativeError {
    // ... existing variants ...
    EmitError(String),
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_finish_elf_basic() -> Result<(), NativeError> {
        let mut ctx = EmitContext::new(/*is_leaf=*/ true);
        // Add some code
        ctx.push32(encode_add(10, 8, 9));
        ctx.push32(encode_ret());
        
        let elf = ctx.finish_elf("test_func")?;
        
        // Verify it's a valid ELF
        assert!(elf.len() > 0);
        assert_eq!(&elf[0..4], b"\x7fELF"); // ELF magic
        
        // Parse and verify structure
        let obj = object::File::parse(&*elf).expect("valid ELF");
        assert_eq!(obj.architecture(), object::Architecture::Riscv32);
        
        Ok(())
    }
    
    #[test]
    fn test_finish_elf_with_call_reloc() -> Result<(), NativeError> {
        let mut ctx = EmitContext::new(/*is_leaf=*/ false);
        ctx.emit_call("__lpir_fadd_q32");
        ctx.push32(encode_ret());
        
        let elf = ctx.finish_elf("test_call")?;
        
        // Parse and verify relocation exists
        let obj = object::File::parse(&*elf).expect("valid ELF");
        let relocs: Vec<_> = obj.dynamic_relocations().unwrap_or_default().collect();
        // Or iterate sections and check relocation sections
        
        Ok(())
    }
}
```

## Key Points

- Uses `object` crate with `write_core` feature (no_std compatible)
- Sets `EF_RISCV_FLOAT_ABI_SOFT` flag (ESP32-C6 without FPU)
- Creates external symbols for call targets (undefined section)
- `R_RISCV_CALL_PLT` covers auipc+jalr pair at relocation offset

## Validate

```bash
cargo test -p lpvm-native --lib finish_elf::tests
cargo check -p lpvm-native
```

## References

- `object` crate docs: https://docs.rs/object/0.38.1/object/
- RISC-V ELF psABI: `docs/roadmaps/2026-04-07-lpvm-native-poc/references.md`
