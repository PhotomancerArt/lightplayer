# Plan: lpvm-native ABI Sret Implementation

## Scope of Work

Complete M1 ABI milestone by implementing sret (struct-return) calling convention for RV32:
- Functions returning >4 scalars (16 bytes) use sret pointer in a0
- Caller allocates buffer, passes address in a0
- Callee stores return values to buffer
- Applies to mat4 (16 scalars), large structs, multiple vec4s

## Current State

**Return classification exists:**
- `abi.rs` has `ReturnClass` enum with `Direct { regs }` and `Sret { ptr_reg }`
- Classification by scalar count: ≤4 = Direct, >4 = Sret
- Tests confirm mat4/vec5+ classify as Sret

**Emission does NOT handle sret:**
- `emit.rs` `VInst::Ret` handler just errors if vals.len() > 4
- No use of `ReturnClass` in emission
- Assumes all returns fit in a0-a3

**Caller does NOT handle sret:**
- `rt_emu/instance.rs` has no sret buffer allocation
- Args passed starting at a0, no shifting for sret
- No buffer reading after call

**Missing infrastructure:**
- No `AbiAnalysis` per-function struct (mentioned in roadmap)
- Can't easily detect sret from LPIR `IrFunction` alone

## Decisions

### Q1: Detect sret at call time
**Decision:** Use `ReturnClass::from_lps_types()` in caller (`rt_emu/instance.rs`).
- For sret: allocate buffer from arena, pass ptr in a0, shift real args to a1-a7
- After call: read return values from buffer

### Q2: Pass sret info to emission
**Decision:** Thread `LpsFnSig` through to emission (Option A).
- `emit_function_bytes()` will receive both `IrFunction` and function name
- Can look up `LpsFnSig` from `NativeEmuModule` signatures
- Use `ReturnClass` directly in emission for proper Ret handling
- Cleaner than extending LPIR, matches Cranelift's approach

### Q3: Handle sret in VInst::Ret emission
**Decision:** Use ReturnClass to determine strategy.
- For Sret: emit stores from vregs to a0-relative buffer (sret pointer)
- For Direct: emit moves to a0-a3 (current behavior)

### Q4: Buffer allocation
**Decision:** Allocate sret buffers from the shared arena.
- Same mechanism as VMContext allocation
- Caller allocates before call, reads results after

### Q5: Out-of-scope
**Decision:** Out-parameters deferred to separate plan.
- Focus this plan on sret only (mat4, large struct returns)
- Out-parameters are a different ABI pattern

## Open Questions

None - ready for design iteration.
