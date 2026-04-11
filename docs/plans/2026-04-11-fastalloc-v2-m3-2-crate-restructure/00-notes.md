# M3.2: Crate Restructure - Notes

## Scope of Work

Restructure `lpvm-native-fa` into a clean Cranelift-inspired architecture:

1. **Flatten `isa/rv32/`** — one ISA, hierarchy adds nothing
2. **Add `compile.rs`** — module-level orchestrator, `CompileSession`, per-function compile
3. **Add `link.rs`** — relocation resolution, ELF generation (extracted from `emit.rs`)
4. **Shared pipeline** — both `rt_jit` and `rt_emu` call `compile::compile_module`
5. **Module-level `CompileSession`** — shared `ModuleSymbols`, ABI, options
6. **Per-function compile with cleanup** — temporaries freed after each function
7. **Pluggable emission** — `EmitMode::Full` (PInst) vs `EmitMode::Direct` (bytes)
8. **Delete dead abstractions** — `IsaBackend` trait, `Rv32Backend`, `CodeBlob`

## Current State

### Crate structure (47 files)
```
lpvm-native-fa/src/
├── abi/                    # ABI types (classify, frame, func_abi, regset)
├── config.rs               # Constants (MAX_VREGS)
├── debug/                  # VInst debug formatters
├── debug_asm.rs            # compile_module_asm_text (host debug)
├── error.rs                # LowerError, NativeError
├── isa/
│   ├── mod.rs              # IsaBackend trait, Rv32Backend, CodeBlob (dead code)
│   └── rv32/
│       ├── abi.rs          # func_abi_rv32, arg register consts
│       ├── alloc.rs        # Forward allocator (fastalloc stage I)
│       ├── debug/          # Disasm, PInst debug formatters
│       ├── emit.rs         # 1470 lines: lower → regalloc → emit + ELF
│       ├── encode.rs       # RV32 instruction encoding (pure math)
│       ├── gpr.rs          # GPR constants
│       ├── inst.rs         # SymbolRef, PhysInst types (old)
│       ├── mod.rs          # emit_function_fastalloc_bytes
│       └── phys_emit.rs    # PhysEmitter (PInst → bytes)
├── lib.rs
├── lower.rs                # LPIR → VInst lowering (1597 lines)
├── native_options.rs       # NativeCompileOptions
├── peephole.rs             # VInst peephole optimizer
├── regalloc/
│   ├── greedy.rs           # Old greedy allocator
│   ├── linear_scan.rs      # Old linear scan allocator
│   └── mod.rs              # RegAlloc trait, Allocation, PReg
├── region.rs               # RegionTree (arena-based)
├── regset.rs               # RegSet bitset
├── rt_emu/                 # Emu runtime (ELF path)
├── rt_jit/                 # JIT runtime (raw bytes path)
├── types.rs                # NativeType enum
└── vinst.rs                # VInst, VReg, VRegSlice, ModuleSymbols
```

### Key problems
1. `isa/rv32/emit.rs` is 1470 lines: orchestration + emission + ELF generation + tests
2. `rt_jit/compiler.rs` duplicates pipeline logic (lower → emit per function)
3. Both runtimes invoke the pipeline differently
4. `ModuleSymbols` is per-function (should be module-level in CompileSession)
5. Dead abstractions: `IsaBackend`, `Rv32Backend`, `CodeBlob` (never used polymorphically)
6. `isa/rv32/` nesting with only one ISA

### Proposed architecture (from prior discussion)

**Target structure:**
```
src/
├── compile.rs              # NEW: CompileSession, compile_function, compile_module
├── lower.rs                # LPIR → VInst (exists, takes &mut CompileSession.symbols)
├── alloc.rs                # VInst → Allocation (M4/M5 allocator, moved from isa/rv32/alloc.rs)
├── emit.rs                 # VInst + Allocation → bytes + relocs (extracted from isa/rv32/emit.rs)
├── encode.rs               # RV32 instruction encoding (moved from isa/rv32/encode.rs)
├── link.rs                 # NEW: Relocation resolution, ELF generation
│
├── abi.rs                  # func_abi_rv32 (merged from isa/rv32/abi.rs + abi/)
├── gpr.rs                  # GPR constants (moved from isa/rv32/gpr.rs)
├── pinst.rs                # PInst enum (moved from isa/rv32/inst.rs)
├── phys_emit.rs            # PhysEmitter (moved from isa/rv32/phys_emit.rs)
│
├── vinst.rs                # VInst, VReg, VRegSlice, SymbolId
├── region.rs               # RegionTree
├── regset.rs               # RegSet bitset
├── config.rs               # Constants
├── error.rs                # Errors
├── native_options.rs       # Options
├── peephole.rs             # Peephole optimizer
├── types.rs                # NativeType
│
├── rt_jit/                 # Simplified: calls compile::compile_module + link::link_jit
├── rt_emu/                 # Simplified: calls compile::compile_module + link::link_elf
│
├── debug/                  # Debug formatters
├── debug_asm.rs            # Host debug
└── lib.rs
```

