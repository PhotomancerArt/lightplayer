# Fastalloc Roadmap — Notes

## Scope

Replace the current register allocation + emitter call-save scheme in
lpvm-native with a backward-walk "fastalloc" allocator that produces
per-instruction allocations and explicit move edits, eliminating the
conservative call-save overhead that dominates runtime cost.

## Current State

### Performance gap

| Metric | lpvm-native | cranelift/wasmtime | Ratio |
|--------|-------------|-------------------|-------|
| FPS (on-device) | 25 | 29 | 0.86 |
| Perf filetest inst | 3,774 | 2,009 | 1.88x |
| Rainbow filetest inst | ~1.18x | baseline | 1.18x |

The 1.88x on perf filetests is dominated by call-save overhead, not code
quality. Real shaders are closer to 1.18x.

### Root cause: static allocation + conservative call saves

The current pipeline:

1. **Allocator** (greedy or linear scan) produces a **function-wide**
   `vreg_to_phys` map — each vreg has one physical register for its entire
   lifetime.

2. **Emitter** (`emit.rs`) handles call clobbers independently:
   `regs_saved_for_call()` saves **every** assigned caller-saved register
   before each call and restores after, regardless of liveness.

This creates two compounding problems:

- **Parameters stay in caller-saved regs.** Params arrive in a0-a7 and the
  allocator never moves them to callee-saved regs (s1-s11). So a1/a2 get
  saved around every float builtin call.

- **Spilled-forever.** Once a value is spilled by the allocator, it stays
  spilled — every use generates `lw tmp, [fp+off]` even when registers are
  plentiful at that point.

- **No liveness at calls.** The emitter saves all assigned caller-saved regs,
  not just those live after the call (marked as TODO in the code).

### Assembly evidence

Early in function (low pressure): 3 regs saved per call (s1, a1, a2) = 6
memory ops per float operation.

Late in function (high pressure): 7 regs saved per call (s1, a1, a2, t3-t6) =
14 memory ops per float operation.

Each float op (fmul/fsub/fadd) is itself just 2 instructions (auipc + jalr).

### Existing allocators

- **`GreedyAlloc`** (~180 lines): single pass, exhaustion order, no liveness.
  Working correctly.
- **`LinearScan`** (~760 lines): interval-based, loop extension, spill victim
  heuristic. Currently active (`USE_LINEAR_SCAN_REGALLOC = true`). User
  reports it's partially broken.

Both produce the same `Allocation` struct: a static `vreg_to_phys` map +
`spill_slots` + `rematerial_iconst`.

### Existing design doc

`docs/design/native/2026-04-09-fastalloc-mini.md` contains a detailed algorithm
sketch, data structures, block boundary handling, and effort estimate (~800
lines). Written as reference for later implementation.

## Questions

### Q1: Allocation output format — per-instruction operand map vs edit list?

**Context:** The current `Allocation` is a global `vreg_to_phys` map. Fastalloc
needs per-instruction information. There are two approaches:

**(a) Edit list** (regalloc2-style): The allocator outputs a list of
`(position, Edit)` where Edit is `Move { from, to }`. The emitter splices these
between VInsts. The `vreg_to_phys` map is gone — the allocator tells the emitter
exactly which physical register to use for each operand at each instruction.

**(b) Per-operand allocation table**: Each VInst operand gets its own PhysReg
assignment in a parallel array. No global map. Similar in spirit to (a) but
the emitter reads from a table rather than an edit stream.

**Suggestion:** (a) edit list. It's the proven approach from regalloc2, cleanly
separates allocation from emission, and the design doc already describes this
model. The emitter walks VInsts and interleaves edits at `Before(idx)` /
`After(idx)` positions.

**Answer:** (a) Edit list. Proven approach, matches the design doc.

### Q2: Should we keep the old allocators or replace them?

**Context:** We have greedy (working, simple) and linear scan (partially broken).
Fastalloc would be a third allocator. Options:

**(a) Add fastalloc alongside, keep greedy as fallback.** The config flag
switches between them. Useful for debugging — if fastalloc has a bug, flip to
greedy.

**(b) Replace linear scan with fastalloc, keep greedy.** Linear scan is broken
anyway. Greedy stays as a simple fallback.

**(c) Replace both.** Fastalloc becomes the only allocator.

