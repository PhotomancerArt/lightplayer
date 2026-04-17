# LPIR Inliner — Stage III Notes (M3: Inlining Pass)

## Summary (shipped 2026-04-17)

**M2.5:** `LpirOp::Continuing` marks the start of a loop’s continuing block; builder, parse, print, validate, interpreter, const-fold, and all three backends handle it (marker is structural; backends still use cached `LoopStart::continuing_offset`).

**M3:** `lpir::inline_module(&mut LpirModule, &InlineConfig) -> InlineResult` plus the crate-private `lpir/src/inline/` submodule (`callgraph`, `offsets`, `remap`, `splice`, `heuristic`): bottom-up topo order, cycle skip, per-param scan with alias-or-copy remap, multi-return `Block`/`ExitBlock`/`End` wrapping, structural `recompute_offsets`, heuristic + `log::debug!` / `log::info!`. Only `inline_module` and `InlineResult` are public from `lpir`; the `inline` module is not re-exported as a path.

Source roadmap: `docs/roadmaps/2026-04-15-lpir-inliner/m3-inlining-pass.md`.

This is the meat of the inliner work. M0 (stable `CalleeRef`) and M2 (`Block` /
`ExitBlock` ops) are landed; M1 (`compile-opt` + `CompilerConfig`) is landed
in `lpir`. This stage adds `lpir/src/inline.rs`: a module-level pass that
replaces every local `Call` with the callee's body, in place, never deleting
functions. Wiring (M4) and dead-function elimination (M5) are out of scope.

## Scope of work

Build `lpir::inline_module(&mut LpirModule, &InlineConfig) ->
InlineResult` plus everything it needs:

1. Call-graph construction (callees-of, callers-of, call-site count).
2. Bottom-up topological order (leaves first), with a cycle-skip safety net.
3. Per-function inlining transform:
   - VReg remap (vmctx → caller vmctx; params → arg vregs; rest → fresh).
   - Slot remap (append callee slots to caller, offset by `caller.slots.len()`).
   - VReg-pool splice for any remaining (import) `Call` ops in the inlined body.
   - Body splicing with multi-return wrapping (`Block` / `ExitBlock` / `End`).
4. Single offset-recomputation pass per mutated function (`else_offset`,
   `end_offset`, `continuing_offset`).
5. Heuristic decision (`InlineMode::Auto` / `Always` / `Never` + budgets).
6. Unit tests covering: single-return callee, multi-return callee, callee
   that calls an import, callee with slots, diamond call graph (A→B,C; B→C),
   void callee, recursion-skip, post-condition that all original functions
   remain.
7. Round-trip safety: parse → inline → validate must succeed for every
   passing test.

Out of scope (deferred): wiring into `lpvm-native::compile_module`, filetest
`compile-opt` tagging, perf measurement on `rainbow.glsl`, dead function
elimination.

## Current state of the codebase

### What's already in place (M0/M1/M2)

- `CalleeRef = Import(ImportId(u16)) | Local(FuncId(u16))`. Stable ids.
  `LpirModule.functions` is a `BTreeMap<FuncId, IrFunction>` keyed by stable id.
- `LpirOp::Block { end_offset }` and `LpirOp::ExitBlock` exist with full
  parser/printer/interp/validator support and lower in all three backends.
- `CompilerConfig { inline: InlineConfig, .. }` lives in
  `lpir/src/compiler_config.rs` with `apply(key, value)` plus `FromStr` for
  `InlineMode`. `InlineConfig` has all the knobs the M3 doc calls for
  (`mode`, `always_inline_single_site`, `small_func_threshold`,
  `max_growth_budget`, `module_op_budget`).
- `FunctionBuilder` already has `push_block` / `push_exit_block` / `end_block`,
  so the inliner's emitted IR is constructable through normal channels (good
  for tests).
- `IrFunction` shape: flat `body: Vec<LpirOp>`; per-function `vreg_types`,
  `slots`, `vreg_pool`. `vmctx_vreg = VReg(0)`, user params at `v1..=v(param_count)`.
