# LPIR inlining pass

Function inlining for LPIR. Lives in `lp-shader/lpir/src/inline/`, exposed as
`lpir::inline_module(&mut LpirModule, &InlineConfig) -> InlineResult`.

## Goals

1. **Reduce call overhead** on the rv32n target. Local LPIR calls lower to a
   prologue / argument shuffle / `jal` / epilogue per call site; for tiny
   helpers this overhead dominates the body.
2. **Enable downstream constant folding.** Inlined parameters often become
   constants at the call site, opening folding and dead-code opportunities
   the const-fold pass alone cannot reach across a call boundary.
3. **Stay embedded-friendly.** The pass is mutative (in-place), allocation-
   bounded, and uses `BTreeMap` / `Vec` — no recursion in the algorithm,
   no large temporaries.

Non-goals: cross-module inlining (imports are never inlined), inlining
through indirect calls (LPIR has none), removing functions that became
unreachable after inlining (handled separately by a future pass).

## Algorithm

Bottom-up over the local call graph. Each callee is considered exactly
once, after every function it calls has been processed.

1. **Build the call graph** from `LpirOp::Call` ops. Imports are excluded
   (`CalleeRef::Import` does not introduce an edge). The graph stores
   callees-of, callers-of, and `(op_idx, callee)` call sites per caller —
   all keyed by `BTreeMap<FuncId, …>` for deterministic iteration over a
   sparse `FuncId` space.
