# LPIR Inliner — Future Work

Things surfaced while planning M0–M5 that are real wins but not blocking
the inliner. Capture here so they don't get forgotten.

## Remove denormalized control-flow offsets

### Problem

`LpirOp::IfStart`, `LoopStart`, `SwitchStart`, `CaseStart`, `DefaultStart`,
and `Block` all carry `else_offset` / `end_offset` / `continuing_offset`
fields. These are **caches of structural information** — they can be fully
recomputed by walking the body and matching openers to their closers
(`Else`, `End`, the new `Continuing` marker from M2.5).

Storing them in the IR is denormalization. The cost shows up every time a
pass mutates the body:

- M3 (inliner) needs a recompute pass over the entire body of every
  function it transforms.
- Every future structural transform (loop unrolling, dead-code-elim, peephole
  on control flow, etc.) inherits the same maintenance burden.
- Bugs in offset maintenance are subtle: tests pass for "happy path" code
  shapes and explode on contrived nesting. Hard to fuzz.

The inliner conversation made this concrete: even after M2.5 adds the
`Continuing` marker for parity, every consumer that mutates the body has
to remember to call `recompute_offsets` or the cached fields go stale.

### Proposal

1. Drop `else_offset`, `end_offset`, `continuing_offset` from
   `LpirOp`. The opener variants become e.g.:

   ```rust
   IfStart   { cond: VReg }
   LoopStart {}                  // no fields at all
   SwitchStart { selector: VReg }
   CaseStart   { value: i32 }
   DefaultStart {}
   Block     {}
   ```

2. Add a single `lpir::offsets` module exposing:

   ```rust
   /// Side-table keyed by op index (`body[i]`) → derived offsets.
   pub struct OffsetMap {
       /// Per-index entry; only populated for opener ops.
       entries: Vec<Option<Offsets>>,
   }

   pub enum Offsets {
       If    { else_pc: u32, end_pc: u32 },
       Loop  { continuing_pc: u32, end_pc: u32 },
       Switch { end_pc: u32, /* per-arm: */ arm_ends: SmallVec<...> },
       Case  { end_pc: u32 },
       Block { end_pc: u32 },
   }

   pub fn compute_offsets(body: &[LpirOp]) -> OffsetMap;
   ```

   Single O(n) pass, identical to the M3 inliner's recompute pass. No
   allocation per op for non-opener positions (use `Option<Offsets>` or
   a sparse map).

3. Each backend / interpreter / validator calls `compute_offsets(&body)`
   exactly once at function entry, then looks up by `pc` as needed.

   - Cost: one extra O(n) walk per function compile. Negligible compared
     to actual codegen.
   - Benefit: zero maintenance burden for any pass that mutates the
     body. Inliner becomes simpler. Any future transform (loop fusion,
     control-flow simplification, predicate hoisting, …) becomes
     trivially correct w.r.t. offsets.

### Scope estimate

Touches all three backends + interpreter + validator + parser/printer
(printer needs to walk and find positions to print `else:` / `end` text;
parser already builds without offsets, just patches at end). Roughly the
same shape as M2 + M2.5 combined. ~12-15 files.

### When to do it

- **Not** during M3-M5 — those should stay focused.
- After M5 lands, when we're touching backends for other reasons (more
  passes, perf tuning, etc.) and the velocity benefit of "no offset
  bookkeeping in transforms" starts compounding.
- Pre-requisite for M2.5 to land first (or land them together as a
  combined cleanup).

### Acceptance criteria

- All filetests pass with no behavioral change.
- A representative pass that mutates the body (could be the inliner
  itself, after M3) becomes shorter — measure LOC delta on `inline/`.
- A new test category: "structural mutation" — randomly insert/remove
  `Copy` ops in valid loop nests and assert behavior is preserved
  without any offset bookkeeping.

## Inline-and-delete-as-we-go (peak-memory optimization)

### Problem

Today (M3 + M5):

1. M3 inlines all `Call` ops, leaving fully-inlined helpers in
   `LpirModule.functions` with zero remaining callers.
2. M5 (DeadFuncElim) runs as a separate pass and deletes them.

In between, the module holds **both** the original helpers *and* the
inlined-into callers. Peak memory during compile is roughly
`sizeof(callers post-inline) + sizeof(unused helpers)`. On embedded
targets (ESP32, ~120 KB heap budget for compile state), this matters for
shaders with many helpers.

### Proposal

When the inliner finishes a callee `f` (i.e. has spliced into all
callers), and `f` is not in the configured root set / entry set, delete
`f` from `LpirModule.functions` immediately.

- Saves peak memory ≈ `sizeof(f.body) + sizeof(f.vreg_pool)` per fully
  inlined helper, summed over all helpers, integrated over the time
  between M3 and M5 today.
- Bottom-up topological order makes this safe: `f` is processed only
  after all its own callees have been inlined into `f`'s body, and `f`
  is deleted only after all *its* callers have been processed.

### Why not now (M3)

- M5's deletion logic is non-trivial (root set, sig filtering, `FuncId`
  hygiene). Building it first as a standalone pass and then optionally
  collapsing into M3 is the safer path.
- M3 staying read-only at the function-set level (only mutates `body` /
  `vreg_pool` / `vreg_types` / `slots`) keeps tests simple — every
  function the test set up is still there to be inspected after the
  pass.

### When

After M5 lands and is well-tested. Add an `InlineConfig` knob like
`prune_during_inline: bool` (default `false` for filetests, `true` for
production callers with a configured root set).

## Other follow-ups

(Add additional future-work items here as they surface.)