- `Call { callee, args, results }`: `args` is a `VRegRange` into the caller's
  `vreg_pool` and **includes vmctx as the first entry** (so for a callee with
  `param_count = N`, `args.count = 1 + N`). `results` does not include vmctx.
- `LpirOp::SlotAddr` is the **only** op that references a `SlotId` (slot remap
  is therefore very targeted).

### What's missing

- No `lpir::inline` module exists today (`Glob lpir/src/inline*` is empty).
- `LpirOp` has no general "iterate uses / remap vregs" helper — only
  `def_vreg()`. The inliner needs a `for_each_vreg_mut` (or equivalent
  per-arm rewrite). const_fold avoids this by replacing-in-place without
  remap.
- `validate_module` has no recursion check today; the M3 doc assumes
  recursion is forbidden upstream (GLSL frontend), but our inliner must
  defend itself anyway because a malformed test or hand-written LPIR could
  contain it. We detect cycles in topo-sort and skip those nodes.
- No offset-recompute helper exists today. The builder patches offsets as it
  goes; const_fold preserves length so doesn't need it. We have to write one.

### Existing call-overhead context

`rainbow.glsl` is the canonical perf target (many tiny helper calls). M3
doesn't measure perf — that's M4 — but the design must allow significant
shrinkage there. Per-call overhead on rv32n.q32 is ~18-24 instructions today.

### Pipeline integration (preview)

`lpvm-native/src/compile.rs::compile_module` clones the IR module before
per-function compilation; that's the natural place to insert
`inline_module(&mut ir_opt, &options.config.inline)` once M4 lands. We don't
modify `compile.rs` in this stage; the unit tests call `inline_module`
directly.

## Questions

### Q1: Where to compute callee body length for the heuristic, and what counts as an "op"?

**Context.** `InlineConfig::small_func_threshold` and `max_growth_budget` are
phrased in "ops". Some `LpirOp` variants are pure markers (`Else`, `End`,
`Break`, `Continue`, `ExitBlock`, the `*Start` openers); some lower to many
machine instructions (`Call` to an import, `Memcpy`). Definition matters for
threshold tuning later.

**Resolution.** Land M3 with the simplest possible metric and defer
weighting to a small empirical follow-up:

- Single private function `func_weight(&IrFunction) -> u32` whose body is
  `f.body.len() as u32`. The heuristic and budgets all go through it.
- Tracked as **M3.1** (`docs/roadmaps/2026-04-15-lpir-inliner/m3.1-tune-inline-weights.md`):
  build a small `filetests/debug/inline-weights.glsl` corpus, dump
  `lp-cli shader-debug --lpir --asm`, tabulate `lpir_ops` vs candidate
  `weighted_ops` vs `rv32n_insns`, pick the simplest weighting that
  correlates well, swap the body of `func_weight`, retune
  `small_func_threshold`. Independent of M4 (no inliner wiring required).
- Default `small_func_threshold` stays at 20 in M3; M3.1 will revise.

### Q2: How to lay out the `inline` module — single file or submodule?

**Context.** The roadmap says `lpir/src/inline.rs`. The transform has several
distinct concerns: call-graph build, topo order, vreg/slot/pool remap,
splice, offset recompute, heuristic. Keeping them in one file is fine if it
stays under ~600 lines, otherwise it gets unwieldy.

**Resolution.** Submodule layout:

```
lpir/src/inline/
├── mod.rs         # public API: inline_module, InlineResult; orchestration
├── callgraph.rs   # build callees-of / callers-of, topological order, cycle detection
├── remap.rs       # VReg + SlotId + vreg_pool remap helpers
├── splice.rs      # body cloning + multi-return Block/ExitBlock wrapping
├── offsets.rs     # single-pass offset recompute (reusable)
└── heuristic.rs   # InlineConfig decisions, func_weight, budget accounting
```

Each helper file is small and individually unit-testable. Tests live in
`lpir/src/tests/inline_*.rs` mirroring the `block_ops.rs` pattern.

