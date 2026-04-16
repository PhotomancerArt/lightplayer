# Phase 3: Create JitEmitContext

## Scope

Create `JitEmitContext` that emits machine code and records relocations. This follows cranelift's pattern of emitting with placeholders, then patching during finalize.

## Implementation Details

### 1. Update `lpvm-native/src/isa/rv32/emit.rs`

Add a new function that emits to JIT context instead of ELF:

```rust
/// Emit one function to JIT context (code + relocations).
///
/// This is the JIT path - emits to buffer with relocations recorded,
/// which are then resolved and patched during finalize.
pub fn emit_function_to_jit(
    func: &lpir::IrFunction,
    ir: &lpir::IrModule,
    module_abi: &ModuleAbi,
    fn_sig: &lps_shared::LpsFnSig,
    float_mode: lpir::FloatMode,
    alloc_trace: bool,
) -> Result<(Vec<u8>, Vec<NativeReloc>), NativeError> {
    // Reuse existing emit_function_bytes logic
    // But return (code, relocs) instead of EmittedFunction
    let emitted = emit_function_bytes(
        func, ir, module_abi, fn_sig, float_mode, false, alloc_trace
    )?;
    
    Ok((emitted.code, emitted.relocs))
}
```

### 2. Create `lpvm-native/src/rt_jit/compiler.rs`

