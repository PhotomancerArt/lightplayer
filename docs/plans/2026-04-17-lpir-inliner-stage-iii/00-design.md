# LPIR Inliner — Stage III Design (M3 + M2.5)

Source roadmap: `docs/roadmaps/2026-04-15-lpir-inliner/m3-inlining-pass.md`
plus `m2.5-continuing-marker.md` (folded in as Phase 1).

Question / answer trail in [00-notes.md](00-notes.md).

## Scope of work

Implement the LPIR inlining pass: `lpir::inline_module(&mut LpirModule,
&InlineConfig) -> InlineResult`. Bottom-up, never deletes functions, never
hard-errors, fully structural offset recompute, per-param scan-then-alias-or-copy,
heuristic-driven with debug-level decision logging.

Bundled prerequisite: M2.5's `LpirOp::Continuing` marker, which adds the
final piece of structural symmetry needed for offset recompute (loops gain
a marker for the start of their continuing block, mirroring `Else` for
ifs). Backends and interpreter keep using the cached
`LoopStart::continuing_offset` field unchanged.

Out of scope (deferred): wiring into `lpvm-native::compile_module` (M4),
GLSL filetests with `compile-opt` annotations (M4), perf measurement on
real shaders (M4 step 3), `func_weight` empirical tuning (M3.1), dead
function elimination (M5), inline-and-delete-as-we-go (future-work),
removing offset fields entirely (future-work).

## File structure

```
lp-shader/
├── lpir/
│   └── src/
│       ├── lib.rs                            # UPDATE: re-export inline_module / InlineResult
│       ├── lpir_op.rs                        # UPDATE (M2.5): + LpirOp::Continuing variant
│       ├── builder.rs                        # UPDATE (M2.5): push_continuing emits the marker
│       ├── parse.rs                          # UPDATE (M2.5): existing `continuing:` token → marker
│       ├── print.rs                          # UPDATE (M2.5): print marker, drop offset detection
│       ├── validate.rs                       # UPDATE (M2.5): exhaustive matches + nesting check
│       ├── interp.rs                         # UPDATE (M2.5): Continuing => pc += 1
│       ├── const_fold.rs                     # UPDATE (M2.5): conservative-clear arm
│       ├── inline/                           # NEW: the inliner
│       │   ├── mod.rs                        #   public API + orchestration loop
│       │   ├── callgraph.rs                  #   callees-of, callers-of, topological order, cycle detection
│       │   ├── offsets.rs                    #   recompute_offsets(&mut [LpirOp]) — reusable
│       │   ├── remap.rs                      #   scan_param_writes, build_remap, remap_op
│       │   ├── splice.rs                     #   inline_call_site (the splicer)
│       │   └── heuristic.rs                  #   func_weight, Decision, should_inline
│       └── tests/
│           ├── inline_basic.rs               # NEW: void / single-return / multi-return / nested
│           ├── inline_callgraph.rs           # NEW: cycles, diamond, chains
│           ├── inline_remap.rs               # NEW: vmctx alias, slot remap, pool splice via imports
│           ├── inline_heuristic.rs           # NEW: thresholds, budgets, mode=Never/Always/Auto
│           ├── inline_offsets.rs             # NEW: recompute_offsets correctness
│           └── inline_param_writes.rs        # NEW: read-only alias vs mutated copy
├── lpvm-native/
│   └── src/
│       └── lower.rs                          # UPDATE (M2.5): no-op match arm for Continuing
├── lpvm-wasm/
│   └── src/
│       └── emit/
│           └── ops.rs                        # UPDATE (M2.5): no-op match arm for Continuing
└── lpvm-cranelift/
    └── src/
        └── emit/
            └── control.rs                    # UPDATE (M2.5): no-op match arm for Continuing
```

## Conceptual architecture