### Q3: Recursion / cycle handling — error or skip silently?

**Context.** GLSL forbids recursion, so the frontend should never produce a
cycle. But the inliner gets handed an `LpirModule`, not GLSL. The M3 doc
says "If cycles exist (shouldn't in GLSL — recursion is forbidden), skip
them." There's no `ValidationError::Recursion` today.

**Resolution.** Skip silently and log at `debug!`. Detect cycles by spotting
any function that remains unprocessable once all leaves are exhausted in the
topological walk; leave its `Call` ops untouched. Record the count in
`InlineResult.functions_skipped_recursive` for visibility. Other (non-GLSL)
frontends writing to LPIR are theoretically possible, so failing hard would
be punishing — defense-in-depth without breakage. Adding a validator check
belongs in a separate change.

### Q4: When the call-site arg vreg already matches the remapped param, do we still emit `Mov`?

**Context.** The roadmap's 3a says "`v1..v(param_count)` → map to the actual
argument vregs from the `Call`'s `args` range" — i.e., **no `Mov`**, the
remap table just aliases the callee param vreg to the caller arg vreg. The
roadmap's 3c then says "Argument moves: For each user parameter, emit `Mov
{ dst: remapped_param_vreg, src: arg_vreg }`. (If remapping maps params
directly to arg vregs, these can be skipped.)" These two statements are
consistent only if you pick one strategy.

**Resolution.** Per-param scan-then-alias-or-copy. LPIR is **not SSA** and
the frontend's `param_aliases` optimization (`lps-frontend/src/lower_ctx.rs`
`scan_param_argument_indices`) deliberately makes by-value GLSL params
mutable in LPIR — `t = t * 2.0` lowers to `v1 = fmul v1, v2_const` writing
the param vreg in place. Blind aliasing (strategy A) is therefore a
correctness bug. Blanket copying (strategy B) is safe but leaves easy
performance wins on the table (constant args don't const-fold through the
inserted `Copy`).

The scan:

```rust
fn scan_param_writes(callee: &IrFunction) -> Vec<bool> {
    let n = 1 + callee.param_count as usize;
    let mut written = vec![false; n];
    for op in &callee.body {
        if let Some(v) = op.def_vreg() {
            let i = v.0 as usize;
            if i < n { written[i] = true; }
        }
        if let LpirOp::Call { results, .. } = op {
            for v in callee.pool_slice(*results) {
                let i = v.0 as usize;
                if i < n { written[i] = true; }
            }
        }
    }
    written
}
```

Per-param remap decision:

- `remap[0] = caller_vmctx_vreg` always (vmctx is opaque pointer; user code
  never writes it; `debug_assert!(!written[0])`).
- For each user param `i`:
  - `written[1 + i] == false` → alias `remap[1 + i] = caller_arg_vreg[1 + i]`.
    Zero overhead, const-fold sees through.
  - `written[1 + i] == true`  → allocate fresh vreg in caller, prepend
    `Copy { dst: fresh, src: caller_arg }` to spliced body, set
    `remap[1 + i] = fresh`. Correctness guaranteed.
- `remap[rest] = fresh` (callee locals always get fresh caller vregs).

Properties: O(n) one extra pass per callee, ~50 LOC, tested via dedicated
unit tests (`scan_param_writes_*`, `inline_aliases_readonly_params`,
`inline_copies_mutated_param_only`). Bottom-up traversal keeps the analysis
correct even for callees that already had their own callees spliced in —
splices add fresh vregs only, never write to the outer callee's params.

### Q5: Use `LpirOp::Copy` or `LpirOp::Mov` for the return-value plumbing?

**Context.** I keep saying "Mov" but LPIR's actual move op is `LpirOp::Copy
{ dst, src }` (verified in `lpir_op.rs` and `const_fold.rs`). There is no
`Mov`.

**Resolution.** Use `LpirOp::Copy` everywhere — for the per-param
pre-copies (Q4) and for the result moves at the end of the inlined body.
No new opcode. Mentally substitute `Copy` wherever the M3 doc says `Mov`.

### Q6: Multi-return wrapping — when exactly do we need `Block` / `ExitBlock`?

**Context.** The M3 doc says "If the callee has exactly one `Return` at the
end, no `Block`/`EndBlock` wrapper is needed." Otherwise we wrap the body
in `Block { end_offset: _ }` and rewrite each `Return` as
"copies to caller results, then `ExitBlock`". The trailing `End` falls
through to the post-call moves.

**Resolution.** Three cases, decided by a single piggybacked scan on the
callee body (same pass as Q4's `scan_param_writes` — count `Return` ops,
note the position of the last one):

| Callee return shape | Splice strategy |
|--|--|
| **0 returns** (void) | Splice body. No wrapper. No result `Copy`s. |
| **Exactly 1 `Return` and it's the last op** | Splice body without the trailing `Return`. Replace it with `Copy { dst: caller_result_vreg[k], src: remap[callee_return_vreg[k]] }` for each return value. No wrapper. **Most common case.** |
| **≥1 `Return`, not the unique-final pattern** | Emit `Block { end_offset: 0 }`. Splice body; replace each `Return` with the result `Copy`s followed by `ExitBlock`. Close with `End`. Caller's fall-through is the op after `End`. |

Notes:

- The `end_offset` on the opened `Block` gets patched by the offset-recompute
  pass (Q10), not the splicer — splicer emits `Block { end_offset: 0 }`.
- "1 return at the end of the body" is the GLSL pattern for almost every
  helper (`paletteHeatmap`, `paletteRainbow`, `applyPalette`'s arms, etc.),
  so the wrapper-free path is the hot one.
- Multi-return case correctly handles GLSL early-return idioms
  (`if (cond) return X; ... return Y;`).

### Q7: Do we re-validate after inlining inside the pass, or trust the contract?

**Context.** const_fold doesn't re-validate. But inlining does much more
structural work and is much easier to get subtly wrong (offset patching,
vreg remap arity, slot count).

**Resolution.** Tiered validation:

- **Production callers (M4 wiring):** no `validate_module` after the pass —
  doubles work for no benefit; the pass owns its output's correctness.
- **Unit tests:** always call `validate_module` after `inline_module`. Cheap
  insurance with good error messages.
- **Inside the pass:** `debug_assert!`s on internal invariants the validator
  doesn't know about (remap table size = `callee.vreg_types.len()`,
  control-flow stack empty at end of offset recompute, pool splice arity
  matches, vmctx slot of `written` bitset is `false`, etc.). Free in
  release, loud in debug.

### Q8: Bottom-up order — what when a function calls itself indirectly via an import?

**Context.** Imports are external; we never inline them. Calls to imports
are leaves of the local call graph regardless of what the import does.

**Resolution.** The call graph only tracks `CalleeRef::Local` edges. Import
`Call` ops are leaves; they stay as-is in the inlined body with `vreg_pool`
entries remapped and appended (Q9). LPIR has no re-entrant import path
today, and even if a host did re-enter, we'd have no IR to optimize against
— so this is the only sensible policy.

### Q9: How do we splice the callee's `vreg_pool` entries safely?

**Context.** The callee's body contains `Call` ops (to imports — local ones
are already inlined since we go bottom-up) and `Return` ops, both of which
reference `vreg_pool` slices via `VRegRange { start, count }`. When we
copy the callee's body into the caller, those `start` offsets are wrong.

**Resolution.** Single linear pass through the callee body, cooperating
with the splicer's main loop:

- **`Call { callee, args, results }`** — read both callee pool slices,
  remap each `VReg`, append remapped vregs to the *caller's* `vreg_pool`.
  Rewrite the op with `start = new pool position`; counts unchanged.
- **`Return { values }`** — never appears in spliced body verbatim. Read
  the callee pool slice once, remap, use values directly to emit result
  `Copy`s (and `ExitBlock` in multi-return case per Q6). Nothing appended
  to caller's pool for this op.
- **All other ops** — no pool references. Just remap `VReg` fields in
  place.

Implementation pattern: emit spliced ops into a `Vec<LpirOp>` scratch
buffer, growing `caller.vreg_pool` as we go. Then a single `splice` on
`caller.body` replaces the original `Call` op with the scratch contents.
Pool entries become valid the moment the scratch op gets its `start`
offset, so there's no "patch start offsets after the fact" step.

The caller's existing pool entries (for ops outside the spliced range) are
unaffected — `vreg_pool` is append-only from the inliner's POV.

### Q10: How do we recompute control-flow offsets after splicing?

**Context.** After splicing, every `IfStart`, `LoopStart`, `SwitchStart`,
`CaseStart`, `DefaultStart`, and `Block` op may have stale `else_offset` /
`end_offset` / `continuing_offset` values, since we've inserted ops.

**Resolution.** Fully structural recompute pass, made possible by the
**M2.5 prerequisite** (`docs/roadmaps/2026-04-15-lpir-inliner/m2.5-continuing-marker.md`):

- M2.5 adds `LpirOp::Continuing` as a marker op so loops have parity with
  if-else (which has the `Else` marker). Backends keep using the cached
  `LoopStart::continuing_offset` field unchanged. The marker is purely so
  any pass that reshapes the body (today: the inliner) can rebuild every
  cached offset structurally with no special cases.
- M3 then ships `inline/offsets.rs` with one function:

```
fn recompute_offsets(body: &mut [LpirOp]):
  stack: Vec<(Kind, idx)> = []
  for (i, op) in body.iter().enumerate():
    match op:
      IfStart       -> push (If, i)
      LoopStart     -> push (Loop, i, continuing=None)
      SwitchStart   -> push (Switch, i, pending_case=None)
      Block         -> push (Block, i)
      Else          -> top must be (If, i0); body[i0].else_offset = i;
                       replace top with (Else, i0)
      Continuing    -> top must be (Loop, i0, _); store i in stack frame
      CaseStart     -> patch top.pending_case.end_offset = i;
                       set top.pending_case = i
      DefaultStart  -> same
      End           -> pop top:
        (If, i0)        -> body[i0].else_offset = i; body[i0].end_offset = i+1
        (Else, i0)      -> body[i0].end_offset = i+1
        (Loop, i0, c)   -> body[i0].continuing_offset = c.unwrap_or(i0+1);
                            body[i0].end_offset = i+1
        (Switch, i0, p) -> patch p.end_offset = i (if any);
                            body[i0].end_offset = i+1
        (Block, i0)     -> body[i0].end_offset = i+1
  debug_assert!(stack.is_empty())
