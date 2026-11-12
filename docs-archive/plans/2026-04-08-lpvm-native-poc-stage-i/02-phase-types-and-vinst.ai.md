## Phase 2: Types and VInst

### Scope

Define `NativeType`, `PhysReg`, `VInst`, and supporting types. These are pure data definitions — no logic beyond `From` impls and basic constructors.

### Code organization

Entry points (type definitions, `From` traits) at top. Helper impls (`Display`, etc.) at bottom.

### Implementation details

**`types.rs`:**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeType {
    I32,
    F32,
    Ptr,
    I64Stub,
}

pub type PhysReg = u8;

pub enum RegisterClass {
    Int,
    Float,  // for future F32 hardware mode
}

impl From<IrType> for NativeType {
    fn from(t: IrType) -> Self {
        match t {
            IrType::I32 => NativeType::I32,
            IrType::F32 => NativeType::F32,
            IrType::Pointer => NativeType::Ptr,
        }
    }
}
```

**`vinst.rs`:**

```rust
pub type VReg = lpir::types::VReg;
pub type LabelId = u32;

#[derive(Clone, Debug)]
pub enum VInst {
    // ALU
    Add32 { dst: VReg, src1: VReg, src2: VReg },
    Sub32 { dst: VReg, src1: VReg, src2: VReg },
    Mul32 { dst: VReg, src1: VReg, src2: VReg },
    
    // Memory
    Load32 { dst: VReg, base: VReg, offset: i32 },
    Store32 { src: VReg, base: VReg, offset: i32 },
    
    // Control
    Call { target: SymbolRef, args: Vec<VReg>, rets: Vec<VReg> },
    Ret { vals: Vec<VReg> },
    Label(LabelId),
    
    // Constants
    IConst32 { dst: VReg, val: i32 },
}

#[derive(Clone, Debug)]
pub struct SymbolRef {
    pub name: alloc::string::String,
}
```

**`error.rs`:**

```rust
#[derive(Debug)]
pub enum LowerError {
    UnsupportedOp { op: Op },
    TypeMismatch { expected: NativeType, got: IrType },
}

#[derive(Debug)]
pub struct EmitStub;
```

### Tests

Unit tests for `From<IrType>` conversion, basic VInst construction.

```bash
cargo test -p lpvm-native --lib types
```
