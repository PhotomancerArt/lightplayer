# M1 Design: Core Infrastructure + Lpvm Traits

**Plan:** `lpvm-native-poc-stage-i`  
**Roadmap:** `docs/roadmaps/2026-04-07-lpvm-native-poc/m1-core-traits.md`  
**Design reference:** `docs/design/native/overview.md`

## Scope

Create the `lp-shader/lpvm-native` crate with crate structure, types, lowering to VInst, and trait stubs implementing `LpvmEngine`/`LpvmModule`/`LpvmInstance`. No instruction encoding or emission yet. The ABI layer (register roles, calling convention) is defined and validated with unit tests — this is critical after the previous backend attempt failed at the ABI/callconv layer.

**Explicitly out of scope:**
- RV32 instruction encoding and emission (M2)
- ELF emission and linking (M3)
- Actual execution or JIT buffer output (M3+)
- Linear scan allocator (post-M1)

## File structure

```
lp-shader/lpvm-native/
├── Cargo.toml
└── src/
    ├── lib.rs              # public API, NativeEngine, re-exports, module declarations
    ├── error.rs            # NativeError, LowerError, UnsupportedOp, EmitStub
    ├── types.rs            # NativeType (I32, F32, Ptr, I64Stub), PhysReg, RegisterClass
    ├── vinst.rs            # VInst enum: Add32, Mul32, Load32, Store32, Call, Ret, Label
    ├── lower.rs            # LPIR Op → VInst lowering
    │
    ├── regalloc/
    │   ├── mod.rs          # RegAlloc trait, Allocation, ClobberSet
    │   └── greedy.rs       # GreedyAlloc: round-robin x8-x31, caller-saved clobber tracking
    │
    ├── isa/
    │   ├── mod.rs          # IsaBackend trait, EmitError, CodeBlob stub
    │   └── rv32/
    │       ├── mod.rs      # ISA constant (XLEN=32), re-exports
    │       ├── abi.rs      # RV32 ILP32 shader subset: register roles, assign_args(), frame layout
    │       └── emit.rs     # stub: "M2: instruction encoding"
    │
    ├── module.rs           # NativeModule implements LpvmModule (compile/instantiate stubs)
    ├── instance.rs         # NativeInstance implements LpvmInstance (call/call_q32 stubs)
    └── engine.rs           # NativeEngine implements LpvmEngine, NativeCompileOptions
```

## Conceptual architecture

```
IrModule (from LPIR)
    │
    ▼
lower::lower_function() ──▶ Vec<VInst> + Result
    │                         Supports: iadd, isub, imul, iconst, return
    │                         Q32 fadd/fmul → VInst::Call { "fadd_q32", ... }
    │                         Unsupported → Err(UnsupportedOp)
    ▼
regalloc::GreedyAlloc.allocate()
    │                         Round-robin x8-x31
    │                         Tracks CALLER_SAVED clobbering for Call VInsts
    ▼
isa::rv32::abi::assign_args() ──▶ ArgAssignment { regs, stack_slots }
    │                         Validated with unit tests (no emission yet)
    ▼
isa::rv32::emit_function() ──▶ stub: panic!("M2: emit")
```

## Key components

### NativeType (`types.rs`)

Backend-specific type system separate from `IrType`:

```rust
pub enum NativeType {
    I32,
    F32,
    Ptr,
    I64Stub,  // unimplemented!() path for future 64-bit support
}

impl From<IrType> for NativeType { ... }
```

### VInst (`vinst.rs`)

Virtual instructions — post-lowering, pre-regalloc, pre-emission:

```rust
pub enum VInst {
    Add32 { dst: VReg, src1: VReg, src2: VReg },
    Mul32 { dst: VReg, src1: VReg, src2: VReg },
    Load32 { dst: VReg, base: VReg, offset: i32 },
    Store32 { src: VReg, base: VReg, offset: i32 },
    Call { target: SymbolRef, args: Vec<VReg>, rets: Vec<VReg> },
    Ret { vals: Vec<VReg> },
    Label(LabelId),
}
```