```

Single forward pass, O(body.len()), small stack only allocation. Lives in
`inline/offsets.rs`. Reusable by any future structural transform.

The M3 plan **depends on M2.5 landing first** — M2.5 is a small,
mechanical change (~9 files, similar shape to M2 itself).

### Q11: How do we handle the heuristic budgets (multi-call-site growth)?

**Context.** `max_growth_budget` caps total growth from multi-site
inlining; `module_op_budget` aborts entirely if module total exceeds it.
Single-site inlining is always free in code-size terms because the original
will (eventually) be deleted by M5.

**Resolution.** Per-callee in topological (bottom-up) order:

- `body_size = func_weight(callee)` (M3: `body.len()`; M3.1 will tune).
- `local_call_sites = #callers in callgraph`.
- `extra_growth = max(0, local_call_sites - 1) * (body_size - 1)`
  (first site is "free" because the original gets pruned by M5; each
  subsequent site replaces a `Call` op with `body_size` ops).

Decision in `heuristic.rs::should_inline(callee_id, callgraph, config,
growth_used) -> Decision` returning the verdict + projected delta:

```
match config.mode:
  Never  -> Skip("config: mode=never")
  Always -> Inline { extra_growth: 0 }     // budgets ignored
  Auto:
    if local_call_sites == 0:
        return Skip("no callers")
    if local_call_sites == 1 && always_inline_single_site:
        return Inline { extra_growth: 0 }   // single-site is free
    if body_size <= small_func_threshold:
        return Inline { extra_growth }      // small enough regardless
    if let Some(budget) = max_growth_budget:
        if growth_used + extra_growth > budget:
            return Skip("max_growth_budget exhausted")
    Inline { extra_growth }
```

