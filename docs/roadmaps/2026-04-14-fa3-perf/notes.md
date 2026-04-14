# FA3 Performance Roadmap — Notes

## Scope

Improve rv32fa (lpvm-native-fa) code quality toward parity with rv32
(lpvm-native / cranelift + regalloc2), focusing on call-heavy code (Q32 math).

Current benchmark: `test_four_live_across_call` — 41 instructions (rv32fa) vs
26 instructions (rv32 cranelift). 1.58x ratio.

Both backends receive the same LPIR. The gap comes from how LPIR is lowered to
VInsts and how those VInsts are register-allocated.

## Current state

Pipeline: `LPIR → lower_ops → VInsts → fa_alloc (backward walk) → emit`

- **Lowering** (`lower.rs`): 1:1 mapping from LPIR ops to VInsts. No constant
  folding, no immediate-operand fusion. `IConst32` always produces a separate
  VInst; binary ops always take two register operands.

- **Regalloc** (`fa_alloc/walk.rs`, `fa_alloc/pool.rs`): Backward-walk
  allocator with LRU pool. Call handling uses save/restore pairs for
  caller-saved t-regs (step 3 of `process_call`).

- **Peephole** (`peephole.rs`): Exists but only removes redundant branches.
  Not wired into `compile_function`.

- **Emit** (`emit.rs`): Prologue/epilogue saves/restores every callee-saved
  register that appears in any alloc or edit. More callee-saved regs used =
  more prologue/epilogue instructions.

## Identified improvement areas

### A. Pool LRU register reuse

`pool.free()` clears the occupant but leaves the register at its MRU position
in the LRU list. Next `alloc()` scans from front (LRU end) and finds an s-reg
before the recently-freed t-reg. This pushes post-call temporaries into
callee-saved registers unnecessarily.

**Fix**: move freed regs to front of LRU so they are reused first.

**Impact**: Reduces total callee-saved register count. For the benchmark, this
is the single biggest source of bloat (~10 instructions in prologue/epilogue).

### B. Evict-then-reload call clobber

Current step 3 of `process_call` emits Before(call) save + After(call) restore
pairs for each caller-saved pool reg with a live vreg. This creates an ordering
hazard (documented in fa-impl-notes.md) and requires fixup logic for arg
evictions.

regalloc2's fastalloc instead evicts the vreg from the pool and emits only
After(call) reload. The def, encountered later in the backward walk, writes
directly to the spill slot. No save needed.

**Fix**: Replace save/restore pairs with evict + reload-only. Remove the
`before_saves` fixup logic.

**Impact**: Cleaner architecture, eliminates ordering hazard. Instruction count
savings depend on how many t-regs are live at call sites — modest for this
benchmark, significant for complex multi-call sequences.

### C. Constant/immediate folding in lowering

Cranelift lowers `iadd v1, v2` where `v1 = iconst 1` into `addi rd, rs, 1`,
folding the constant as an immediate. Our lowering emits separate `IConst32`
VInsts for every constant, which each consume a register.

For `test_four_live_across_call`, constants 1, 3, 4 are only used as one
operand of an `iadd` after the call. If folded as immediates, they wouldn't
need registers at all — saving 3 registers and 3 instructions.

**Fix**: Add a lowering pass or peephole that fuses `IConst32` + binary op
into an immediate-form VInst (e.g. `AddImm32`). Requires new VInst variants
and emitter support.

**Impact**: Reduces register pressure and instruction count. High impact for
arithmetic-heavy code.

### D. Call arg/ret register shortcuts

The backward walk allocates entry parameters (v0 in a0) to pool callee-saved
regs, then the entry_move copies a0 → s8, and the call moves s8 → a0. Two
wasted instructions + one wasted callee-saved reg.

Similarly, constants used only as call args (e.g. `iconst 42` used only as
arg to a call) get materialized into a callee-saved reg, then moved to the
arg reg.

**Fix**: Detect at the call that an arg vreg is the entry parameter or a
single-use constant, and short-circuit the allocation.

**Impact**: Saves 2-4 instructions per call for common patterns. Moderate
complexity.

### E. Empty-function overhead (prologue/epilogue/emit)

Even the identity function (`callee_identity`: return second arg) emits 9
instructions vs cranelift's 2. Three sources:

1. **Frame pointer always emitted** — s0/fp save/restore + sp adjust even when
   there are no spills, no callee-saved regs, and no frame pointer is needed.
   4 wasted instructions.

2. **Entry move roundtrip** — a1 → t4 → a0 instead of a1 → a0. The allocator
   puts v1 in a pool reg, then entry/ret ABI shuffles around it. 1 wasted
   instruction (related to D but visible even without calls).

3. **Redundant branch to epilogue** — `Br L0` / `Label L0` emits `j 4` that
   jumps to the very next instruction.

For Q32 math where every fmul/fadd/sin is a function call to a short helper,
this fixed overhead applies to every callee.

**Fix**: (1) Omit fp save/restore when no spills and no callee-saved regs are
used. (2) Collapse entry param → ret value when the allocator sees a direct
pass-through. (3) Peephole or emitter: eliminate `j` to the immediately
following instruction.

**Impact**: High for Q32 where most functions are short call targets. Saves
4-7 instructions per trivial function.

## Questions

### Q1: Milestone ordering — LRU fix first?

Suggested: A first, then B, then E, then C/D as later milestones.

**Answer**: Yes, that order.

### Q2: Scope of immediate folding (C)?

Suggested: Defer — keep in roadmap as a late milestone for the record.

**Answer**: Keep as late milestone. Roadmap captures things we can improve; we
won't necessarily do them all.

### Q3: Scope of call arg shortcuts (D)?

Suggested: Same — keep as late milestone.

**Answer**: Same as C.

### Q4: How do we validate improvements?

Suggested: Existing tools are sufficient.

**Answer**: Yes. The filetest runner (`scripts/glsl-filetests.sh -t rv32,rv32fa`)
gives total instruction counts and `vs fastest` ratio. The `shader-debug` CLI
gives per-function drill-down. No new infrastructure needed.

Baseline: `caller-save-pressure.glsl` — rv32 85 inst, rv32fa 140 inst (1.65x).
