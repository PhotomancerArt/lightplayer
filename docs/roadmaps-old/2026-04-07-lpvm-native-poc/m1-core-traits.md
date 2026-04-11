# Milestone 1: Core Infrastructure + Lpvm Traits

**Goal**: Crate structure, types, lowering, and trait stubs. Internal pipeline compiles (no emission yet).

## Suggested Plan Name

`lpvm-native-m1`

## Scope

### In Scope

- `lp-shader/lpvm-native/` crate structure
- `VInst` enum: target-independent virtual instructions (32-bit ops, 64-bit stub variants)
- Extended `IrType` (I32, I64 stub, F32, Ptr)
- LPIR → VInst lowering for integer ALU ops (`iadd`, `isub`, `imul` via builtins)
- Greedy register allocator: interface + skeleton implementation
- **Stub `LpvmEngine`, `NativeModule`, `NativeInstance` trait implementations**
- `CompileOptions` integration

### Explicitly Out of Scope

- No instruction encoding (no RV32 bytes yet)
- No ELF emission
- No actual register assignment (just interface)
- No linking/execution

## Key Decisions

### VInst Design

Virtual instructions are post-lowering, pre-register-allocation. They know about virtual registers but not physical ones.

```rust
pub enum VInst {
    Add32 { dst: VReg, src1: VReg, src2: VReg },
    Load32 { dst: VReg, base: VReg, offset: i32 },
    Store32 { src: VReg, base: VReg, offset: i32 },
    CallBuiltin { name: &'static str, args: Vec<VReg>, rets: Vec<VReg> },
    Ret,
    // 64-bit stubs
    Load64 { dst: VReg, base: VReg, offset: i32 }, // unimplemented!()
}
```

### RegAlloc Interface

Design for upgradeability: trait-based so we can swap greedy → linear scan later.

```rust
pub trait RegAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_types: &[IrType]) -> AllocationResult;
}

pub struct GreedyAlloc;
impl RegAlloc for GreedyAlloc {
    // Round-robin through x8-x31
}
```

### Lpvm Trait Stubs

Mirror `CraneliftEngine`/`CraneliftModule`/`CraneliftInstance` structure.

```rust
pub struct NativeEngine;
impl LpvmEngine for NativeEngine {
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Arc<dyn LpvmModule>, Error> {
        todo!("M3: lower, regalloc, emit, link")
    }
}

pub struct NativeModule {
    code: Vec<u8>,  // Populated in M3
    meta: LpsModuleSig,
    // ...
}
impl LpvmModule for NativeModule {
    fn instantiate(&self) -> Result<Box<dyn LpvmInstance>, Error> {
        todo!("M3")
    }
}
```

## Deliverables


| File                     | Contents                                                    |
| ------------------------ | ----------------------------------------------------------- |
| `Cargo.toml`             | Crate manifest, dependencies (`lpir`, `lpvm`, `lps-shared`) |
| `src/lib.rs`             | Public API, re-exports, `LpvmEngine` stub                   |
| `src/types.rs`           | Extended `IrType` enum                                      |
| `src/vinst.rs`           | `VInst` definitions                                         |
| `src/lower.rs`           | `lower_lpiir()` function: LPIR ops → VInst sequence         |
| `src/regalloc/mod.rs`    | `RegAlloc` trait, `AllocationResult` struct                 |
| `src/regalloc/greedy.rs` | `GreedyAlloc` skeleton                                      |
| `src/module.rs`          | `NativeModule` struct, `LpvmModule` stub impl               |
| `src/instance.rs`        | `NativeInstance` struct, `LpvmInstance` stub impl           |
| `src/isa/mod.rs`         | `IsaBackend` trait (target abstraction)                     |
| `src/isa/rv32/mod.rs`    | RV32 backend stub                                           |


## Dependencies

- `lpir` crate (for `IrModule`, `Op`, `IrFunction`)
- `lpvm` crate (for `LpvmEngine`, `LpvmModule`, `LpvmInstance` traits)
- `lps-shared` (for `LpsModuleSig`, `CompileOptions`)

## Estimated Scope

- ~800 lines
- 2-3 days
- Complexity: mostly boilerplate and enum definitions

## Validation

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib  # type checking, trait implementations compile
```

No functional tests yet - just compilation.