Caller updates `growth_used += extra_growth` only on `Inline`.

`module_op_budget`: check the running sum of all functions' `func_weight`
before processing each callee. If exceeded, set
`result.budget_exceeded = true` and stop the pass entirely. Bottom-up
order means we've already done the leaves (highest leverage), so a
partial result is still useful.

**Debug logging.** At `log::debug!` level in the orchestration loop, emit
one line per decision so a future debugging session has a paper trail:

```
[lpir-inline] callee=@paletteHeatmap (id=3) sites=4 size=14
              decision=inline reason=small_func_threshold
              extra_growth=39 growth_used=0 -> 39
[lpir-inline] callee=@bigHelper (id=11) sites=3 size=180
              decision=skip   reason=max_growth_budget_exhausted
              would_grow=358 budget=300 used=212
[lpir-inline] callee=@only_caller_helper (id=7) sites=1 size=92
              decision=inline reason=single_site
              extra_growth=0
```

Plus a single `log::info!` summary at the end:

```
[lpir-inline] inlined 12 functions across 38 call sites,
              skipped 2 (1 recursive, 1 over budget),
              growth_used=412 / module_total=2104 ops
```

The structured fields make it grep-friendly without needing a parser.
Logging lives in `inline/mod.rs` (the orchestrator), not in
`heuristic.rs` — the heuristic returns enough info (`Decision` carries
the reason) for the orchestrator to log.

