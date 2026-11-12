# Phase 2: Update VInst::Call with sret Flag

## Scope of Phase

Update `VInst::Call` to include a `callee_uses_sret` flag. This flag tells the emission code whether the callee uses sret (and thus the caller needs caller-side sret handling).

## Code Organization Reminders

- Keep the VInst enum variants ordered logically (group by category)
- Update `src_op()`, `defs()`, `uses()`, and `is_call()` match arms for the new field
- Keep `Call` near other control-flow instructions (Br, BrIf, Ret, Label)

## Implementation Details

### File: `lp-shader/lpvm-native/src/vinst.rs`

Update `VInst::Call` variant:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VInst {
    // ... arithmetic ops above ...
    
    /// Function call: auipc+jalr with relocation.
    /// When `callee_uses_sret` is true, emission uses caller-side sret:
    ///   - Pass sret buffer address in a0 (user args shift to a1-a7)
    ///   - Load return values from buffer after call
    /// When false, emission uses direct return (results in a0-a1).
    Call {
        target: SymbolRef,
        args: Vec<VReg>,
        rets: Vec<VReg>,
        callee_uses_sret: bool,  // NEW
        src_op: Option<u32>,
    },
    
    /// Return: move vals to return registers or sret buffer.
    Ret {
        vals: Vec<VReg>,
        src_op: Option<u32>,
    },
    
    /// Label for branch targets.
    Label(LabelId, Option<u32>),
}
```

Update `src_op()` method:

```rust
pub fn src_op(&self) -> Option<u32> {
    match self {
        // ... other variants ...
        | VInst::Call { src_op, .. }
        | VInst::Ret { src_op, .. } => *src_op,
        VInst::Label(_, src_op) => *src_op,
    }
}
```

Update `defs()` method:

```rust
pub fn defs(&self) -> impl Iterator<Item = VReg> + '_ {
    let mut v = Vec::new();
    match self {
        // ... other variants (unchanged) ...
        VInst::Call { rets, .. } => v.extend(rets.iter().copied()),  // unchanged - callee_uses_sret doesn't affect defs
        VInst::Ret { .. } => {}
        VInst::Label(..) => {}
    }
    v.into_iter()
}
```

Update `uses()` method:

```rust
pub fn uses(&self) -> impl Iterator<Item = VReg> + '_ {
    let mut v = Vec::new();
    match self {
        // ... other variants (unchanged) ...
        VInst::Call { args, .. } => v.extend(args.iter().copied()),  // unchanged - callee_uses_sret doesn't affect uses
        VInst::Ret { vals, .. } => v.extend(vals.iter().copied()),
        VInst::Label(..) => {}
    }
    v.into_iter()
}
```

`is_call()` method unchanged (still matches on `VInst::Call { .. }`).

### Update Existing Call Sites

Find and update all places that construct `VInst::Call`:

1. **In `lower.rs`** (float builtin lowering, lines 219-242):

```rust
Op::Fadd { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
    target: SymbolRef {
        name: String::from("__lp_lpir_fadd_q32"),
    },
    args: alloc::vec![*lhs, *rhs],
    rets: alloc::vec![*dst],
    callee_uses_sret: false,  // ADD: builtins use direct return
    src_op,
}),
// Same for Fsub, Fmul
```

2. **In `regalloc/greedy.rs`** (test code, line 240):

```rust
let vinsts = alloc::vec![VInst::Call {
    target: SymbolRef { name: String::from("callee") },
    args: alloc::vec![VReg(1)],
    rets: alloc::vec![VReg(2)],
    callee_uses_sret: false,  // ADD
    src_op: Some(0),
}];
```

3. **In `emit.rs` tests** (line 525):

```rust
VInst::Call {
    target,
    args,
    rets,
    callee_uses_sret: false,  // ADD
    src_op,
}
```

## Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib
```

Ensure:
- All `VInst::Call` constructions updated with new field
- No compiler errors or warnings
- Tests still pass
