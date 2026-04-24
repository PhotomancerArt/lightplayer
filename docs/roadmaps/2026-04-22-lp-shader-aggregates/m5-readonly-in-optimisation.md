# Milestone 5 — Read-only-`in` optimisation

## Status

**Complete** — 2026-04-23. Implementation and notes under
`docs/roadmaps/2026-04-22-lp-shader-aggregates/m5-readonly-in-optimisation/`
(`plan.md`, `summary.md`). Measurement notes in `m5-bench.md`.

## Goal

Recover the per-call slot-allocation + `Memcpy` cost paid by `in`
aggregate parameters whose callee never mutates them. Pre-scan each
function body; for any `in` aggregate param the callee proves it
doesn't write through, elide the slot allocation and entry-`Memcpy` and
let member access `Load` directly from the caller's slot via the
arg-pointer. Applies uniformly to all aggregates (arrays, structs,
arrays-of-structs).

## Suggested plan name

`lp-shader-aggregates-m5-readonly-in-optimisation`

## Scope

### In scope

- New module `lp-shader/lps-frontend/src/readonly_in_scan.rs`. Per-
  function pre-pass that walks the body and, for each `in` aggregate
  param, decides:
  - **Read-only** if the param's `LocalVariable` pointer is never the
    `pointer` operand of a `Statement::Store` *and* never passed as an
    `inout`/`out` arg to a callee.
  - **Mutable** otherwise. (Conservative: any "passed onward as
    inout/out" forces mutable, even if the callee turns out to be
    read-only — full interprocedural propagation is out of scope.)
- Wire the read-only flag into `AggregateSlot` as a new variant or
  marker: `AggregateSlot::ParamReadOnly(arg_i)`. `AggregateSlot::Param`
  remains the mutable-pointer-arg case (unchanged behaviour).
- Update `LowerCtx::new` to consult the read-only scan: for read-only
  `in` aggregate params, skip slot allocation and skip the entry
  `Memcpy`. Member access through that param uses the arg pointer
  directly.
- Update `lower_expr.rs` / `lower_stmt.rs` / `lower_access.rs` /
  `lower_struct.rs` / `lower_array.rs` paths that currently treat
  `AggregateSlot::Local` and `AggregateSlot::Param` to also handle
  `AggregateSlot::ParamReadOnly` (mostly a one-line "address source =
  arg pointer instead of slot address").
- Benchmark on `examples/basic` (rainbow.shader) and at least one
  domain shader. Report cycle delta for tiny-aggregate hot paths
  (`distance_from_origin(Point p)`-like signatures). Capture results
  in `docs/roadmaps/2026-04-22-lp-shader-aggregates/m5-bench.md` (or
  similar).
- No filetest changes (this is a perf optimisation that must not
  change observable behaviour).

### Out of scope

- Cross-function (interprocedural) read-only propagation.
- Small-struct register-return fast path (separate follow-up roadmap).
- Any reshape of the existing aggregate ABI.

## Key decisions

- **Conservative vs. precise read-only analysis.** Conservative pass
  (intra-procedural, treats any onward `inout`/`out` pass as mutable)
  is the M5 default. A precise interprocedural pass is explicitly out
  of scope.
- **Whether to surface read-only at the LPIR level.** Two options: (a)
  frontend-only — the read-only path is just "skip the allocation +
  Memcpy" with no IR-level marker; backends never know. (b) Surface as
  `IrFunction::param_attrs[i] = ReadOnly` so backends can pass attrs
  to cranelift (`readonly`/`noalias`) for further optimisation.
  Settled in the M5 plan; expected to start with (a) and consider (b)
  as a follow-up if benchmarks show value.
- **Scan correctness.** Static guarantee: if the scan says read-only,
  the callee body emits no `Store` to the slot/pointer derived from
  the param, and emits no `Call` that takes the param as `inout`/
  `out`. M5 includes a debug-build assertion that catches violations
  during lowering.

## Deliverables

- New file:
  - `lp-shader/lps-frontend/src/readonly_in_scan.rs`
- Modified files:
  - `lp-shader/lps-frontend/src/lower_ctx.rs` (consume the scan, new
    `AggregateSlot::ParamReadOnly` variant)
  - `lp-shader/lps-frontend/src/lower_struct.rs`
  - `lp-shader/lps-frontend/src/lower_array.rs`
  - `lp-shader/lps-frontend/src/lower_expr.rs`
  - `lp-shader/lps-frontend/src/lower_stmt.rs`
  - `lp-shader/lps-frontend/src/lower_access.rs`
- Bench artefact:
  - `docs/roadmaps/2026-04-22-lp-shader-aggregates/m5-bench.md` with
    before/after cycle counts on `examples/basic` and one domain
    shader (R4 mitigation evidence).

## Dependencies

- **Requires M1 + M2 complete.** The optimisation operates over
  already-lowered functions and assumes both array and struct
  aggregate types use `AggregateSlot::Param` for `in` aggregates.
- Recommended after M3 (so arrays-of-structs benefit too) but not
  strictly required.

## Execution strategy

**Option B — `/plan-small`.**

Justification: self-contained analysis pass + a focused lowering tweak
across the existing `AggregateSlot` consumers. ~2 phases. One real
design question (frontend-only vs. surface-as-LPIR-attr); otherwise
mechanical. Benchmark gate distinguishes "shipped" from "started."

**Suggested chat opener:**

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?