### Lowering (`lower.rs`)

Single `Op` → `Result<VInst, LowerError>`:

- `iadd`, `isub`, `imul`, `iconst.i32`, `return` → direct VInst
- Q32 mode `fadd`/`fsub`/`fmul` → `Call { "fadd_q32", ... }` VInst
- Everything else → `Err(UnsupportedOp)`

### RegAlloc (`regalloc/`)

**Trait:**

```rust
pub trait RegAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_info: &VRegInfo) -> Allocation;
}
```

**GreedyAlloc:** Round-robin through allocatable registers (x8-x31 on RV32). Marks `CALLER_SAVED` registers as clobbered when encountering `Call` VInsts.

**ABI integration:** `isa::rv32::abi::CALLER_SAVED` constant drives clobber tracking.

### RV32 ABI (`isa/rv32/abi.rs`)

Based on RV32 ILP32, stripped to shader needs:

```rust
// Register roles
pub const ARG_REGS: [PhysReg; 8] = [10, 11, 12, 13, 14, 15, 16, 17]; // a0-a7
pub const RET_REGS: [PhysReg; 2] = [10, 11]; // a0-a1
pub const CALLER_SAVED: &[PhysReg] = &[...]; // a0-a7, t0-t6
pub const CALLEE_SAVED: &[PhysReg] = &[...]; // s0-s11 (s0 = FP)
pub const FP: PhysReg = 8;  // s0
pub const RA: PhysReg = 1;
pub const SP: PhysReg = 2;

// Calling convention
pub struct ArgAssignment {
    pub regs: Vec<PhysReg>,  // Which arg regs to use
    pub stack_slots: u32,    // Overflow to stack (M1: error on >8 args)
}

pub fn assign_args(sig: &LpsFnSig) -> ArgAssignment;

// Frame layout (validated, not yet emitted)
pub struct FrameLayout {
    pub size: u32,
    pub saved_ra: bool,
    pub saved_s0: bool,
}

pub fn leaf_frame() -> FrameLayout;  // Minimal: no frame needed
```

**Unit tests prove:**
- 2-arg function → a0, a1
- Return value → a0
- Caller-saved list includes a0-a7, t0-t6
- Clobber tracking marks correct registers

### Traits (`engine.rs`, `module.rs`, `instance.rs`)

Mirror `CraneliftEngine`/`CraneliftModule`/`CraneliftInstance`:

```rust
pub struct NativeEngine { options: NativeCompileOptions }
impl LpvmEngine for NativeEngine {
    type Module = NativeModule;
    type Error = NativeError;
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        todo!("M2: lower, regalloc, emit")
    }
    fn memory(&self) -> &dyn LpvmMemory { ... }
}

pub struct NativeModule { ... }
impl LpvmModule for NativeModule { ... }

pub struct NativeInstance { ... }
impl LpvmInstance for NativeInstance { ... }
```

### NativeCompileOptions (`engine.rs`)

Backend-specific, not shared with Cranelift:

```rust
pub struct NativeCompileOptions {
    pub float_mode: FloatMode,  // Q32 vs F32
}
```

## Dependencies

- `lpir`: `IrModule`, `Op`, `IrFunction`, `FloatMode`
- `lpvm`: `LpvmEngine`, `LpvmModule`, `LpvmInstance`, `LpvmMemory`
- `lps-shared`: `LpsModuleSig`, `LpsFnSig`

## Validation

```bash
# Crate compiles
$ cargo check -p lpvm-native

# Unit tests pass (ABI assignment, lowering, regalloc clobber tracking)
$ cargo test -p lpvm-native --lib

# No ESP32 check yet — no upstream dependency until M3 integration
```

## References

- **QBE RV64 ABI:** https://github.com/michg/qbe_riscv32_64/blob/master/rv64/abi.c — structure pattern for `selcall`, `selret`, argument classification
- **RISC-V psABI:** https://riscv-non-isa.github.io/riscv-elf-psabi-doc/ — official spec (we use RV32 ILP32 subset)