### Q12: Result shape — what does `InlineResult` track?

**Context.** Roadmap declares:

```rust
pub struct InlineResult {
    pub functions_inlined: usize,
    pub call_sites_replaced: usize,
    pub budget_exceeded: bool,
}
```

**Resolution.** The roadmap shape plus `functions_skipped_recursive`:

```rust
pub struct InlineResult {
    /// Distinct callees whose body was spliced into ≥1 caller this run.
    pub functions_inlined: usize,
    /// Total `Call` ops replaced.
    pub call_sites_replaced: usize,
    /// Distinct functions skipped due to call-graph cycles (Q3).
    pub functions_skipped_recursive: usize,
    /// True iff `module_op_budget` was hit and the pass stopped early (Q11).
    pub budget_exceeded: bool,
}
```

No `Result<_, InlineError>` — we never hard-error (Q3 silently skips
recursion; Q11 signals budget overrun via the field, not an error).

### Q13: How do we want to test? In-process LPIR or via parser?

**Context.** Tests can build `LpirModule` either via `ModuleBuilder` (Rust
API, terse, type-safe) or by parsing LPIR text (matches what production
sees). M2 tests parsed text for round-trip and built directly for in-depth
work.

**Resolution.** Mix per concern, all in-process LPIR (no GLSL compile):

```
lpir/src/tests/
├── inline_basic.rs        # parser-based: void, single-return, multi-return, nested
├── inline_callgraph.rs    # builder-based: cycles, diamond (A→B,C; B→C), chains
├── inline_remap.rs        # parser-based: vmctx alias, slot remap, pool splice via imports
├── inline_heuristic.rs    # builder-based: thresholds, budgets, mode=Never/Always/Auto
├── inline_offsets.rs      # builder-based: hand-built bodies, run recompute_offsets, assert
└── inline_param_writes.rs # parser-based: read-only params alias, mutated params copy (Q4)
```

All wired via `lpir/src/tests.rs`. Pattern matches `block_ops.rs`:
parse → inline → validate → interp → assert.

**GLSL filetests are M4.** That's where `compile_module` gets the inliner
wired in and we get end-to-end semantic coverage on real shaders
(`rainbow.glsl` etc.) and where `// compile-opt(inline.mode, …)`
annotations come into play.

### Q14: Should `inline_module` clone the input or always mutate?