**Suggestion:** (b). Remove linear scan (it's broken), keep greedy as a debug
fallback, make fastalloc the default. The greedy allocator is simple enough to
maintain and useful for validating that a codegen issue is allocator-related.

**Answer:** Keep both greedy and linear scan as fallbacks for validation.
Add fastalloc alongside, make it the default via config flag. Remove old
allocators later once fastalloc is proven.

### Q3: Milestone split — what's the right decomposition?

**Context:** The design doc estimates ~800 lines total. You mentioned wanting at
least two plans. Possible splits:

**(a) Two milestones:**
- M1: Core allocator (backward walk, straight-line code, no control flow).
  Includes new Allocation output format, edit splicing in emitter, call clobber
  handling. Validates against straight-line filetests.
- M2: Control flow + cleanup. Block splitting, liveness at block boundaries,
  loop handling. Integration testing, perf validation, remove linear scan.

**(b) Three milestones:**
- M1: New allocation output format + emitter edit splicing (infrastructure).
  Existing allocators produce the new format. No new algorithm yet.
- M2: Core backward-walk algorithm (straight-line).
- M3: Control flow, perf validation, cleanup.

**Suggestion:** Leaning toward (a). The infrastructure (new output format) is
tightly coupled with the algorithm — building it separately without the
algorithm to exercise it may create integration issues. Two milestones gives
clean separation: "make it work on straight-line code" then "make it work on
everything."

**Answer:** (b) Three milestones. M1 = new output format + emitter plumbing
(existing allocators produce the new format). M2 = backward-walk algorithm
(straight-line). M3 = control flow + perf validation + cleanup.

### Q4: How to handle params — move to callee-saved at entry, or let the allocator handle it?

**Context:** Currently params are precolored to a0-a7 (caller-saved) and stay
there forever. Fastalloc's backward walk would naturally evict them to stack at
the first call, then reload into whatever register is free. But there's a
simpler option: insert explicit `mov a1 -> s_x` at function entry and let the
allocator treat the s-reg copy as the "real" home.

**(a) Let fastalloc handle it naturally.** The backward walk sees the param is
live across a call, evicts to stack, reloads after. Simple, correct, but pays
one spill+reload per call boundary.

**(b) Insert param copies to callee-saved regs in the lowerer.** Add
`Mov32 { dst: v_copy, src: v_param }` at function entry. The allocator sees
v_copy as a fresh vreg and assigns it a callee-saved reg. Params that span many
calls benefit from one move at entry vs repeated spill/reload.

**(c) Let the allocator itself detect long-lived params and prefer callee-saved
regs.** More complex but optimal — the allocator knows both the live range and
the available callee-saved regs.

**Suggestion:** (a) for the initial implementation. The backward walk handles it
correctly, and the spill/reload cost is small compared to the current overhead.
(b) or (c) can be a follow-up optimization once the allocator is working.

**Answer:** (a) Let fastalloc handle it naturally for now. The backward walk
evicts params at call boundaries correctly. Follow-up optimization: insert
explicit copies to callee-saved regs at entry (b) or teach the allocator to
prefer callee-saved for long-lived values (c) if param handling shows up as a
bottleneck in perf numbers.

### Q5: What's the validation strategy?

**Context:** Register allocation bugs are notoriously hard to debug. The
existing filetest infrastructure compares instruction counts and verifies
correctness. Questions:

- Are the current filetests sufficient, or do we need new ones targeting
  fastalloc-specific patterns (e.g., values that should be reloaded into
  different registers after calls)?
- Should we validate against the greedy allocator (same result, fewer
  instructions) or against cranelift (different IR, different approach)?
- Do we want a "round-trip" test that compiles + executes on the emulator and
  checks output values?

**Suggestion:** Use existing filetests for correctness, add a few targeted
filetests for call-clobber patterns and block-boundary behavior. The emulator
round-trip tests (`fw-tests`) are the ultimate validation. Keep greedy as a
correctness reference — if fastalloc and greedy produce different outputs on the
emulator, the bug is in fastalloc.

**Answer:** Existing filetests are thorough and have been sufficient. Use
filetests as primary validation during development. Add targeted filetests for
call-clobber and spill/reload patterns specific to fastalloc. Emulator
round-trip tests as final gate. Keep greedy as correctness reference behind
config flag.

## Future Improvements

- **Param-to-callee-saved optimization:** Once fastalloc is working, measure
  whether long-lived params in caller-saved regs are still a bottleneck. If so,
  either insert explicit copies to s-regs at function entry in the lowerer, or
  teach the allocator to prefer callee-saved regs for long-lived values.
