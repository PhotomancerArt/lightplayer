# Phase 7: Update `lib.rs` + `debug_asm.rs`

## Scope

Update `debug_asm.rs` to use the new `compile_module` pipeline. Clean up
`lib.rs` re-exports to reflect the new structure. Update any downstream
crate references if needed.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Rewrite `debug_asm.rs`

The old `compile_module_asm_text` called `emit_function_bytes` per function
(which had `debug_info: true` for line mapping). The new pipeline needs to
produce debug line info.

For now, `compile_function` sets `debug_lines: Vec::new()` (TODO from phase 3).
The disassembler can still work without line info — it just won't annotate LPIR
ops. Update `debug_asm.rs` to use `compile_module`:

```rust
pub fn compile_module_asm_text(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    float_mode: lpir::FloatMode,
    opts: DisasmOptions,
    _alloc_trace: bool,
) -> Result<String, NativeError> {
    let options = NativeCompileOptions {
        float_mode,
        alloc_trace: _alloc_trace,
        ..Default::default()
    };
    let compiled = crate::compile::compile_module(ir, sig, float_mode, options)?;

    let mut out = String::new();
    for (i, cf) in compiled.functions.iter().enumerate() {
        let func = &ir.functions[i];
        let table = LineTable::from_debug_lines(&cf.debug_lines);
        out.push_str(&disassemble_function(&cf.code, &table, func, opts));
        out.push('\n');
    }
    Ok(out)
}
```

Update imports to use `crate::rv32::debug::` instead of `crate::isa::rv32::debug::`.

### 2. Clean up `lib.rs`

Final `lib.rs` should look like:

```rust
#![no_std]

#[macro_use]
extern crate alloc;

pub mod abi;
pub mod compile;
pub mod config;
pub mod debug;
pub mod debug_asm;
pub mod emit;
pub mod error;
pub mod link;
pub mod lower;
pub mod native_options;
pub mod peephole;
pub mod region;
pub mod regset;
pub mod rv32;
pub mod types;
pub mod vinst;

#[cfg(feature = "emu")]
pub mod rt_emu;

#[cfg(target_arch = "riscv32")]
pub mod rt_jit;

// Primary API re-exports
pub use abi::ModuleAbi;
pub use compile::{
    CompileSession, CompiledFunction, CompiledModule, NativeReloc,
    compile_function, compile_module,
};
pub use debug_asm::compile_module_asm_text;
pub use error::{LowerError, NativeError};
pub use link::link_elf;
pub use lower::{LoopRegion, LoweredFunction, lower_lpir_op, lower_ops};
pub use native_options::NativeCompileOptions;
pub use types::NativeType;
pub use vinst::{
    IcmpCond, IrVReg, LabelId, ModuleSymbols, SRC_OP_NONE, SymbolId,
    VInst, VReg, VRegSlice, pack_src_op, unpack_src_op,
};

#[cfg(feature = "emu")]
pub use rt_emu::{NativeEmuEngine, NativeEmuInstance, NativeEmuModule};

#[cfg(target_arch = "riscv32")]
pub use rt_jit::{
    BuiltinTable, NativeJitDirectCall, NativeJitEngine,
    NativeJitInstance, NativeJitModule,
};
```

### 3. Check downstream usage

Grep for any crate that imports from `lpvm_native::isa::` and update:

```bash
rg 'lpvm_native::isa' --type rust
```

Key downstream crates to check:
- `lp-engine` (may import `NativeEmuEngine`, `NativeEmuModule`, etc.)
- `lp-cli` (may use `compile_module_asm_text`, shader-rv32 commands)
- `fw-esp32` (JIT engine)

Update any broken imports.

## Validate

```bash
cargo check -p lpvm-native
cargo check -p lpvm-native --features emu
cargo test -p lpvm-native -- debug_asm::tests
```

If downstream crates are affected:
```bash
cargo check -p lp-engine --features emu
cargo check -p lp-cli
```
