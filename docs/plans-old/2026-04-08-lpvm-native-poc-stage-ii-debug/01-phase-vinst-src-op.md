# Phase 1: VInst Source Operation Tracking

## Scope

Add `src_op: Option<u32>` field to all `VInst` variants to track which LPIR operation generated each virtual instruction.

## Code Organization Reminders

- Place `src_op` as the last field in each variant for consistency
- Use `Option<u32>` so we can disable tracking by setting to `None`
- Update pattern matches to handle the new field
- Helper functions go at the bottom of the module

## Implementation Details

### Update `vinst.rs`

Add `src_op` to each VInst variant:

```rust
pub enum VInst {
    Add32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,  // NEW
    },
    Sub32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,  // NEW
    },
    Mul32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: Option<u32>,  // NEW
    },
    Const32 {
        dst: VReg,
        value: i32,
        src_op: Option<u32>,  // NEW
    },
    Load {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: Option<u32>,  // NEW
    },
    Store {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: Option<u32>,  // NEW
    },
    Call {
        target: String,
        args: Vec<VReg>,
        rets: Vec<VReg>,
        src_op: Option<u32>,  // NEW
    },
    Ret {
        values: Vec<VReg>,
        src_op: Option<u32>,  // NEW
    },
    Nop {
        src_op: Option<u32>,  // NEW
    },
}
```

### Update constructor helpers

Add `with_src_op()` constructors or update existing ones to take `src_op` parameter.

Simplest approach: add a `src_op: Option<u32>` parameter to all existing constructors.

### Update pattern matches

Anywhere we match on VInst (in `lower.rs`, `regalloc/greedy.rs`, `isa/rv32/emit.rs`), update to handle the new field.

For example, in `emit_vinst`:

```rust
match inst {
    VInst::Add32 { dst, src1, src2, src_op: _ } => { ... }
    // ...
}
```

### Future convenience method

At the bottom of the module, add:

```rust
impl VInst {
    /// Get the source LPIR operation index, if tracked
    pub fn src_op(&self) -> Option<u32> {
        match self {
            VInst::Add32 { src_op, .. } => *src_op,
            VInst::Sub32 { src_op, .. } => *src_op,
            // ... all variants
            VInst::Nop { src_op } => *src_op,
        }
    }
}
```

## Tests

```rust
#[test]
fn vinst_src_op_roundtrip() {
    let inst = VInst::Add32 {
        dst: VReg(1),
        src1: VReg(2),
        src2: VReg(3),
        src_op: Some(5),
    };
    assert_eq!(inst.src_op(), Some(5));
}
```

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib vinst_src_op_roundtrip
# No-std check (src_op field has no std dependency)
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```
