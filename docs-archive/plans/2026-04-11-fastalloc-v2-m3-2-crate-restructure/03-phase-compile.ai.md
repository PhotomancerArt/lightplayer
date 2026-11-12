# Phase 3: Create `compile.rs`

## Scope

Create the central compilation orchestrator: `CompileSession`,
`compile_function`, `compile_module`, and the output types `CompiledFunction`
and `CompiledModule`. Also define `NativeReloc` here (moved from old emit.rs).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Create `src/compile.rs`

```rust
//! Module compilation orchestrator.
//!
//! [`CompileSession`] holds module-level shared state (symbols, ABI, options).
//! [`compile_function`] compiles one LPIR function through the full pipeline
//! (lower → peephole → alloc → emit), freeing temporaries on return.
//! [`compile_module`] compiles all functions in a module.

use alloc::string::String;
use alloc::vec::Vec;

use lpir::{FloatMode, IrFunction, LpirModule};
use lps_shared::{LpsFnSig, LpsModuleSig};

use crate::abi::ModuleAbi;
use crate::error::NativeError;
use crate::native_options::NativeCompileOptions;
use crate::vinst::ModuleSymbols;

/// Byte offset in `.text` where a relocation applies (auipc+jalr pair).
#[derive(Clone, Debug)]
pub struct NativeReloc {
    pub offset: usize,
    pub symbol: String,
}

/// Output of compiling one function.
#[derive(Debug)]
pub struct CompiledFunction {
    pub name: String,
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    pub debug_lines: Vec<(u32, Option<u32>)>,
}

/// Output of compiling a full module.
#[derive(Debug)]
pub struct CompiledModule {
    pub functions: Vec<CompiledFunction>,
    pub symbols: ModuleSymbols,
}

/// Module-level state shared across function compilations.
pub struct CompileSession {
    pub symbols: ModuleSymbols,
    pub abi: ModuleAbi,
    pub float_mode: FloatMode,
    pub options: NativeCompileOptions,
}

impl CompileSession {
    pub fn new(
        ir: &LpirModule,
        meta: &LpsModuleSig,
        float_mode: FloatMode,
        options: NativeCompileOptions,
    ) -> Self {
        Self {
            symbols: ModuleSymbols::default(),
            abi: ModuleAbi::from_ir_and_sig(ir, meta),
            float_mode,
            options,
        }
    }
}

/// Compile one LPIR function. Temporaries freed on return.
pub fn compile_function(
    session: &mut CompileSession,
    func: &IrFunction,
    ir: &LpirModule,
    fn_sig: &LpsFnSig,
) -> Result<CompiledFunction, NativeError> {
    // 1. Lower LPIR → VInst
    let mut lowered = crate::lower::lower_ops_with_symbols(
        func,
        ir,
        &session.abi,
        session.float_mode,
        &mut session.symbols,
    )?;

    // 2. Peephole optimize
    crate::peephole::optimize(&mut lowered.vinsts);

    // 3. Allocate registers (fastalloc)
    let func_abi = crate::rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots() as usize);
    let pinsts = crate::rv32::alloc::allocate(
        &lowered.vinsts,
        &func_abi,
        func,
        &lowered.vreg_pool,
    ).map_err(NativeError::FastAlloc)?;

    // 4. Emit PInst → bytes
    let mut emitter = crate::rv32::rv32_emit::Rv32Emitter::new();
    for p in &pinsts {
        emitter.emit(p);
    }
    let (code, phys_relocs) = emitter.finish_with_relocs();

    let relocs = phys_relocs
        .into_iter()
        .map(|r| NativeReloc {
            offset: r.offset,
            symbol: r.symbol,
        })
        .collect();

    // lowered + pinsts + func_abi dropped here
    Ok(CompiledFunction {
        name: func.name.clone(),
        code,
        relocs,
        debug_lines: Vec::new(), // TODO: wire up debug_lines from lowered.vinsts src_op mapping
    })
}

/// Compile all functions in a module.
pub fn compile_module(
    ir: &LpirModule,
    meta: &LpsModuleSig,
    float_mode: FloatMode,
    options: NativeCompileOptions,
) -> Result<CompiledModule, NativeError> {
    if ir.functions.is_empty() {
        return Err(NativeError::EmptyModule);
    }

    let mut session = CompileSession::new(ir, meta, float_mode, options);
    let sig_map: alloc::collections::BTreeMap<&str, &LpsFnSig> =
        meta.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut functions = Vec::new();
    for func in &ir.functions {
        let default_sig = LpsFnSig {
            name: func.name.clone(),
            return_type: lps_shared::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);
        let compiled = compile_function(&mut session, func, ir, fn_sig)?;
        functions.push(compiled);
    }

    Ok(CompiledModule {
        functions,
        symbols: session.symbols,
    })
}
```

### 2. Update `lower.rs` — add `lower_ops_with_symbols`

The existing `lower_ops` creates its own `ModuleSymbols` per function. Add a
variant that accepts `&mut ModuleSymbols` from the `CompileSession`:

```rust
/// Lower with external (module-level) symbols table.
pub fn lower_ops_with_symbols(
    func: &IrFunction,
    ir: &LpirModule,
    module_abi: &ModuleAbi,
    float_mode: FloatMode,
    symbols: &mut ModuleSymbols,
) -> Result<LoweredFunction, LowerError> {
    // Same as lower_ops but uses the provided symbols instead of creating a new one
    // ...
}
```

The existing `lower_ops` can delegate to `lower_ops_with_symbols` for backward
compatibility:

```rust
pub fn lower_ops(...) -> Result<LoweredFunction, LowerError> {
    let mut symbols = ModuleSymbols::default();
    lower_ops_with_symbols(func, ir, module_abi, float_mode, &mut symbols)
}
```

### 3. Update `rv32/mod.rs`

Replace the stubbed `emit_function_fastalloc_bytes` with a real implementation
that delegates to `compile_function`:

```rust
pub fn emit_function_fastalloc_bytes(
    func: &IrFunction,
    ir: &LpirModule,
    module_abi: &ModuleAbi,
    fn_sig: &LpsFnSig,
    float_mode: FloatMode,
) -> Result<Vec<u8>, NativeError> {
    let options = crate::native_options::NativeCompileOptions::default();
    let meta = lps_shared::LpsModuleSig {
        functions: alloc::vec![fn_sig.clone()],
    };
    let mut session = crate::compile::CompileSession::new(ir, &meta, float_mode, options);
    let compiled = crate::compile::compile_function(&mut session, func, ir, fn_sig)?;
    Ok(compiled.code)
}
```

### 4. Update `lib.rs`

Add:
```rust
pub mod compile;
```

Add re-exports:
```rust
pub use compile::{CompileSession, CompiledFunction, CompiledModule, NativeReloc, compile_function, compile_module};
```

### 5. Expose `rv32_emit` module publicly

In `rv32/mod.rs`, make sure `rv32_emit` is accessible. Currently it's included
via `#[path]` in the old `emit.rs`. Since we deleted that, declare it directly:

```rust
pub mod rv32_emit;
```

This requires the file is named `rv32_emit.rs` in the `rv32/` directory (it is).

## Validate

```bash
cargo check -p lpvm-native
```

The new `compile_function` and `compile_module` should compile. The
`emit_function_fastalloc_bytes` wrapper should compile. The stubs in `debug_asm`,
`rt_jit/compiler`, `rt_emu/engine` are still in place (fixed in later phases).

### Test

```bash
cargo test -p lpvm-native -- rv32::alloc::tests
```

The fastalloc unit tests should still pass (they don't depend on the old
pipeline).