```rust
//! JIT compilation: emit code with relocations, then finalize.

use crate::error::NativeError;
use crate::isa::rv32::emit::emit_function_to_jit;
use crate::isa::rv32::inst::{encode_addi, encode_auipc, encode_jalr};
use crate::vinst::NativeReloc;
use crate::abi::ModuleAbi;
use super::buffer::JitBuffer;
use super::builtins::BuiltinTable;
use alloc::vec::Vec;
use lpir;
use lps_shared;

/// Context for JIT compilation of a single module.
///
/// Following cranelift's pattern: emit code with placeholder instructions,
/// record relocations, then patch them during finalize.
pub struct JitEmitContext<'a> {
    /// Code bytes (with placeholder instructions for calls)
    code: Vec<u8>,
    /// Relocations to resolve during finalize
    relocs: Vec<NativeReloc>,
    /// Entry points: function name → byte offset
    entries: alloc::collections::BTreeMap<&'a str, usize>,
    /// Reference to builtin table for relocation resolution
    builtin_table: &'a BuiltinTable,
}

impl<'a> JitEmitContext<'a> {
    /// Create new emit context.
    pub fn new(builtin_table: &'a BuiltinTable) -> Self {
        Self {
            code: Vec::new(),
            relocs: Vec::new(),
            entries: alloc::collections::BTreeMap::new(),
            builtin_table,
        }
    }

    /// Emit one function to the context.
    ///
    /// Appends code to internal buffer and records entry point.
    pub fn emit_function(
        &mut self,
        func: &lpir::IrFunction,
        ir: &lpir::IrModule,
        module_abi: &ModuleAbi,
        fn_sig: &lps_shared::LpsFnSig,
        float_mode: lpir::FloatMode,
        alloc_trace: bool,
    ) -> Result<(), NativeError> {
        let offset = self.code.len();
        
        let (code, relocs) = emit_function_to_jit(
            func, ir, module_abi, fn_sig, float_mode, alloc_trace
        )?;
        
        self.code.extend_from_slice(&code);
        self.relocs.extend(relocs);
        self.entries.insert(&func.name, offset);
        
        Ok(())
    }

    /// Finalize: resolve relocations and return executable buffer.
    ///
    /// This performs the actual relocation patching, following cranelift's pattern:
    /// 1. For each relocation, look up target address
    /// 2. Patch the auipc+jalr placeholder with resolved PC-relative address
    pub fn finalize(mut self) -> Result<JitModuleImage, NativeError> {
        // Allocate executable buffer
        let capacity = self.code.len().max(4096); // Minimum 4KB
        let mut buffer = JitBuffer::with_capacity(capacity)
            .map_err(|_| NativeError::Alloc(alloc::string::String::from("JIT buffer alloc failed")))?;
        
        // Copy code to buffer
        buffer.push_bytes(&self.code);
        
        // Resolve and patch relocations
        for reloc in &self.relocs {
            let target_addr = self.builtin_table.lookup(&reloc.symbol)
                .ok_or_else(|| NativeError::UnresolvedSymbol(reloc.symbol.clone()))?;
            
            // Patch the auipc+jalr pair at reloc.offset
            unsafe {
                patch_call_reloc(buffer.as_mut_ptr(), reloc.offset, target_addr)?;
            }
        }
        
        Ok(JitModuleImage {
            buffer,
            entries: self.entries,
        })
    }
}

/// Result of finalizing JIT compilation.
pub struct JitModuleImage {
    pub buffer: JitBuffer,
    pub entries: alloc::collections::BTreeMap<&'static str, usize>,
}

/// Patch auipc+jalr call relocation.
///
/// # Safety
/// - ptr must point to valid code buffer
/// - offset must be 4-byte aligned and point to auipc instruction
/// - target_addr must be valid function address
unsafe fn patch_call_reloc(
    ptr: *mut u8,
    offset: usize,
    target_addr: usize,
) -> Result<(), NativeError> {
    let auipc_addr = ptr.add(offset) as *mut u32;
    let jalr_addr = ptr.add(offset + 4) as *mut u32;
    
    // Current PC at auipc (actual runtime address)
    let pc = auipc_addr as usize;
    
    // PC-relative offset to target
    let pcrel = target_addr.wrapping_sub(pc) as i32;
    
    // RISC-V auipc+jalr encoding (following cranelift's pattern)
    // auipc rd, imm[31:12]  -> opcode 0x17
    // jalr rd, rs1, imm[11:0] -> opcode 0x67
    
    // Split into hi20 and lo12 (with rounding for sign extension)
    let hi20 = pcrel.wrapping_add(0x800) & 0xFFFFF000;
    let lo12 = pcrel.wrapping_sub(hi20) & 0xFFF;
    
    // Read current instructions (should be placeholders: auipc ra, 0; jalr ra, ra, 0)
    let auipc_inst = auipc_addr.read_unaligned();
    let jalr_inst = jalr_addr.read_unaligned();
    
    // Verify they are auipc+jalr (sanity check)
    if (auipc_inst & 0x7F) != 0x17 || (jalr_inst & 0x7F) != 0x67 {
        return Err(NativeError::InvalidRelocation);
    }
    
    // Patch: auipc keeps rd, gets hi20; jalr keeps rd/rs1, gets lo12
    let new_auipc = (auipc_inst & 0xFFF) | (hi20 as u32);
    let new_jalr = (jalr_inst & 0xFFFFF) | ((lo12 as u32) << 20);
    
    auipc_addr.write_unaligned(new_auipc);
    jalr_addr.write_unaligned(new_jalr);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt_jit::builtins::BuiltinTable;

    #[test]
    fn context_create() {
        let table = BuiltinTable::new();
        let _ctx = JitEmitContext::new(&table);
    }
}
```

### 3. Add error variants to `lpvm-native/src/error.rs`

```rust
pub enum NativeError {
    // ... existing variants ...
    
    /// Symbol not found in builtin table during relocation
    UnresolvedSymbol(alloc::string::String),
    
    /// Invalid relocation (not auipc+jalr where expected)
    InvalidRelocation,
}
```

## Key Points

- Reuses existing `emit_function_bytes()` for code emission
- Records relocations that were already being collected
- `finalize()` patches auipc+jalr pairs following cranelift's RISC-V pattern
- Relocations are resolved using the `BuiltinTable`

## Validate

```bash
# Check compilation
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```

## Next Phase

Once JitEmitContext works, proceed to Phase 4: Engine/Module/Instance implementations.
