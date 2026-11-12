# FastAlloc v4 - Development Notes

## Scope

Rewrite the `fa_alloc` register allocator in `lpvm-native` to use an
**edit-list** architecture instead of direct PInst emission. The previous
approach (fastalloc3) attempted direct backward-walk PInst emission, which
proved fundamentally flawed: later backward walk decisions (evictions) can
invalidate register assignments in already-emitted instructions. The edit-list
approach, proven by regalloc2's fastalloc, separates allocation decisions from
instruction emission.

## Current State

### What works

The `lpvm-native` crate has a complete pipeline *around* the allocator:

- **Lowering** (`lower.rs`): LPIR → VInst + RegionTree. ~1780 lines, solid.
- **VInst IR** (`vinst.rs`): compact post-lowering IR. Clean.
- **RegionTree** (`region.rs`): arena-based structured control flow. Clean.
- **Encoding** (`rv32/encode.rs`): instruction encoders. Solid.
- **ABI** (`abi/`): FuncAbi, classify, frame layout. Solid.
- **Emission** (`emit.rs`): orchestration wrapper. Thin, clean.
- **Compilation** (`compile.rs`): module-level compilation. Solid.
- **Linking** (`link.rs`): relocation resolution. Solid.
- **Runtimes** (`rt_jit/`, `rt_emu/`): JIT and emulator engines. Solid.
- **Debug** (`debug/`, `debug_asm.rs`): disassembly, VInst display. Solid.
- **Peephole** (`peephole.rs`): VInst optimization. Solid.
- **RegSet** (`regset.rs`): bitset for VRegs. Clean.
- **CLI**: `shader-rv32fa` command. Working.
- **Filetests**: `rv32fa` backend target exists in `lps-filetests`. 19/27 passing.

### What's broken

**`fa_alloc/walk.rs`** (1632 lines): the backward walk directly emits PInsts.
This is architecturally flawed — evictions during later backward steps
invalidate register assignments in already-emitted instructions. The value a
register holds in forward execution doesn't match what the backward walk assumed
when it emitted the instruction.

### What to keep vs gut

| Module | Status | Action |
|--------|--------|--------|
| `fa_alloc/walk.rs` | Broken architecture | **Delete** |
| `fa_alloc/mod.rs` | Entry point + frame wrapping | **Rewrite** |
| `fa_alloc/spill.rs` | Slot allocator | **Keep** |
| `fa_alloc/liveness.rs` | Region-tree liveness | **Keep** |
| `fa_alloc/trace.rs` | Debug trace | **Keep** (may extend) |
| `rv32/inst.rs` | PInst type | **Delete** (no longer needed) |
| `rv32/rv32_emit.rs` | PInst → bytes | **Delete** (replaced by ported emitter) |
| `rv32/debug/pinst.rs` | PInst debug fmt | **Delete** |
| `emit.rs` | Orchestration | **Rewrite** (call new alloc + new emitter) |
| Everything else | Working | **Keep** |

### Filetest inventory (rv32fa.q32)

18 lpvm/native filetests total. As of last run, 19/27 passing across all
filetests (many non-native filetests also run against rv32fa). Failing:

- **Spill bugs**: `spill_simple.glsl`, `perf/spill-density.glsl`
- **Sret missing**: `native-call-vec4-return.glsl`, `native-call-mat4-return.glsl`,
  `perf/mat4-reg-pressure.glsl`, `spill_pressure.glsl`
- **Stack args**: `perf/stack-args-incoming.glsl`, `perf/stack-args-incoming-16.glsl`,
  `perf/stack-args-outgoing.glsl`

## Questions

### Q1: Should we delete walk.rs entirely or try to salvage pieces?

**Answer:** Delete entirely. Start fresh. Bring over `RegPool` (LRU utility)
into the new walk module; port the old `lpvm-native` emitter (`EmitContext`)
as the forward emitter.

### Q2: What does the allocation output look like?

**Answer:** Follow regalloc2 closely. Per-operand allocations (not per-vreg
global table), plus a sorted edit list. Concretely:

regalloc2's `Output`:
- `allocs: Vec<Allocation>` — flat array, one entry per operand across all insts
- `inst_alloc_offsets: Vec<u32>` — index into `allocs` for each instruction
- `edits: Vec<(ProgPoint, Edit)>` — sorted moves tagged before/after an inst
- `num_spillslots: usize`

`Allocation` is a tagged union: register, stack slot, or none.
`Edit` is just `Move { from: Allocation, to: Allocation }`.

Our simplified version for RV32 integer-only:
- `allocs: Vec<Alloc>` where `Alloc` = PReg | SpillSlot | None
- `inst_alloc_offsets: Vec<u16>` (we won't exceed 64K operands)
- `edits: Vec<(EditPoint, Edit)>` where EditPoint = Before(inst) | After(inst)
- `Edit::Move { from: Alloc, to: Alloc }` (covers spill, reload, reg-reg move)
- `num_spill_slots: u32`

**Why per-operand, not global vreg→preg?** A vreg can be evicted from one
register and reloaded into a different one. e.g. v5 in `s3` at inst 4, evicted,
then v5 in `s9` at inst 12. A global table can't represent this. The linear
scan (old `lpvm-native`) avoids this by never evicting — spilled vregs stay
spilled and use temp registers at every access. Per-operand allocation lets us
do dynamic eviction/reload, producing better code.

### Q3: Should the backward walk still process region-tree or should we flatten?

**Answer:** Keep region-tree walk. Structured control flow is our key advantage.
The walk recurses into regions, records allocations and edits (not PInsts).
Spill-at-boundary for IfThenElse/Loop stays conceptually the same.

### Q4: Do we need PInst?

**Answer:** No. Delete PInst and the PInst emitter. Port the old `lpvm-native`
`EmitContext` which goes directly from VInst + allocation → bytes. It handles
calls, sret, branches/fixups, spill loads/stores via `use_vreg`/`def_vreg`.
Proven code. Adapt for FA crate's VInst types (VReg u16, VRegSlice, SymbolId).

### Q5: Milestone structure?

**Answer:** Separate gutting/prep milestone (M1), then build up incrementally:
1. M1: Gut + prep (delete broken code, set up types, port emitter, placeholders)
2. M2: Straight-line allocator + unit tests (edit-list infra, backward walk
   for Linear regions, validate with unit tests and spill_simple filetest)
3. M3: Calls + sret (call clobbers, arg/ret ABI, sret, stack-passed args)
4. M4: Control flow (IfThenElse, Loop, spill-at-boundary edits)
5. M5: Cleanup (remove lpvm-native, rename lpvm-native)

### Q6: Testing strategy?

**Answer:** Snapshot-style unit tests with a textual format. Use the VInst text
parser (`debug/vinst.rs`) for input. Render the AllocOutput as human-readable
text showing edits interleaved with annotated VInsts. Compare against expected
string. When a test fails, print the actual output; copy-paste to bless.

Test format — single column, edits appear before/after their instruction:

```
expect_alloc("
    i0 = IConst32 10
    i1 = IConst32 20
    i2 = Add32 i0, i1
    Ret i2
", "
mv s5 -> slot[0]
i0 = IConst32 10          # i0→s3
---
i1 = IConst32 20          # i1→s5
---
i2 = Add32 i0, i1         # i0→s3 i1→s5 i2→s3
---
Ret i2                     # i2→s3
");
```

Don't rely on filetests for basic cases — filetests are integration validation.
AllocTrace should be part of the output for debugging.

## Notes

- Follow regalloc2 fastalloc as closely as possible. Previous deviations
  (direct PInst emission) failed.
- The old `lpvm-native` emitter is the reference for forward emission.
  Port it rather than rewrite.
- Debugging: AllocTrace for allocator decisions + disassembly for bytes.
  No PInst layer needed.
- Allocation output should carry trace/debug info for inspection.
- `debug/vinst.rs` has a text parser for VInst — use it for allocator unit
  tests. Parse text → VInst[], run allocator, check allocs/edits.