**Key new type:**
```rust
pub struct CompileSession {
    pub symbols: ModuleSymbols,
    pub abi: ModuleAbi,
    pub float_mode: FloatMode,
    pub options: NativeCompileOptions,
}
```

**Central function:**
```rust
pub fn compile_function(
    session: &mut CompileSession,
    func: &IrFunction,
    ir: &IrModule,
    fn_sig: &LpsFnSig,
) -> Result<CompiledFunction, NativeError> {
    let lowered = lower::lower_function(func, ir, &session.abi, session.float_mode, &mut session.symbols)?;
    let func_abi = abi::func_abi_rv32(fn_sig, func.total_param_slots());
    let allocation = alloc::allocate(&lowered.vinsts, &lowered.vreg_pool, &func_abi, &lowered.region_tree)?;
    let emitted = emit::emit_function(&lowered, &allocation, &func_abi, &session.symbols)?;
    Ok(emitted) // lowered + allocation dropped here
}
```

## Questions

### Q1: Should the old allocators (greedy, linear_scan) survive the restructure?

**Answer:** No. Already deleted. The old allocators live in `lpvm-native`.
This crate (`lpvm-native-fa`) is a clean slate — only the fastalloc in
`isa/rv32/alloc.rs` remains. `regalloc/` directory is gone.

### Q2: Should we merge `abi/` directory and `isa/rv32/abi.rs`?

**Answer:** Keep `abi/` directory for shared ABI framework (`ModuleAbi`,
`FrameLayout`, `FuncAbi`, `PregSet`). Move `isa/rv32/abi.rs` to `rv32/abi.rs`
(ISA-specific: `func_abi_rv32`, register constants, callee-saved sets).

Lightweight ISA separation: `rv32/` module at root (no `isa/` wrapper, no
traits). ISA-specific code (encode, gpr, abi, alloc, phys_emit) in `rv32/`.
Shared pipeline (compile, lower, emit, vinst, region) at root. If a second
ISA is added later, create `xtensa/` or `arm/` alongside `rv32/`.

### Q3: Should `ModuleSymbols` move from `vinst.rs` to `compile.rs`?

**Answer:** Keep definition in `vinst.rs` (tightly coupled to `SymbolId` and
`VInst::Call`). `CompileSession` in `compile.rs` owns an instance.

### Q4: What about `phys_emit.rs` — merge into `emit.rs` or keep separate?

**Answer:** Keep separate. Renamed to `rv32_emit.rs` in `rv32/` for clarity.
`emit.rs` at root orchestrates; `rv32/rv32_emit.rs` does ISA-specific PInst →
bytes encoding.

### Q5: How should we handle the two parallel pipelines during the restructure?

**Context:** The old pipeline (`emit_function_bytes` + greedy/linear_scan) and
the new pipeline (`compile_function` + fastalloc) both need to work during M3.2.
Tests use the old pipeline. The new pipeline is the target.

**Suggested answer:** Phase the work:
1. First flatten files (move, no behavior change, tests pass)
2. Then create `compile.rs` calling into the *existing* logic
3. Then simplify runtimes to use `compile.rs`
4. Old paths become dead code, deleted last

### Q6: What types should `compile_function` return?

**Context:** Need to define `CompiledFunction` and `CompiledModule`.

**Suggested answer:**
```rust
pub struct CompiledFunction {
    pub name: String,
    pub code: Vec<u8>,
    pub relocs: Vec<NativeReloc>,
    pub debug_lines: Vec<(u32, Option<u32>)>,
}

pub struct CompiledModule {
    pub functions: Vec<CompiledFunction>,
    pub symbols: ModuleSymbols,
}
```

## Notes

(filled in as questions are answered)