2. **Topological sort** (Kahn's, leaves first). Nodes with `callees_of[g]
   == 0` come first; cycles are extracted separately and reported as
   `functions_skipped_recursive`. Isolated functions (no incoming or
   outgoing local calls) are still emitted in the order so that orphan
   leaves are not lost.
3. For each callee in topo order:
   - Apply the [heuristic](#heuristic) to decide whether to inline.
   - If yes, [splice](#splicer) the callee body into every caller call
     site. The callee `IrFunction` itself is left in the module.
4. After the loop, recompute control-flow offsets once per mutated caller
   (see [offset recompute](#offset-recompute)).

The callee is *not* deleted after inlining — call sites are replaced but
the body remains addressable via `FuncId`. A future "dead function" pass
may sweep what is no longer reachable from entry points.

## Splicer

`splice::inline_call_site(caller, callee, call_op_idx)` replaces a single
`LpirOp::Call` with a remapped copy of the callee body.

### Steps

1. **Arity check** between the call's `args` / `results` and the callee's
   `param_count` / `return_types`. Mismatch is a no-op (`debug_assert!`
   in debug builds).
2. **Param-write scan** (`scan_param_writes`) walks the callee body and
   marks any parameter VReg that is the destination of any op via
   `LpirOp::def_vreg`. Read-only params can be aliased; written params
   need a private copy.
3. **Build the remap** (`build_remap`):
   - `vreg_table[0]` always maps to `VMCTX_VREG` in the caller (`vmctx`
     is a process-wide singleton; aliasing is safe and required for
     pointer identity through chained calls).
   - Each read-only param maps to the matching argument VReg in the
     caller (alias).
   - Each mutated param allocates a fresh caller VReg of the callee's
     type, emitted as a leading `LpirOp::Copy { dst: new, src: arg }`
     in the spliced scratch.
   - Non-param VRegs map to fresh caller VRegs of matching type.
   - Slots are translated by `slot_offset = caller.slots.len()` after
     extending `caller.slots` with the callee's slots.
   - `vreg_pool` ranges from the callee are appended to
     `caller.vreg_pool` and recorded as a base offset for `VRegRange`
     translation.
4. **Classify return shape** of the callee body:
   - `None` — body has no `LpirOp::Return`.
   - `SingleAtEnd` — exactly one `Return` and it is the last op.
   - `Multi` — anything else (early returns or multiple returns).
5. **Build the scratch** `Vec<LpirOp>`:
   - Emit `Copy` ops for each mutated parameter.
   - Walk the callee body, emit `remap_op(op)` for each non-`Return`.
     `Return` ops are emitted as the appropriate `Copy`s into the
     caller's `results` VRegs:
     - `SingleAtEnd` and `None`: a flat sequence of `Copy { dst: results[i], src: ret_vals[i] }`.
     - `Multi`: the entire spliced body is wrapped as
       `Block { end_offset: 0 } … ExitBlock End` and each in-body
       `Return` becomes the `Copy` sequence followed by `ExitBlock`.
       This preserves early-exit semantics in structured control flow.
6. **Splice** the scratch into `caller.body` at `call_op_idx`,
   replacing the `Call` op (`Vec::splice` of length 1).

`end_offset` fields on `Block` / `IfStart` / `LoopStart` and the `Switch`
family are left set to `0` in the splicer; the [offset recompute](#offset-recompute)
fixes them after all splicing for that caller is done.

### Why scan-then-alias-or-copy

GLSL by-value parameters are mutable inside the function. A naive "always
copy" strategy spends `Copy` ops the const-folder can rarely remove. A
naive "always alias" strategy is unsound when the callee writes through
the param. The scan is `O(callee.body.len())` and is the cheapest way to
get aliasing for the common read-only case (the majority of helpers) and
correctness for the rest.

`vmctx` (`VReg(0)`) is a special case: it is never written by any
function and aliases unconditionally.

## Offset recompute

Control-flow ops carry cached offsets — `IfStart::else_offset`,
`IfStart::end_offset`, `LoopStart::end_offset`,
`LoopStart::continuing_offset`, `Block::end_offset`,
`SwitchStart::end_offset`, etc. Splicing inserts ops at arbitrary
positions and invalidates every offset in or around the spliced range.

Rather than thread incremental fixups through the splicer,
`offsets::recompute_offsets(&mut Vec<LpirOp>)` runs once per mutated
caller after all splicing for that caller is complete. It does a
single stack-walk of the body and re-derives every offset structurally,
matching `FunctionBuilder` conventions.

This requires structural markers for every control region. The
`continuing` block of a loop previously had only a cached
`continuing_offset` and no marker op, which made structural recompute
ambiguous. Stage III (M2.5) added [`LpirOp::Continuing`](#continuing-marker)
to fix this.

### `Continuing` marker

`LpirOp::Continuing` is emitted at the start of a loop's continuing
block. Backends still consume `LoopStart::continuing_offset` for fast
branch-target lookup; the marker is what lets the recompute pass
re-derive that cached value structurally. The marker is a no-op at
runtime and lowers to nothing on every backend.

## Configuration

`InlineConfig` (`lp-shader/lpir/src/compiler_config.rs`):

| field | default | meaning |
|---|---|---|
| `mode` | `Auto` | `Never` skips everything; `Always` ignores the size threshold; `Auto` consults `small_func_threshold`. |
| `always_inline_single_site` | `true` | When `Auto`, inline a callee that has exactly one call site even if it is over `small_func_threshold`. |
| `small_func_threshold` | `16` | Maximum `func_weight` for "small" callees that are inlined unconditionally under `Auto`. See [empirical tuning](#empirical-tuning). |
| `max_growth_budget` | `None` | Per-callee cap on `weight × callsite_count`; on overflow the callee is skipped and processing continues. |
| `module_op_budget` | `None` | Module-wide cap on total ops projected after inlining a callee; on overflow the pass stops early and `InlineResult::budget_exceeded = true`. |

Fields are settable via `compile-opt inline.<field> = <value>` directives
in shader source.

## Heuristic

`should_inline(weight, callsite_count, current_module_op_count, config)`
returns one of:

| decision | when |
|---|---|
| `Inline` | All gates pass. |
| `SkipMode` | `mode == Never`. |
| `SkipTooLarge { weight, threshold }` | `Auto`, `weight > threshold`, and not (single call site with `always_inline_single_site`). |
| `SkipBudget { reason: MaxGrowth, … }` | `weight × sites > max_growth_budget`. Per-callee skip; pass continues. |
| `SkipBudget { reason: ModuleTotal, … }` | Projected module ops would exceed `module_op_budget`. Pass stops; further callees not considered this run. |

The two skip-budget variants behave differently because per-callee
budgeting is a local decision (other callees may still fit), while
module-total budgeting is monotonic over remaining work — there is no
point continuing once we've crossed it.

## `func_weight`

Production weight is the simplest possible:

```rust
fn func_weight(func: &IrFunction) -> usize {
    func.body.len()
}
```

Three candidates were evaluated empirically in M3.1; all three remain
public under `lpir::inline_weights::{weight_body_len, weight_markers_zero, weight_heavy_bias}`
and a `WeightKind` dispatcher, retained for re-tuning when the cost
model shifts (e.g. switching to a different rv32 backend).

| candidate | rule | combined Pearson r vs `rv32n_insns` |
|---|---|---|
| `body_len` (production) | `func.body.len()` | **0.980** |
| `markers_zero` | All ops weight 1 except structural markers (`IfStart`, `Else`, `Continuing`, `LoopStart`, `*Start`, `End`, `Block`, `ExitBlock`, `Break`, `Continue`, `Return`) which weight 0. | 0.974 |
| `heavy_bias` | `markers_zero` + `Call=5`, `Memcpy=4`, `Fsqrt=4`, `Fdiv`/`IdivS`/`IdivU`/`IremS`/`IremU`=3. | 0.962 |

`body_len` won linear correlation and is the simplest. `markers_zero`
adds branching for negligible gain — structural ops are a small fraction
of body length for typical code. `heavy_bias` over-penalizes single-cycle
hardware ops like `FSQRT.S` on the rv32n backend; the resulting weight
distorts the cliff at which the threshold sits.

## Empirical tuning

`small_func_threshold = 16` was picked from the M3.1 corpus
(`lp-shader/lps-filetests/filetests/debug/inline-weights.glsl` plus the
existing `rainbow.glsl`) by mapping `body_len` to measured rv32n
instruction count. Selected representative rows:

| function | body_len | rv32n insns |
|---|---|---|
| `iw_clamp01` | 7 | 25 |
| `iw_lerp` | 10 | 33 |
| `iw_mul3` | 12 | 46 |
| `iw_add3` | **16** | 51 |
| `iw_fold_rgb` | 18 | **85** |
| `paletteFire` | 22 | 104 |
| `applyPalette` | 42 | 148 |
| `rainbow_main` | 154 | 541 |

`body_len ≤ 16` cleanly captures every corpus function that lowers to
≤ ~50 rv32n insns (well under the M3.1 target of ≤ 64) without picking
up `iw_fold_rgb` at 85.

Re-tune by running `lp-cli shader-debug --weights …` against the
corpus; the flag emits `body_len` / `mz` / `hb` columns next to the
existing rv32n / rv32c counts. See
`docs/roadmaps/2026-04-15-lpir-inliner/m3.1-tune-inline-weights.md` for
the methodology.

## Recursion

Local call graphs may contain cycles (GLSL 4.50 permits recursion).
The inliner detects cycles during the topological sort and counts
their members in `InlineResult::functions_skipped_recursive`. Bodies
of recursive functions are not modified.

Imports never participate in the call graph and are never inlined.

## Determinism

All adjacency structures are `BTreeMap<FuncId, …>` and call-site lists
are sorted by op index (descending, so splicing earlier sites does not
shift later ones). Topological sort, splicer, and offset recompute are
all deterministic functions of the input module. Re-running
`inline_module` on identical input yields byte-identical output.

## Logging

Decisions and a per-run summary are emitted via the `log` crate at
`debug` and `info` levels. Embedded builds depend on `log` with
`default-features = false`; the calls compile to no-ops when no logger
is installed.

```
inline: callee=FuncId(3) weight=12 sites=2 module_ops=87 decision=inline
inline: callee=FuncId(7) skip too_large weight=42 threshold=16
inline: callee=FuncId(9) skip budget projected=400 budget=300 reason=ModuleTotal
inline: done inlined=4 sites=11 skipped_recursive=1 budget_exceeded=false
```

## File layout

```
lp-shader/lpir/src/inline/
├── mod.rs          # InlineResult, inline_module orchestration
├── callgraph.rs    # CallGraph, build, topo_order
├── heuristic.rs    # func_weight, weight candidates, should_inline
├── offsets.rs      # recompute_offsets
├── remap.rs        # ParamWriteMask, scan_param_writes, Remap, remap_op
└── splice.rs       # inline_call_site
```

Public surface from `lpir`:

```rust
pub fn inline_module(&mut LpirModule, &InlineConfig) -> InlineResult;
pub struct InlineResult { … }            // counters
pub mod inline_weights {                 // M3.1 candidates, re-tuning
    pub enum WeightKind { BodyLen, MarkersZero, HeavyBias }
    pub fn weight(WeightKind, &IrFunction) -> usize;
    pub fn weight_body_len(&IrFunction) -> usize;
    pub fn weight_markers_zero(&IrFunction) -> usize;
    pub fn weight_heavy_bias(&IrFunction) -> usize;
}
```

Everything else (`CallGraph`, `Remap`, `splice::*`, `should_inline`,
`Decision`) is `pub(crate)`.

## Alternatives considered

### Top-down inlining

Walking from entry points down would let the heuristic see specialized
parameters (constants flowing through) before deciding. It would also
make budget accounting easier (you stop when you hit the budget at any
depth). Bottom-up was chosen because it composes: by the time we
consider `f`, every callee inside `f` has already been processed, so
`weight(f)` reflects the *post-inline* size of `f`. Top-down would
require either a fixed-point loop or per-call-site re-evaluation.

### Inlining-with-deletion

Removing a callee `IrFunction` from the module after every call site
has been spliced would shrink the module and reduce subsequent
serialization cost. It would also require fixing every other reference
to that `FuncId` (none exist in LPIR today, but a future pass could
add them) and would make incremental recompilation harder. The chosen
design leaves the function in place; a separate "dead function" pass
can sweep unreachable functions when needed.

### Per-call-site cost model

A more accurate heuristic would weight each call site by the cost of
the surrounding call (argument shuffle, return-value movement) so that
a 3-op leaf inlined twenty times in the same loop is preferred over a
3-op leaf called once in cold code. The current pass treats every site
uniformly. The simpler model is sufficient at present module sizes;
revisiting requires a profile-driven workflow that does not yet exist.

### Smarter weight functions

`weight_markers_zero` and `weight_heavy_bias` were designed to better
predict rv32n instruction count. Empirically (M3.1) they did not beat
`body.len()` as a linear predictor, and `heavy_bias`'s non-linearity
distorts the threshold cliff in the wrong direction (over-penalizing
fast hardware ops like `FSQRT.S`). They remain available as public
candidates so a future cost-model change (different backend, SIMD
expansion, etc.) can be evaluated without re-deriving the
infrastructure.
