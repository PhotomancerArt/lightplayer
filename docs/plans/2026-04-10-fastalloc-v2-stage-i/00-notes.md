# M1: Core Types - Analysis Notes

## Scope of Work

Implement M1 of the fastalloc v2 roadmap: Core Types. This includes:

1. Create `isa/rv32fa/` directory structure
2. Copy ABI definitions from `rv32/abi.rs`
3. Define `PhysInst` enum (physical-register instructions)
4. Implement `PhysInst` text parser and formatter (for expect tests)
5. Wire up `rv32fa` module in `isa/mod.rs`

## Current State

### What We Have (M0 complete)

- VInst text parser/formatter in `debug/vinst.rs`
- `peephole.rs` optimizer integrated
- `debug` module exported from `lib.rs`
- All 12 VInst tests pass

### What We Need (M1)

**Directory structure:**
```
lp-shader/lpvm-native/src/isa/
├── rv32/                    # Existing - unchanged
└── rv32fa/                  # NEW - needs to be created
    ├── mod.rs               # NEW
    ├── abi.rs               # NEW - copy from rv32/abi.rs
    ├── inst.rs              # NEW - PhysInst enum
    └── debug/
        ├── mod.rs           # NEW
        └── physinst.rs      # NEW - parser/formatter
```

### ABI Content to Copy

From `rv32/abi.rs`:
- `ARG_REGS`, `RET_REGS` - register lists
- `FP_REG`, `SP_REG`, `RA_REG` - special registers
- `callee_saved_int()`, `caller_saved_int()` - register sets
- `allocatable_int()` - allocatable registers
- `reg_name()` - register name for debugging
- `func_abi_rv32()` - function ABI computation

### PhysInst Requirements

Must mirror VInst variants but with `PhysReg` (u8) instead of `VReg`:
- FrameSetup, FrameTeardown
- All arithmetic (Add32, Sub32, Mul32, etc.)
- Unary (Neg32, Bnot32, Mov32)
- Comparison (Icmp32, IeqImm32)
- Select32 (with named fields: cond, if_true, if_false)
- Memory (Load32, Store32, MemcpyWords, SlotAddr)
- Immediate (LoadImm - replaces IConst32)
- Control (Call, Ret)

### PhysInst Text Format

Similar to VInst but with `aN`/`sN`/`tN` for physical registers:
```
a0 = LoadImm 42
a0 = Add32 a1, a2
Call mod (a0, a1)
Ret
```

## Questions

### Q1: Should we re-export ABI from rv32/ or copy it?

**Context:** The ABI is stable and identical between rv32/ and rv32fa/. We could:
- **Option A:** Re-export: `pub use crate::isa::rv32::abi::*;` in `rv32fa/abi.rs`
- **Option B:** Copy: Duplicate the file contents

**Tradeoffs:**
- Option A: Less code, but creates dependency between the two ISAs
- Option B: More code, but clean separation per roadmap philosophy

**Decision:** Option B (copy) - aligns with roadmap's "clean separation" and "we will delete the old isa and rename this one once things work"

### Q2: Should PhysInst use aN/sN/tN register naming or just generic rN?

**Context:** Physical registers have specific roles:
- a0-a7: argument/return
- s0-s11: callee-saved
- t0-t6: temporary/caller-saved

**Options:**
- **Option A:** `a0, a1, s0, s1, t0, t1` etc - role-specific prefixes
- **Option B:** Generic `r0-r31` - just the hardware number

**Decision:** Option A - more readable for debugging, matches RISC-V conventions

### Q3: How much of the parser/formatter should we implement now vs later?

**Context:** Full PhysInst set has ~25 variants. We could:
- **Option A:** Implement all now (more work upfront, complete)
- **Option B:** Implement core subset (Add32, LoadImm, Call, Ret) and add rest as needed

**Decision:** Option A - implement all PhysInst variants now. There's no uncertainty (they mirror VInst exactly), so we should be complete. Add `todo!()` in the emitter for variants we don't emit yet.

## Notes

- Peephole optimizer is already done and working
- VInst parser (M0) is the template for PInst parser
- Frame operations (FrameSetup/FrameTeardown) are unique to PInst - not in VInst
- **PInst text format uses standard RISC-V assembly syntax** (add a0, a1, a2) - differentiates from VInst format and follows existing standards
- **Nomenclature**: VReg → PReg, VInst → PInst (Physical Instruction)
