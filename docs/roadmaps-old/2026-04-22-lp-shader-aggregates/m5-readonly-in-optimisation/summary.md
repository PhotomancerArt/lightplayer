### What was built

- `readonly_in_scan.rs`: intra-procedural classification of by-value `in` aggregate params (Store + onward `inout`/`out` call detection); `local_for_in_aggregate_value_param_optional` moved here to avoid import cycles.
- `AggregateSlot::ParamReadOnly(arg_i)`: elide stack slot + prologue `Memcpy` when read-only; addressing matches `Param` via `aggregate_storage_base_vreg`.
- `LowerCtx::new`: runs scan; read-only path inserts `aggregate_map` with `ParamReadOnly`; struct/array `ZeroValue` init skipped for `ParamReadOnly` so caller buffer is not zeroed.
- `debug_assert_not_param_readonly_aggregate_store` + call sites in array stores, struct stores, stmt whole-aggregate store, struct init.
- `copy_stack_array_slots`: clearer internal error when non-`Local` slots.
- `m5-bench.md`: methodology and expected effect; no checked-in before/after numbers.
- Plan directory `m5-readonly-in-optimisation/` with `plan.md` + this summary.

### Decisions for future reference

#### Read-only means “no writes to param proxy local” + no onward out/inout

- **Decision:** Classify mutable if any `Store` peels to the param’s aggregate local or if a `Call` passes `FunctionArgument(i)` / that local to a callee `Pointer` formal; otherwise read-only.
- **Why:** Conservative intra-procedural guarantee; avoids clobbering caller memory when eliding memcpy.
- **Rejected alternatives:** Interprocedural analysis (out of scope for M5).
- **Revisit when:** If Naga emits store lvalue shapes the peelers miss, extend peels or default to mutable.

#### `local_for_in_aggregate_value_param_optional` lives in `readonly_in_scan`

- **Decision:** Implement and `pub(crate) use` from `lower_ctx` to break `lower_ctx` ↔ scan cycle.
- **Why:** `LowerCtx::new` must call the scan without `readonly_in_scan` importing `LowerCtx`.
- **Rejected alternatives:** Duplicating scan-only helper logic in two files.

#### No LPIR `readonly` parameter attributes in M5

- **Decision:** Frontend-only elision; backends unchanged.
- **Why:** Roadmap Q1; smaller change; Cranelift `readonly` can be a follow-up if bench proves value.
- **Revisit when:** Benchmarks show backend could exploit aliasing.
