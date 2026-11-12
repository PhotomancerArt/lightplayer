# M3: lpvm-cranelift JIT — Stage III Plan Notes

## Scope of Work

**UPDATED:** After discussion, M3 is about adding LPVM trait implementations to
the **existing** `lpir-cranelift` crate (to be renamed `lpvm-cranelift`), NOT
creating a new crate from scratch.

The crate will have BOTH:
- **Existing API:** `JitModule`, `DirectCall`, `GlslQ32`, `call()`, `direct_call()`
- **New trait impls:** `CraneliftEngine` (LpvmEngine), `CraneliftModule` (LpvmModule),
  `CraneliftInstance` (LpvmInstance)

**Work:**
1. Rename crate: `lpir-cranelift` → `lpvm-cranelift`
2. Add trait implementations alongside existing API
3. Unit tests for trait-based compilation/instantiation/calling
4. Verify both host and RISC-V targets compile

**What stays for later (M7 cleanup):**
- Old `call.rs`, `values.rs`, `GlslQ32`, `CallResult` remain
- `lp-engine` keeps using old API until M6

**Cranelift code structure:**
- Shared: LPIR → Cranelift IR lowering, ISA config, compile options, error types
- JIT path (M3): `cranelift-jit`, `JITModule`, VMContext, memory, calls
- RV32 emu path (M4): `cranelift-object`, ELF linking, emulator (stays behind `riscv32-emu` feature)

## Current State

### Existing `lpir-cranelift` (legacy crate at `lp-shader/legacy/lpir-cranelift/`)

- `JitModule` — wraps `cranelift_jit::JITModule` + shader metadata + code pointers
- `DirectCall` — raw function pointer + ABI metadata for hot-path calls
- `jit()`, `jit_from_ir()`, `jit_from_ir_owned()` — compilation entry points
- `CompileOptions`, `CompileError`, `CompilerError`, `CallError` — types
- LPIR → Cranelift IR lowering in internal modules
- `riscv32-emu` feature — RV32 object compilation, ELF linking, emulated calls
  (this is M4, separate feature from JIT)

### `lpvm` traits (from M1)

- `LpvmEngine` — compile LPIR to module
- `LpvmModule` — immutable compiled code + metadata
- `LpvmInstance` — VMContext + memory + callable interface
- `LpvmMemory` — NOT a trait. WASM is special. JIT uses concrete buffer in instance.

### `lpvm-wasm` (from M2)

- Successfully validated traits against WASM backend
- Browser (`rt_browser`) and wasmtime (`rt_wasmtime`) implementations
- Pattern for target-specific runtime modules works well

## Architecture Decision (CONFIRMED)

**Option B: Rename `lpir-cranelift` to `lpvm-cranelift`, dual API**

- Keep all existing compilation code in place
- Add new types implementing the LPVM traits alongside
- Feature gate stays (`riscv32-emu` for M4 emu path, JIT always present)
- Cleanup of old API happens in M7 when `lp-engine` migrates

### New types to add

- `CraneliftEngine` — implements `LpvmEngine` (compiles LPIR to module)
- `CraneliftModule` — implements `LpvmModule` (immutable, has code pointers + metadata)
- `CraneliftInstance` — implements `LpvmInstance` (VMContext, memory buffer, call interface)

### Old types remain (until M7)

- `JitModule` — the existing monolithic type
- `DirectCall` — raw function pointer wrapper
- `GlslQ32`, `CallResult` — value marshaling
- `jit()`, `jit_from_ir()` — existing entry points

### Dual API pattern

```rust
// Old API (stays for compatibility until M7)
let module = jit(source, &opts)?;  // JitModule
let dc = module.direct_call("render")?;
dc.call_i32_buf(vmctx, args, ret_buf);

// New trait API (M3 adds this)
let engine = CraneliftEngine::new(opts);
let module = engine.compile(&ir, &meta)?;  // LpvmModule
let mut inst = module.instantiate()?;      // LpvmInstance
let val = inst.call("render", &[LpsValue::F32(1.0)])?;
```

## Design Questions (ANSWERED)

### Q1: How to handle the hot-path `DirectCall` in the trait model?

**Answer:** `DirectCall` stays as a separate method on `CraneliftModule`.
The trait interface is ergonomic general use. The old API gives direct control.
Engine chooses which to use. No trait change needed.

### Q2: What should memory look like for JIT?

**Answer:** No `LpvmMemory` trait. Concrete `Vec<u8>` buffer inside
`CraneliftInstance`. VMContext pointer points to it. No polymorphism needed.

### Q3: Which files to work with?

**Answer:** Work in existing crate at `lp-shader/legacy/lpir-cranelift/`,
add new files for trait implementations. Rename crate to `lpvm-cranelift`.

### Q4: How to structure target-specific code?

**Answer:** Same pattern as `lpvm-wasm`:

```rust
#[cfg(not(target_arch = "riscv32"))]
pub mod rt_native;  // uses cranelift-native

#[cfg(target_arch = "riscv32")]
pub mod rt_riscv32; // hardcoded triple
```

Or simpler: cfg-gate the ISA setup in engine impl and have single module.rs,
instance.rs.

### Q5: Test strategy?

**Answer:** Unit tests in the crate:

- Compile simple LPIR (int add, float add in Q32 and F32)
- Instantiate with memory
- Call via `LpvmInstance::call()`
- Call via `DirectCall` hot path
- Verify VMContext fuel stub works

Host tests only (cranelift-native). RISC-V target checked with `cargo check`.

## Design Decisions

1. **No re-export:** New trait types are fresh implementations alongside old API
2. **Memory:** Owned `Vec<u8>` in `CraneliftInstance`, exposed via VMContext pointer
3. **DirectCall:** Accessible through `module.direct_call()` (separate method on `CraneliftModule`)
4. **Target cfg:** Use `#[cfg(target_arch)]` for ISA selection:
   - Host: `cranelift-native` for ISA detection
   - Embedded: hardcoded `riscv32imac` triple
