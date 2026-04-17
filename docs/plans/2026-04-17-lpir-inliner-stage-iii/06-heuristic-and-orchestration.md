# Phase 6 — Heuristic + orchestration

## Scope of phase

Tie everything together: add **`lpir/src/inline/heuristic.rs`** and
fill in **`lpir/src/inline/mod.rs::inline_module`** with the full
orchestration loop. After this phase, calling
**`lpir::inline_module(&mut module, &config)`** actually inlines.

Per Q1 / M3.1, the **`func_weight`** heuristic uses `body.len()` as a
first-pass approximation; empirical tuning is deferred to M3.1.

Per Q11, the orchestrator emits **`log::debug!`** for every
inlining decision (inline / skip-budget / skip-recursive /
skip-too-large) so behavior is debuggable from CLI tools.

## Code Organization Reminders

- Two files: `lpir/src/inline/heuristic.rs` (new) and
  `lpir/src/inline/mod.rs` (fill in the stub from Phase 2).
- **`log`** crate: confirm it's already a dependency of **`lpir`**
  (other crates in the workspace use it). If not, add with
  `default-features = false` for **`#![no_std]`** compatibility.
- Keep heuristic decisions pure functions: input is `(callee_size,
  call_count, current_module_size, config)`, output is `Decision`.

## Implementation Details

### `lpir/src/inline/heuristic.rs`

```rust
pub(crate) fn func_weight(func: &IrFunction) -> usize {
    func.body.len()
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Decision {
    Inline,
    SkipTooLarge { weight: usize, threshold: usize },
    SkipBudget { projected: usize, budget: usize },
    SkipMode,
}

pub(crate) fn should_inline(
    callee_weight: usize,
    callsite_count_at_callee: usize,
    current_module_op_count: usize,
    config: &InlineConfig,
) -> Decision {
    use crate::InlineMode::*;
    match config.mode {
        Never => return Decision::SkipMode,
        Always => { /* fall through; only budget can stop us */ }
        Auto => {
            if callee_weight > config.small_func_threshold
                && callsite_count_at_callee > 1
            {
                return Decision::SkipTooLarge {
                    weight: callee_weight,
                    threshold: config.small_func_threshold,
                };
            }
        }
    }

    // max_growth_budget per call site (post-inline body grows by ~weight per site).
    let projected_growth = callee_weight.saturating_mul(callsite_count_at_callee);
    if projected_growth > config.max_growth_budget {
        return Decision::SkipBudget {
            projected: projected_growth,
            budget: config.max_growth_budget,
        };
    }

    // module_op_budget: hard cap on total module ops post-inline.
    let projected_total =
        current_module_op_count.saturating_add(projected_growth);
    if projected_total > config.module_op_budget {
        return Decision::SkipBudget {
            projected: projected_total,
            budget: config.module_op_budget,
        };
    }

    Decision::Inline
}
```

> Confirm field names against `CompilerConfig` / `InlineConfig`
> (added in stage II); rename above if they differ.

### `lpir/src/inline/mod.rs` — full orchestration

```rust
pub fn inline_module(
    module: &mut LpirModule,
    config: &InlineConfig,
) -> InlineResult {
    let graph = callgraph::build(module);
    let (topo, cyclic) = callgraph::topo_order(&graph);

    let mut result = InlineResult {
        functions_skipped_recursive: cyclic.len(),
        ..Default::default()
    };

    for &cyc in &cyclic {
        log::debug!("inline: skip recursive func={:?}", cyc);
    }

    let mut current_op_count = total_op_count(module);
    let mut inlined_callees = BTreeSet::new();
    let mut mutated_callers = BTreeSet::new();

    'outer: for callee_id in topo {
        if cyclic.contains(&callee_id) { continue; }

        let callee_weight = heuristic::func_weight(&module.functions[callee_id]);
        let sites: Vec<(FuncId, usize)> = graph
            .callers_of
            .get(&callee_id)
            .into_iter()
            .flat_map(|callers| callers.iter())
            .flat_map(|&caller| {
                graph
                    .call_sites_of
                    .get(&caller)
                    .into_iter()
                    .flat_map(move |sites| {
                        sites.iter().filter_map(move |&(idx, c)| {
                            (c == callee_id).then_some((caller, idx))
                        })
                    })
            })
            .collect();

        if sites.is_empty() { continue; }

        let decision = heuristic::should_inline(
            callee_weight, sites.len(), current_op_count, config,
        );

        match decision {
            Decision::Inline => {
                log::debug!(
                    "inline: callee={:?} weight={} sites={} module_ops={}",
                    callee_id, callee_weight, sites.len(), current_op_count,
                );
                // Splice each site. Process within a caller in DESCENDING
                // op_idx order so earlier indices stay valid as later ones
                // are spliced in place.
                let by_caller = group_by_caller_desc(&sites);
                // Take the callee out of the map so we can freely &mut
                // every caller; put it back when done with this callee.
                let callee = module.functions.remove(&callee_id)
                    .expect("topo callee must exist");
                for (caller_id, indices) in by_caller {
                    let caller = module.functions.get_mut(&caller_id)
                        .expect("caller must exist");
                    for op_idx in indices {
                        splice::inline_call_site(caller, &callee, op_idx);
                        result.call_sites_replaced += 1;
                    }
                    mutated_callers.insert(caller_id);
                }
                module.functions.insert(callee_id, callee);
                inlined_callees.insert(callee_id);
                current_op_count = total_op_count(module);
            }
            Decision::SkipTooLarge { weight, threshold } => log::debug!(
                "inline: skip callee={:?} too_large weight={} threshold={}",
                callee_id, weight, threshold,
            ),
            Decision::SkipBudget { projected, budget } => {
                log::debug!(
                    "inline: skip callee={:?} budget projected={} budget={}",
                    callee_id, projected, budget,
                );
                if projected > config.module_op_budget {
                    result.budget_exceeded = true;
                    break 'outer;
                }
            }
            Decision::SkipMode => log::debug!(
                "inline: skip callee={:?} mode=Never", callee_id,
            ),
        }
    }

    // Recompute offsets once per mutated caller.
    for caller_id in mutated_callers {
        let f = module.functions.get_mut(&caller_id)
            .expect("mutated caller must exist");
        recompute_offsets(&mut f.body);
        // Optional: shrink_to_fit for embedded RAM hygiene.
        f.body.shrink_to_fit();
    }

    result.functions_inlined = inlined_callees.len();
    result
}
```