**Context.** The roadmap's signature is `inline_module(&mut LpirModule,
&InlineMode)`. `lpvm-native::compile_module` already does
`let mut ir_opt = ir.clone();` before per-function compile.

**Resolution.** Take `&mut LpirModule` as the roadmap declares. Mutating
in place is critical for embedded targets where every clone of an
`LpirModule` is a real cost on a constrained heap. The caller (M4 wiring)
clones once at the start of `compile_module` if they need the original
preserved; the inliner does no internal cloning of the module structure.

Future optimization (out of scope for M3, captured here for M5): delete
orphaned functions as we go to keep peak memory low — currently a fully
inlined helper sticks around in `LpirModule.functions` until M5's
DeadFuncElim runs separately. Inline-and-delete-as-we-go would be one
pass instead of two and would lower peak module size during compilation,
which matters for big shaders on the ESP32. Stays as a separate pass for
now to keep the inliner focused and the M5 deletion logic reusable.

### Q15: Naming — `inline_module` vs `run`?

**Context.** Other LPIR passes use snake_case verbs (`fold_constants`,
`validate_module`, `parse_module`).

**Resolution.** `pub fn inline_module(module: &mut LpirModule, config:
&InlineConfig) -> InlineResult`. Re-exported from `lpir/src/lib.rs` so
callers say `lpir::inline_module(..)`, matching `lpir::validate_module`,
`lpir::parse_module`, `lpir::print_module`, `lpir::interpret`.

## Notes

- The roadmap says the `mode` parameter is `&InlineMode` in one place and
  `&InlineConfig` in another. Use `&InlineConfig` (it's the richer struct
  and includes `mode`) — that matches the M4 wiring snippet
  (`&options.config.inline`) which is the production caller.
- `InlineConfig` has no `Default` impl issue — it's already there.
- The M3 doc mentions `EndBlock`; M2 closed `Block` with the existing `End`
  op instead. Treat all "EndBlock" mentions in the M3 doc as `End`.
- We do **not** delete or rename any function in this stage. After
  `inline_module`, every `IrFunction` previously in the module is still
  present, with the same `FuncId`. Functions that were fully inlined now
  have zero remaining callers but are still compilable; M5 will prune them.
- Const-fold runs per-function *after* inlining (M4 pipeline). Inlining
  exposes new constants (e.g. `paletteHeatmap(0.0)`), so this is the
  intended order. M3 doesn't need to invoke const_fold itself.

## Execution notes (implementation vs plan)

Appendix for Phase 7 — deviations and concrete choices during build-out:

- **`topo_order` direction:** Kahn’s algorithm uses **in-degree = number of distinct local callees** per function. The queue seeds functions with in-degree 0 (no local calls). Peeling a callee decrements its callers’ in-degrees. The resulting `Vec` is **bottom-up** (leaves first), matching the design intent; early sketches that treated “out-degree” were corrected during implementation.

- **Adjacency keyed by `BTreeMap`:** `callees_of`, `callers_of`, and `call_sites_of` use `BTreeMap<FuncId, …>` for deterministic iteration order (stable tests and logs).

- **`Decision::SkipBudget`:** Split budget motivation into `BudgetReason` (`MaxGrowth` vs `ModuleTotal`) so the orchestrator can set `budget_exceeded` only when the **module total** cap trips (multi-site growth cap does not abort the whole pass).

- **Multi-return `Block`:** The splicer emits **`ExitBlock` after each rewritten `Return`**, and ensures a trailing **`ExitBlock`** before **`End`** when the last body op is not already an exit (so `Block` always pairs with `ExitBlock` + `End` as required by LPIR structure).

- **Param scan / remap:** `scan_param_writes` tracks **only defs via `def_vreg()`** for user params (`v1..=vN`); vmctx is asserted never defined. Read-only params **alias** caller arg vregs; written params get a fresh vreg plus a leading **`Copy`**. Callee locals and appended slots get fresh caller indices; import `Call` pool slices are remapped in `remap_op`.
