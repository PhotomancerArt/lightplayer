# M1: ABI - sret, Multi-Return, Out-Params

## Scope of Work

Implement the RISC-V RV32 ILP32 calling convention for LPIR, matching Cranelift's ABI:

- **Multi-scalar returns**: Return small structs/vectors in registers (vec2 in a0-a1, vec4 in a0-a3)
- **`sret` pointer**: For large returns (>4 scalars), pass destination address in a0
- **Out-parameters**: Pointer arguments for LPIR pointer types (e.g., `gradient` param in `lpfx_psrdnoise`)
- **Stack spill slots**: Frame layout with proper alignment for spilled registers
- **Frame management**: Prologue/epilogue with saved ra, frame pointer, spill area

This enables the ABI support needed for rainbow.glsl's builtins and vector returns.

## Current State

Looking at `lpvm-native/src/isa/rv32/abi.rs`:

- Basic `ArgAssignment` exists but only assigns one register per `FnParam` (no struct expansion)
- `return_reg()` returns a single register (a0) - no multi-return support
- `FrameLayout` exists but minimal: only tracks saved_ra, no spill slot calculation
- `ARG_REGS` defined (a0-a7), `RET_REGS` defined (a0-a1), `CALLER_SAVED`, `CALLEE_SAVED` defined
- No `sret` classification
- No out-parameter handling (pointer args treated same as scalars)

Looking at `lpvm-native/src/isa/rv32/emit.rs`:

- `EmitContext` has `frame_size` fixed at 16 bytes
- `emit_prologue()` handles ra save for non-leaf
- `emit_epilogue()` handles ra restore
- No spill slot emission (no sw/lw for spilled vregs)
- No `sret` pointer setup
- Return values: simple move to RET_REGS[0], no multi-value handling

The current ABI is essentially "M1 POC" level - works for single scalar returns, no spills, no structs.

## Questions

### Q1: How to classify returns (scalar vs sret)

**Question:** What is the exact threshold for using `sret` pointer vs direct register returns? Cranelift uses >4 scalars as the cutoff, but should we match exactly?

**Current state:** Currently only returns single scalar in a0.

**Context:** Rainbow.glsl returns vec4 (4 scalars = a0-a3 direct), but some builtins or future shaders might return larger structs.

**Suggested answer:** Match Cranelift: ≤4 scalars = direct in a0-a3, >4 scalars = sret pointer in a0. This is the standard RV32 calling convention.

**Answer (user):** Match Cranelift and standard RISC-V psABI: ≤16 bytes (4 scalars on RV32) = direct in a0-a3, >16 bytes = sret pointer in a0. QBE uses the same threshold (16 bytes), confirmed in `rv64/abi.c:146-155` where `size > 16` triggers `Cptr` classification. This aligns with Cranelift and avoids changing function dispatch.

_Status: **answered**._

### Q2: Out-parameter ABI for builtins

**Question:** How should out-parameters like `gradient` in `lpfx_psrdnoise` be handled in the ABI?

**Current state:** LPIR represents out-params as pointer arguments. Current abi.rs just assigns them to arg registers like scalars.

**Context:** `psrdnoise(vec2 pos, vec2 per, float time, out vec2 gradient, uint seed)` - `gradient` is written by the builtin.

**Suggested answer:** Pointer arguments use standard integer register passing (a0-a7). The caller allocates stack space for the out-param, passes address in register. This is standard - no special ABI treatment needed, just ensure the lowering emits the proper pointer address calculation.

**Answer (user):** Out-parameters are just **pointer arguments** using standard integer register passing (a0-a7). The "out" semantics are handled at the LPIR/lowering level (caller allocates stack, passes address), not in the ABI. No special ABI treatment needed—treat as normal pointer args.

_Status: **answered**._

### Q3: Spill slot layout and alignment

**Question:** What layout and alignment for spill slots in the frame?

**Current state:** `frame_size` is hardcoded 16 bytes. No spill slot tracking.

**Context:** Greedy allocator in POC has no spill support. We need spills for the rainbow checkpoint (even if inefficient).

**Suggested answer:** 16-byte frame minimum. Spill slots at negative offsets from frame pointer (s0). Each spill slot is 4 bytes (I32/F32) or 8 bytes (I64) with 4-byte alignment minimum. Track `spill_size` in `FrameLayout` separate from fixed frame overhead.

**Discussion:** Compared QBE (frame pointer with negative offsets) vs Cranelift (SP-relative with computed offsets). Frame pointer approach is simpler: s0 is an anchor, spills use negative offsets (slot 0 = -8, slot 1 = -12, etc.). This enables simple stack unwinding for debugging. For embedded shaders where panic=abort, this is the right trade-off—simple implementation over optimal code.

**Answer (user):** Use **QBE-style frame pointer addressing**: s0 points to saved s0 (offset 0), ra at s0+4 (or swapped), spills at negative offsets from s0. Simple, intuitive, proven in QBE.

_Status: **answered**._

### Q4: Greedy+spill interim vs waiting for linear scan

**Question:** Should we implement spill support for the greedy allocator as an interim, or focus ABI and leave spills for M3 linear scan?

**Current state:** Greedy allocator has no spill support - it fails if vregs exceed available registers.

**Context:** User wants "green rainbow on greedy+spill first" as a checkpoint. But spills touch both ABI (frame layout) and regalloc.

**Suggested answer:** Implement basic spill support in M1 ABI work: frame layout with spill slots, prologue/epilogue spill code generation. The greedy allocator can use it in M2. This separates concerns - ABI knows how to emit spills, allocator decides when to spill.

**Answer (user):** Implement **working greedy allocator with spills** as an interim checkpoint. It's simple, and with an instruction counter in filetests, we can easily measure the performance impact and see the improvement when linear scan lands. This gives us a baseline for comparison.

_Status: **answered**._

## Notes

- All questions answered. Ready for design iteration.
- Frame layout: QBE-style with frame pointer (s0) and negative offsets for spills
- Greedy+spill will be the first milestone after ABI, giving us measurable performance data
- Instruction counting in filetests will quantify the spill overhead
- Decisions:
  1. Return classification: ≤16 bytes = direct (a0-a3), >16 bytes = sret pointer in a0
  2. Out-params: normal pointer args via a0-a7, no special ABI handling
  3. Frame layout: s0-relative with negative offsets, QBE-style
  4. Greedy+spill as interim milestone, with instruction counting for comparison