Helpers:

- **`total_op_count(module) -> usize`**: sum of `body.len()` across
  functions. Cheap; recompute on each iteration is fine.
- **`borrow_two_mut(map, a, b)`**: helper to borrow two distinct
  entries `&mut IrFunction` out of `BTreeMap<FuncId, IrFunction>`
  simultaneously. Cleanest approach: temporarily `take`/`remove` one
  entry into a local, mutate the other in place via `get_mut`, then
  re-insert. Or use unsafe pointer math through two `get_mut` calls
  (avoid). Or restructure the loop so each splice borrows only one
  function at a time. Prefer the take/insert dance for clarity;
  performance impact is negligible since this happens once per inlined
  callee.
- **`group_by_caller_desc`**: bucket `(caller, op_idx)` pairs by
  caller into `Vec<(FuncId, Vec<usize>)>` with each inner vec sorted
  descending. Iteration order across callers is not material.

### Determinism notes

- Topo order is deterministic (Kahn with `BTreeSet` queue).
- For each callee, the set of call sites comes from
  `callers_of[callee]` (sorted) cross `call_sites_of[caller]` (body
  order); descending splice order within a caller keeps op indices
  stable.
- `inline_module` is therefore deterministic across runs given the
  same input module + config.

### `lpir/src/lib.rs`

- Already re-exports `inline_module` and `InlineResult` from Phase 2.
- Add `pub use inline::InlineResult;` if not already present.

## Tests (`lpir` crate)

`tests/inline_basic.rs` (extend from Phase 5): add end-to-end tests
that go through `inline_module` rather than calling `inline_call_site`
directly:

- **`leaf_inlined_into_caller`**: 2-function module, default config.
  After `inline_module`: 1 call site replaced, caller body grew
  appropriately, callee still present (M5 will delete it).
- **`chain_inlined_bottom_up`**: A→B→C. Expect C inlined into B first,
  then B (with C inlined inside it) inlined into A.
- **`recursive_skipped`**: A→A. Expect `functions_skipped_recursive ==
  1`, `call_sites_replaced == 0`, A's body unchanged.

`tests/inline_heuristic.rs` (new):

- **`mode_never`**: any callee → `SkipMode`, no inlining.
- **`mode_always_inlines_huge_callee`**: huge callee (weight ≫
  threshold) called once → still inlined under `Always` (only budget
  can stop it).
- **`auto_skips_large_multi_site`**: weight > threshold, 2 call sites
  → `SkipTooLarge`, not inlined.
- **`auto_inlines_large_single_site`**: weight > threshold, 1 call
  site → inlined (single-site exception per `should_inline` logic).
- **`module_op_budget_hit`**: tiny budget → `budget_exceeded == true`,
  partial work preserved.
- **`max_growth_budget_per_callee`**: callee weight × sites exceeds
  per-callee growth → `SkipBudget`, other callees still considered.
- **`debug_log_contains_decisions`**: capture `log` output (use
  `log::set_logger` with a test sink), assert one line per decision
  category.

## Validate

```bash
cargo test -p lpir
```

Other crates (lpvm-native / wasm / cranelift / lps-filetests) can
build but are not exercised here — `inline_module` is opt-in and not
yet wired into the compile pipeline (M4).
