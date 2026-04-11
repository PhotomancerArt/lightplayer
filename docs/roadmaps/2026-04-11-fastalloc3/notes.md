# FastAlloc v3 - Development Notes

## Scope

Continue the FastAlloc register allocator from where v2 left off. The v2 roadmap
(M0-M9) completed M0-M4 and built the crate structure, VInst IR, debug infra,
CLI, emitter, region tree, liveness shell, trace system, and backward walk shell.
The remaining work is: real allocation decisions, control flow, call clobbers,
integration, validation, and cleanup.

## Current State

### What exists in `lpvm-native-fa`

**Working compile pipeline (straight-line only):**
- `lower.rs` → LPIR to VInst + RegionTree
- `peephole.rs` → VInst optimization
- `rv32::alloc` → forward-pass allocator (flat VInst list, free-list, last-use)
- `rv32::rv32_emit` → PInst to bytes
- `compile.rs` → ties it all together via `compile_function`

**Stubbed region-tree allocator (`fa_alloc/`):**
- `liveness.rs` → backward liveness for Linear/Seq regions (IfThenElse/Loop stubbed)
- `trace.rs` → AllocTrace with stub entries
- `walk.rs` → backward walk over RegionTree (stub decisions, no allocation)
- `mod.rs` → `run_shell()` entry point (not wired to compile)

**Infrastructure:**
- `region.rs` / `regset.rs` → RegionTree arena, RegSet bitset
- `abi/` → module/function ABI, classification, frame layout
- `rv32/debug/` → disasm, PInst format, region tree display
- `debug/vinst.rs` → VInst text format + parser
- `link.rs` → ELF/JIT linking
- `rt_jit/` / `rt_emu/` → runtime engines (gated)
- CLI: `shader-rv32fa` with `--show-region`, `--show-liveness`, `--show-vinst`, etc.

### Two allocator paths

1. **`rv32::alloc::allocate`** — the one actually used by `compile.rs`. Forward
   pass over flat `&[VInst]`. Handles straight-line only. No spill, no calls,
   no branches.

2. **`fa_alloc::run_shell`** — region-tree backward walk. All decisions stubbed.
   Not connected to compilation.

### What the old roadmap planned but wasn't done

| Old M# | Topic | Status |
|--------|-------|--------|
| M5 | Allocator Core (RegPool, LRU, spill, reload) | Not started |
| M6 | Call Clobbers (caller-save around calls) | Not started |
| M7 | Integration (wire to `compile_function`) | Not started |
| M8 | Validation (filetests, edge cases) | Not started |
| M9 | Cleanup (remove old allocator) | Not started |

## Questions

### Q1: Do we replace `rv32::alloc` with `fa_alloc` or evolve it?

**Context:** There are two allocator implementations. `rv32::alloc` is simple
and working for straight-line code. `fa_alloc` has the region-tree architecture
needed for control flow but is entirely stubbed.

**Suggested answer:** Replace. The whole point of the region tree + backward walk
is to handle control flow properly. `rv32::alloc` was a stepping stone. We build
real allocation into `fa_alloc`, wire it into compile, then delete `rv32::alloc`.

**Answer:** Replace. Build real allocation into `fa_alloc`, wire it in, delete `rv32::alloc`.

### Q2: Should M5 (allocator core) start with straight-line only, or handle control flow from the start?

**Context:** The old M5 was straight-line only, with control flow deferred.
But `fa_alloc` already has the region tree walk for IfThenElse/Loop (just with
stub decisions). We could implement allocation for all region types at once.

**Suggested answer:** Start with Linear regions only (matching what `rv32::alloc`
does today), then extend to IfThenElse and Loop in the same milestone. This lets
us validate against the existing allocator before adding complexity.

**Answer:** Start Linear-only, validate against rv32::alloc, then extend to IfThenElse/Loop.

### Q3: PInst output — does `fa_alloc` produce `Vec<PInst>` or something different?

**Context:** The current `rv32::alloc` outputs `Vec<PInst>`. The old M5 plan
envisioned a `PhysInst` type. The region-tree walk needs to produce a flat
instruction stream from a tree traversal.

**Suggested answer:** `fa_alloc` should produce `Vec<PInst>` (the existing type).
No new instruction type. The walk collects PInsts into a flat vec during
traversal — forward collection even though decisions are made backward.

**Answer:** Produce `Vec<PInst>` (existing type). Future work: pluggable emitter
that could generate bytecode directly, skipping PInst intermediary.

### Q4: How do we validate the new allocator against the old one?

**Context:** We need confidence that the new allocator produces correct code
before deleting the old one.

**Suggested answer:** Run all existing filetests with both allocators and compare
output bytes or at minimum execution results. The existing `rv32::alloc` only
handles straight-line, so validation is limited to that subset. For control flow,
we need new filetests or rely on the emulator tests.

**Answer:** Validate against cranelift pipeline (`lpvm-native`) and filetests.
Nothing in `lpvm-native-fa` is load-bearing — `rv32::alloc` doesn't need to be
preserved as a reference. The cranelift crate + filetests are ground truth.

### Q5: What's the milestone granularity?

**Context:** The old roadmap had 10 milestones (M0-M9). We're picking up from
M5 onward. The remaining work is: core allocation, control flow liveness, call
handling, integration, validation, cleanup.

**Suggested answer:** 4 milestones:
- M1: Allocator core (straight-line PInst output from fa_alloc)
- M2: Control flow + calls (IfThenElse, Loop liveness, call clobbers)
- M3: Integration (wire to compile_function, compare with rv32::alloc)
- M4: Validation + cleanup (filetests, remove rv32::alloc)

**Answer:** 4 milestones (reordered from initial suggestion):
- M1: Allocator core (straight-line, unit tests)
- M2: Integration (wire to compile, rv32fa filetest target, validate straight-line)
- M3: Control flow + calls (full functionality)
- M4: Cleanup (remove lpvm-native entirely, rename lpvm-native-fa to lpvm-native)

## Notes

- No rv32fa filetest target exists yet. Current targets: rv32 (cranelift), rv32lp
  (linear scan), wasm, jit.
- lpvm-native-fa has CLI support (`shader-rv32fa`) but no filetest integration.
- Nothing in lpvm-native-fa is load-bearing — cranelift pipeline is the reference.
- Future work (not in this roadmap): pluggable emitter for direct bytecode generation.
