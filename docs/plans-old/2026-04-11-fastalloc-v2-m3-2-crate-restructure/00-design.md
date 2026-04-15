# M3.2: Crate Restructure - Design

## Scope of Work

Restructure `lpvm-native` from the forked `lpvm-native` layout into a clean,
Cranelift-inspired architecture with:

- Central `compile.rs` orchestrator with `CompileSession` (module-level shared state)
- Per-function compilation with immediate temporary cleanup (bounded peak memory)
- Flat `rv32/` module (no `isa/` wrapper, no dead trait abstractions)
- Shared pipeline: both `rt_jit` and `rt_emu` call `compile::compile_module`
- New `link.rs` for relocation resolution and ELF generation
- Delete all old dead code (old emit.rs, regalloc/, IsaBackend traits)

## File Structure

```
lpvm-native/src/
├── lib.rs                      # UPDATE: new module declarations + re-exports
├── compile.rs                  # NEW: CompileSession, compile_function, compile_module
├── link.rs                     # NEW: link_jit(), link_elf() — relocation + ELF gen
│
├── lower.rs                    # UPDATE: lower_function takes &mut ModuleSymbols
├── emit.rs                     # NEW: shared emission orchestrator (VInst+Alloc → bytes+relocs)
├── peephole.rs                 # KEEP: VInst peephole optimizer
│
├── rv32/                       # ISA-specific (flattened from isa/rv32/)
│   ├── mod.rs                  # Re-exports
│   ├── abi.rs                  # func_abi_rv32, register constants, callee-saved
│   ├── alloc.rs                # Fastalloc (forward allocator, M4 replaces)
│   ├── encode.rs               # RV32 instruction encoding (pure math)
│   ├── gpr.rs                  # GPR constants
│   ├── inst.rs                 # PInst types (SymbolRef, PhysInst)
│   ├── rv32_emit.rs            # PhysEmitter: PInst → bytes
│   └── debug/                  # Disasm, PInst formatters
│       ├── mod.rs
│       ├── disasm.rs
│       └── pinst.rs
│
├── vinst.rs                    # KEEP: VInst, VReg, VRegSlice, SymbolId, ModuleSymbols
├── region.rs                   # KEEP: RegionTree
├── regset.rs                   # KEEP: RegSet bitset
├── config.rs                   # KEEP: MAX_VREGS, constants
├── error.rs                    # KEEP: LowerError, NativeError
├── native_options.rs           # KEEP: NativeCompileOptions
├── types.rs                    # KEEP: NativeType
│
├── abi/                        # KEEP: shared ABI framework
│   ├── mod.rs
│   ├── classify.rs
│   ├── frame.rs
│   ├── func_abi.rs
│   └── regset.rs
│
├── debug/                      # KEEP: VInst debug formatters
│   ├── mod.rs
│   └── vinst.rs
├── debug_asm.rs                # UPDATE: uses compile::compile_module
│
├── rt_jit/                     # UPDATE: simplified, calls compile + link
│   ├── mod.rs
│   ├── engine.rs
│   ├── compiler.rs             # SIMPLIFY: thin wrapper around compile + link
│   ├── module.rs
│   ├── instance.rs
│   ├── buffer.rs
│   ├── builtins.rs
│   ├── call.rs
│   └── host_memory.rs
│
└── rt_emu/                     # UPDATE: simplified, calls compile + link
    ├── mod.rs
    ├── engine.rs               # SIMPLIFY: calls compile + link
    ├── module.rs
    └── instance.rs
```

**Deleted:**
- `isa/mod.rs` — dead `IsaBackend` trait, `Rv32Backend`, `CodeBlob`
- `isa/rv32/emit.rs` — 1470-line monolith, replaced by `emit.rs` + `link.rs` + `compile.rs`
- `regalloc/` — old allocators (already deleted)

## Conceptual Architecture

```
             compile.rs (orchestrator)
                 │
    ┌────────────┼────────────────────┐
    │            │                    │
    │   CompileSession {              │
    │     symbols: ModuleSymbols,     │
    │     abi: ModuleAbi,             │
    │     float_mode, options         │
    │   }                             │
    │            │                    │
    │   for each function:            │
    │     lower → alloc → emit        │
    │     (temps freed per fn)        │
    │            │                    │
    │   → CompiledModule {            │
    │       functions: Vec<CF>,       │
    │       symbols                   │
    │     }                           │
    └─────────────────────────────────┘
                 │
         ┌───────┴───────┐
         │               │
    link::link_jit   link::link_elf
         │               │
    rt_jit/          rt_emu/
    (JitBuffer)      (ELF → lpvm_cranelift)
```

### Pipeline per function

```
LPIR  →  lower.rs  →  VInst[]  →  peephole  →  rv32/alloc.rs  →  emit.rs  →  bytes + relocs
         (shared)      + pool      (shared)     (ISA-specific)    (shared)
                       + regions
```

### Key Types

```rust
/// Module-level state shared across function compilations.
pub struct CompileSession {
    pub symbols: ModuleSymbols,
    pub abi: ModuleAbi,
    pub float_mode: FloatMode,
    pub options: NativeCompileOptions,
}

/// Output of one function's compilation.
pub struct CompiledFunction {
    pub name: String,
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    pub debug_lines: Vec<(u32, Option<u32>)>,
}

/// Output of a full module compilation.
pub struct CompiledModule {
    pub functions: Vec<CompiledFunction>,
    pub symbols: ModuleSymbols,
}
```

### Central Function

```rust
pub fn compile_function(
    session: &mut CompileSession,
    func: &IrFunction,
    ir: &LpirModule,
    fn_sig: &LpsFnSig,
) -> Result<CompiledFunction, NativeError> {
    let lowered = lower::lower_function(
        func, ir, &session.abi, session.float_mode, &mut session.symbols,
    )?;
    peephole::optimize(&mut lowered.vinsts);
    let func_abi = rv32::abi::func_abi_rv32(fn_sig, func.total_param_slots());
    let alloc = rv32::alloc::allocate(
        &lowered.vinsts, &lowered.vreg_pool, &func_abi,
    )?;
    let emitted = emit::emit_function(
        &lowered, &alloc, &func_abi, &session.symbols,
    )?;
    // lowered + alloc dropped here — memory freed
    Ok(emitted)
}
```

### ISA Separation

Lightweight `rv32/` namespace — no traits, no polymorphism. ISA-specific code
(encoding, GPR constants, ABI details, allocator, PInst emission) lives in
`rv32/`. Shared pipeline code (compile, lower, emit orchestration, VInst,
regions) at root. A future ISA would add `xtensa/` or `arm/` alongside `rv32/`.

### Memory Model

Per-function compilation with immediate cleanup:
```
compile_function(f0): lower(+4KB) → alloc(+1KB) → emit(+2KB) → free(-5KB)
compile_function(f1): lower(+4KB) → alloc(+1KB) → emit(+2KB) → free(-5KB)
compile_function(f2): lower(+3KB) → alloc(+1KB) → emit(+1KB) → free(-4KB)

Peak: ~7KB (one function's temporaries + accumulated code)
```
