# M3: lpvm-cranelift JIT — Design

## Overview

Rename `lpir-cranelift` → `lpvm-cranelift` and add LPVM trait implementations
alongside the existing API. This validates the trait model against a native
backend and provides the production JIT path.

## Architecture

### Dual API Design

Both APIs coexist:

```
┌─────────────────────────────────────────────────────────────────┐
│                    lpvm-cranelift                               │
├─────────────────────────────────────────────────────────────────┤
│  Old API (stays until M7)    │  New trait API (M3 adds)         │
│                              │                                  │
│  JitModule                   │  CraneliftEngine (LpvmEngine)    │
│  DirectCall                  │  CraneliftModule (LpvmModule)    │
│  GlslQ32, CallResult         │  CraneliftInstance (LpvmInstance)│
│  jit(), jit_from_ir()        │                                  │
│  call_i32_buf()              │  direct_call() still available   │
└─────────────────────────────────────────────────────────────────┘
```

### New Types

**`CraneliftEngine`**
```rust
pub struct CraneliftEngine {
    options: CompileOptions,
    // Maybe preconfigured JITBuilder state
}

impl LpvmEngine for CraneliftEngine {
    type Module = CraneliftModule;
    type Error = CompileError;
    
    fn compile(&self, ir: &LPIR, meta: &ShaderMetadata) -> Result<Self::Module, Self::Error>;
}
```

**`CraneliftModule`**
```rust
pub struct CraneliftModule {
    // JITModule finalized, code pointers, metadata
    code_ptrs: HashMap<String, *const u8>,
    signatures: HashMap<String, FuncSignature>,
    metadata: ShaderMetadata,
}

impl LpvmModule for CraneliftModule {
    type Instance = CraneliftInstance;
    type Error = CallError;
    
    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;
    
    // Beyond trait: direct call for hot path
    fn direct_call(&self, name: &str) -> Option<DirectCall>;
}
```

**`CraneliftInstance`**
```rust
pub struct CraneliftInstance {
    memory: Vec<u8>,           // VMContext backing
    vmctx_ptr: *mut u8,        // Pointer passed to functions
    // Other instance state
}

impl LpvmInstance for CraneliftInstance {
    type Error = CallError;
    
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error>;
    fn set_fuel(&mut self, fuel: u64);
    fn get_fuel(&self) -> u64;
}
```

### Memory Model

No `LpvmMemory` trait. JIT memory is:
- `Vec<u8>` inside `CraneliftInstance`
- VMContext pointer points to start
- Layout: `[fuel: u64] [globals...] [uniforms...] [scratch...]`

### ISA Selection

Host vs embedded:

```rust
#[cfg(not(target_arch = "riscv32"))]
fn build_isa() -> isa::Builder {
    cranelift_native::builder().expect("native ISA")
}

#[cfg(target_arch = "riscv32")]
fn build_isa() -> isa::Builder {
    isa::lookup(TargetTriple::from_str("riscv32imac-unknown-none-elf").unwrap())
        .expect("riscv32 support")
}
```

## Implementation Phases

### Phase 1: Rename and setup
- Rename `lpir-cranelift` → `lpvm-cranelift` in workspace
- Update Cargo.toml, fix any path references
- Verify `cargo check` still works
- Move from `lp-shader/legacy/` to `lp-shader/` (top-level)

### Phase 2: Add CraneliftEngine
- Create `engine.rs` with `CraneliftEngine` struct
- Implement `LpvmEngine` trait
- Reuse existing `build_jit_module()` logic
- Add basic compile test

### Phase 3: Add CraneliftModule
- Create `module.rs` with `CraneliftModule` struct
- Implement `LpvmModule` trait
- Add `direct_call()` method returning `DirectCall`
- Reuse existing code pointer extraction

### Phase 4: Add CraneliftInstance
- Create `instance.rs` with `CraneliftInstance` struct
- Implement `LpvmInstance` trait
- Memory: `Vec<u8>` for VMContext backing
- Call via name lookup + existing invoke machinery
- Fuel: stored in VMContext

### Phase 5: Integration and tests
- Wire new types together
- Unit tests for trait API
- Verify `DirectCall` hot path still works
- Test both host and RISC-V targets compile

### Phase 6: Documentation
- Update crate README
- Document dual API
- Explain when to use which

## Dependencies

Same as existing `lpir-cranelift`:
- `cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`, `cranelift-module`
- `cranelift-native` (host only, behind `std`)
- `lps-builtins` (for builtin symbol resolution)
- `lpvm` (traits)
- `lpir` (LPIR types)
- `lp-frontend-types` (metadata)

## File Structure

```
lp-shader/lpvm-cranelift/
├── Cargo.toml
└── src/
    ├── lib.rs              # Re-exports both APIs
    ├── engine.rs           # NEW: CraneliftEngine (LpvmEngine)
    ├── module.rs           # NEW: CraneliftModule (LpvmModule)
    ├── instance.rs         # NEW: CraneliftInstance (LpvmInstance)
    ├── jit_module.rs       # EXISTING: JitModule (old API)
    ├── direct_call.rs      # EXISTING: DirectCall (old API)
    ├── call.rs             # EXISTING: call machinery
    ├── values.rs           # EXISTING: GlslQ32, CallResult
    ├── compile.rs          # EXISTING: JIT compilation
    ├── lower.rs            # EXISTING: LPIR lowering
    ├── options.rs          # EXISTING: CompileOptions
    ├── error.rs            # EXISTING: error types
    └── invoke.rs           # EXISTING: invoke helpers
```

## Migration Timeline

- **M3 (now):** Add traits alongside old API
- **M6:** `lp-engine` migrates to trait API
- **M7:** Remove old API, keep only traits

## Validation

```bash
# Host
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift

# Embedded (JIT is the product)
cargo check -p lpvm-cranelift --target riscv32imac-unknown-none-elf

# Firmware (uses old API for now)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

## Open Questions

1. Should `CraneliftModule` store the `JITModule` or just code pointers?
   - Suggestion: Just code pointers + metadata. `JITModule` is an implementation detail.

2. How to handle VMContext layout between host/embedded?
   - Suggestion: Same layout, different size maybe. Fixed structure.

3. Should we unify `CompileError` with trait's error type?
   - Suggestion: Yes, `CraneliftEngine::Error = CompileError`.