```
                  inline_module(&mut LpirModule, &InlineConfig) -> InlineResult
                                          │
                                          ▼
        ┌──────────────────────── inline/mod.rs ────────────────────────┐
        │                                                                │
        │   ┌───────────────┐   ┌───────────────────────────────────┐   │
        │   │  callgraph.rs │   │            heuristic.rs           │   │
        │   │  build_graph  │──▶│  func_weight  │  should_inline    │   │
        │   │  topo_order   │   │               │  Decision         │   │
        │   │  detect_cycles│   └───────────────────────────────────┘   │
        │   └───────┬───────┘              │                             │
        │           │                      │                             │
        │           ▼                      ▼                             │
        │   For each callee in topo order, for each caller of that      │
        │   callee, if Decision::Inline:                                 │
        │           │                                                    │
        │           ▼                                                    │
        │   ┌─────────────────────── splice.rs ──────────────────────┐  │
        │   │  inline_call_site(caller, callee, call_op_idx, …):     │  │
        │   │      ① scan_param_writes(callee)        (remap.rs)     │  │
        │   │      ② build_remap(...)                 (remap.rs)     │  │
        │   │      ③ analyze return shape (0 / 1-at-end / multi)     │  │
        │   │      ④ build scratch Vec<LpirOp>:                      │  │
        │   │           - per-param Copy (if written) or alias       │  │
        │   │           - clone+remap callee body, splicing pool     │  │
        │   │             entries into caller.vreg_pool              │  │
        │   │           - rewrite Return → Copy (+ ExitBlock if      │  │
        │   │             multi); wrap in Block { _ } / End if multi │  │
        │   │      ⑤ caller.body.splice(call_idx..=call_idx, scratch)│  │
        │   └────────────────────────────────────────────────────────┘  │
        │           │                                                    │
        │           ▼                                                    │
        │   After all call sites of all callees processed:               │
        │   For each mutated function:                                   │
        │       recompute_offsets(&mut func.body)   (offsets.rs)         │
        │           │                                                    │
        │           ▼                                                    │
        │   Return InlineResult { functions_inlined, ... }               │
        └────────────────────────────────────────────────────────────────┘
```

## Key invariants enforced by the orchestration

- **Bottom-up topological order:** callee fully inlined before caller
  processes it. Single bottom-up pass.
- **Cycle nodes left alone** (Q3); counted in
  `result.functions_skipped_recursive`. Logged at `debug!`.
- **`module_op_budget`** checked between callees; sets `budget_exceeded`
  on overflow and stops the pass. Bottom-up means partial result still
  has the highest-leverage inlinings.
- **`growth_used`** accumulated across multi-callsite inlinings (Q11).
- **All original `IrFunction`s retained** in `module.functions`. No
  deletion. M5's job.
- **`debug_assert!`s** on internal invariants: remap arity matches
  callee.vreg_types.len(), control-flow stack empty at end of recompute,
  pool splice arity matches, vmctx slot of `param_writes` is `false`,
  every spliced `Call` op's `args.start` points inside `caller.vreg_pool`.

## Component responsibilities

| Module | Inputs | Outputs / Side effects | Reusable? |
|--------|--------|------------------------|-----------|
| `callgraph.rs` | `&LpirModule` | `CallGraph { callers_of, callees_of, topo_order, cyclic_set }` | yes — useful for any module-level pass |
| `heuristic.rs` | callgraph, `&InlineConfig`, `&mut growth_used`, callee id | `Decision { Inline { extra_growth }, Skip(reason) }` | inliner-specific |
| `remap.rs` | `&IrFunction` (callee), caller arg vregs, vmctx | `Remap { table: Vec<VReg>, param_copies: Vec<LpirOp> }` | inliner-specific |
| `splice.rs` | `&mut IrFunction` (caller), callee, call op idx, remap, return-shape | mutates caller body + pool | inliner-specific |
| `offsets.rs` | `&mut [LpirOp]` | patches all opener offsets in place | yes — also useful for any future structural transform |
| `mod.rs` | `&mut LpirModule`, `&InlineConfig` | `InlineResult`, mutates module | public API |

## Public API

```rust
// In lpir/src/inline/mod.rs, re-exported from lib.rs.

pub struct InlineResult {
    /// Distinct callees whose body was spliced into ≥1 caller this run.
    pub functions_inlined: usize,
    /// Total `Call` ops replaced.
    pub call_sites_replaced: usize,
    /// Distinct functions skipped due to call-graph cycles.
    pub functions_skipped_recursive: usize,
    /// True iff `module_op_budget` was hit and the pass stopped early.
    pub budget_exceeded: bool,
}

pub fn inline_module(
    module: &mut LpirModule,
    config: &InlineConfig,
) -> InlineResult;
```

## Logging contract

- `log::debug!` per-callee decision line (callee name, id, sites, size,
  decision, reason, growth deltas).
- `log::info!` end-of-pass summary line (totals + budget usage).
- No `log::warn!` / `log::error!` — recursion is silently skipped per Q3,
  budget overflow is signaled via the result field.

## Validation

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-wasm
cargo test -p lpvm-cranelift
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
```

All existing tests must still pass — M3 doesn't wire the inliner into any
production compile path (that's M4). Behavior is purely additive.
