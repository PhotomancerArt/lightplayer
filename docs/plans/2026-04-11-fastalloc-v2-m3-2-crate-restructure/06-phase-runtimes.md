# Phase 6: Rewire runtimes

## Scope

Simplify `rt_jit` and `rt_emu` to use the new `compile::compile_module` +
`link` pipeline. Remove the old `JitEmitContext` and `compile_module_jit` from
`rt_jit/compiler.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Rewrite `rt_emu/engine.rs`

Replace the stub with a call to `compile_module` + `link_elf`:

```rust
impl LpvmEngine for NativeEmuEngine {
    type Module = NativeEmuModule;
    type Error = NativeError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let compiled = crate::compile::compile_module(
            ir,
            meta,
            self.options.float_mode,
            self.options,
        )?;

        let entry_flags: Vec<bool> = ir.functions.iter().map(|f| f.is_entry).collect();
        let elf = crate::link::link_elf(&compiled, &entry_flags)?;

        let load = Arc::new(lpvm_cranelift::link_object_with_builtins(&elf)?);
        Ok(NativeEmuModule {
            ir: ir.clone(),
            _elf: elf,
            meta: meta.clone(),
            load,
            arena: self.arena.clone(),
            options: self.options,
        })
    }
    // ...
}
```

Remove the `use crate::isa::rv32::emit::emit_module_elf;` import.

### 2. Rewrite `rt_jit/compiler.rs`

Replace `JitEmitContext` and `compile_module_jit` with a thin wrapper:

```rust
//! JIT compilation: compile module + link for direct execution.

use alloc::collections::BTreeMap;
use alloc::string::String;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::error::NativeError;
use crate::native_options::NativeCompileOptions;

use super::buffer::JitBuffer;
use super::builtins::BuiltinTable;

/// Compile and link a full module for JIT execution.
pub fn compile_module_jit(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    builtin_table: &BuiltinTable,
    float_mode: lpir::FloatMode,
    options: NativeCompileOptions,
) -> Result<(JitBuffer, BTreeMap<String, usize>), NativeError> {
    let compiled = crate::compile::compile_module(ir, sig, float_mode, options)?;
    crate::link::link_jit(&compiled, builtin_table)
}
```

Note the signature change: `alloc_trace: bool` is now part of
`NativeCompileOptions`. Update the caller in `rt_jit/engine.rs` accordingly.

### 3. Update `rt_jit/engine.rs`

The call to `compile_module_jit` needs to pass `self.options` instead of
individual fields:

```rust
let (buffer, entry_offsets) = compile_module_jit(
    ir,
    meta,
    &self.builtin_table,
    self.options.float_mode,
    self.options,
)?;
```

### 4. Clean up imports

Remove all references to deleted `crate::isa::rv32::emit::*` from both
runtime modules.

## Validate

```bash
cargo check -p lpvm-native-fa
cargo check -p lpvm-native-fa --features emu
# JIT check (riscv32 target):
# cargo check -p lpvm-native-fa --target riscv32imac-unknown-none-elf
```
