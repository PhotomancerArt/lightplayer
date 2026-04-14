# Phase 3: Cranelift Backends ModuleDebugInfo Population

## Scope

Update `lpvm-native` (Cranelift + native linking) and `lpvm-emu` (Cranelift + emulator) to populate `ModuleDebugInfo` with disassembly sections.

## Implementation Details

### 1. Update `lpvm-native/src/rt_emu/module.rs`

```rust
pub struct NativeEmuModule {
    pub(crate) ir: LpirModule,
    pub(crate) _elf: Vec<u8>,
    pub(crate) meta: LpsModuleSig,
    pub(crate) load: Arc<lp_riscv_elf::ElfLoadInfo>,
    pub(crate) arena: EmuSharedArena,
    pub(crate) options: NativeCompileOptions,
    /// Debug info - NEW.
    pub(crate) debug_info: ModuleDebugInfo,
}

impl LpvmModule for NativeEmuModule {
    type Instance = NativeEmuInstance;
    type Error = NativeError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.meta
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        // ... existing code ...
    }

    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        Some(&self.debug_info)
    }
}
```

### 2. Update `lpvm-native/src/rt_emu/engine.rs`

After compilation, extract per-function disassembly:

```rust
use crate::debug_asm::compile_function_asm_text; // NEW helper

fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
    // ... existing compilation ...
    
    let mut debug_info = ModuleDebugInfo::new();
    
    // Compile each function and extract disassembly
    for func in &ir.functions {
        let emitted = emit_function_bytes(...)?;
        let table = LineTable::from_debug_lines(&emitted.debug_lines);
        let disasm = disassemble_function(&emitted.code, &table, func, DisasmOptions::default());
        
        let mut sections = BTreeMap::new();
        sections.insert("disasm".into(), disasm);
        
        // Note: Cranelift doesn't have VInst or allocation traces, so no interleaved section
        
        let func_debug = FunctionDebugInfo::new(&func.name)
            .with_inst_count(emitted.code.len() / 4)
            .with_sections(sections);
            
        debug_info.add_function(func_debug);
    }
    
    Ok(NativeEmuModule {
        // ... existing fields ...
        debug_info,
    })
}
```

### 3. Update `lpvm-emu/src/module.rs` and `lpvm-emu/src/engine.rs`

Same pattern as `lpvm-native`:

```rust
// module.rs
pub struct EmuModule {
    pub(crate) ir: LpirModule,
    pub(crate) meta: LpsModuleSig,
    pub(crate) load: Arc<ElfLoadInfo>,
    pub(crate) options: CompileOptions,
    pub(crate) arena: EmuSharedArena,
    pub(crate) debug_info: ModuleDebugInfo,  // NEW
}

impl LpvmModule for EmuModule {
    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        Some(&self.debug_info)
    }
}
```

### 4. Create helper in `lpvm-native/src/debug_asm.rs`

Add a function that returns per-function disassembly:

```rust
/// Compile and return per-function disassembly text.
pub fn compile_function_asm_text(
    func: &IrFunction,
    ir: &LpirModule,
    module_abi: &ModuleAbi,
    fn_sig: &LpsFnSig,
    float_mode: lpir::FloatMode,
) -> Result<(String, usize), NativeError> {
    let emitted = emit_function_bytes(func, ir, module_abi, fn_sig, float_mode, true, false)?;
    let table = LineTable::from_debug_lines(&emitted.debug_lines);
    let disasm = disassemble_function(&emitted.code, &table, func, DisasmOptions::default());
    let inst_count = emitted.code.len() / 4;
    Ok((disasm, inst_count))
}
```

### 5. Handle Cranelift JIT (lpvm-cranelift)

The JIT module (`CraneliftModule`) doesn't produce disassembly easily. For now, return `None`:

```rust
// lpvm-cranelift/src/lpvm_instance.rs
impl LpvmModule for CraneliftModule {
    // ... existing ...
    
    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        None  // JIT doesn't support debug info
    }
}
```

This is fine - `shader-debug` will show:
```
--- disasm ---
(not available for jit backend)
```

## Code Organization

- Module structs go in `rt_emu/module.rs` and `rt_jit/module.rs`
- Engine compile methods populate debug info
- The `debug_asm.rs` file contains the disassembly formatting helpers
- Keep changes minimal - Cranelift already has working disassembly, just wire it up

## Notes

Cranelift doesn't have:
- VInst layer (goes LPIR → Cranelift IR)
- Allocation traces (handled internally by regalloc2)

So Cranelift backends only populate the `disasm` section. This is fine - the `shader-debug` output will show this clearly with a note that other sections are FA-specific.

## Validate

```bash
cargo check -p lpvm-native
cargo check -p lpvm-emu
cargo test -p lpvm-native
cargo test -p lpvm-emu
```
