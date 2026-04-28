# M5 — Read-only `in` aggregate optimisation: plan

## Design

### Scope of work

Intra-procedural analysis: for each by-value `in` aggregate parameter (array,
struct, array-of-struct), classify **read-only** vs **mutable**. Read-only:
skip stack slot + prologue `Memcpy`; `aggregate_map` uses
`AggregateSlot::ParamReadOnly(arg_i)`; loads use `arg_vregs[arg_i][0]` as base
(same address as today’s memcpy source). **No filetest expectation changes.**

**Out of scope:** interprocedural propagation; LPIR `readonly` attrs (Q1); ABI
changes; register-return.

### Resolved decisions (Q1–Q6)

| Q   | Decision                                                                          |
| --- | --------------------------------------------------------------------------------- |
| Q1  | Frontend-only M5; no new LPIR param metadata                                      |
| Q2  | New enum variant `AggregateSlot::ParamReadOnly(u32)`                              |
| Q3  | Store to param’s aggregate `LocalVariable` (peel like lowering) ⇒ mutable         |
| Q4  | Call passes param local / `FunctionArgument(i)` as callee `inout`/`out` ⇒ mutable |
| Q5  | `debug_assert!` on impossible Store into read-only classification                 |
| Q6  | Add `m5-bench.md` with before/after cycles                                        |

### File structure

```
lp-shader/lps-frontend/src/
├── lib.rs                          # UPDATE: mod readonly_in_scan;
├── lower_ctx.rs                    # UPDATE: AggregateSlot, LowerCtx::new, optional scan hook
├── readonly_in_scan.rs             # NEW: scan_in_aggregate_params(module, func) -> ReadOnlyInMask
├── lower_array.rs                  # UPDATE: aggregate_storage_base_vreg, copy_stack_array_slots, …
├── lower_struct.rs                 # UPDATE: match ParamReadOnly where slot kind matters
├── lower_expr.rs                   # UPDATE: any AggregateSlot match / aggregate loads
├── lower_stmt.rs                   # UPDATE: Store paths + debug_assert for read-only
├── lower_access.rs                 # UPDATE: AggregateSlot matches
└── lower_call.rs                   # UPDATE: only if by-value in + call interaction needs explicit handling

docs/roadmaps/2026-04-22-lp-shader-aggregates/
├── m5-bench.md                     # NEW: benchmark results (after Phase 2)
└── m5-readonly-in-optimisation/
    └── plan.md                     # this file
```

### Conceptual architecture

```
┌─────────────────────────────────────────────────────────────┐
│  readonly_in_scan (pre-pass, no LowerCtx)                  │
│  For each arg index i with by-value in aggregate:            │
│    mutable if: Store targets param local lv_i                │
│              OR Call passes lv_i or FunctionArgument(i)      │
│                 as inout/out actual                          │
│    else: read-only                                            │
└───────────────────────────┬─────────────────────────────────┘
                            │ BTreeMap<u32, bool> or bitset
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  LowerCtx::new                                               │
│  For each PendingInAggregateValueArg:                        │
│    if read_only[i]:                                         │
│      NO alloc_slot, NO Memcpy                               │
│      aggregate_map[lv] = { slot: ParamReadOnly(i), … }       │
│    else: (unchanged) Local + Memcpy                          │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  aggregate_storage_base_vreg                                 │
│    Local(slot)     → SlotAddr                               │
│    Param(i)        → arg_vregs[i][0]  (inout/out)           │
│    ParamReadOnly(i)→ arg_vregs[i][0]  (by-value in, no copy)│
└─────────────────────────────────────────────────────────────┘
```

**Invariant:** `ParamReadOnly` is only used when the scan proved no store and no
onward `inout`/`out` pass for that param index.

---

## Phases

### Phase 1: Scan + `AggregateSlot::ParamReadOnly` + `LowerCtx::new` — [sub-agent: yes]

#### Scope of phase

- Add `readonly_in_scan.rs` with a public entry point, e.g.
  `pub(crate) fn in_aggregate_param_read_only(
    module: &Module,
    func: &Function,
) -> Result<BTreeMap<u32, bool>, LowerError>`  
  returning **true** = safe to elide copy (read-only). Keys are argument indices
  `i` where `func.arguments[i]` is `Array` or `Struct` (by-value `in`).
- **Mutable** if:
  - Any `Statement::Store` in `func` has a pointer that peels (unwrap `Load` /
    `Access` / `AccessIndex` chains as needed — mirror patterns in
    `lower_stmt.rs` / `lower_access.rs` for aggregate stores) to
    `Expression::LocalVariable(lv)` where `lv` is the **`local_for_in_aggregate_value_param_optional`**
    local for that arg index; OR
  - Any `Statement::Call` / call lowering path: an actual argument slot that is
    **`inout` or `out`** in the **callee** and the expression is
    `FunctionArgument(i)` or `LocalVariable(lv)` for that same `in` aggregate param.
    (Use `module.functions[callee].arguments[j]` / Naga’s binding to detect
    pointer formals; align with how `lower_call.rs` classifies arguments.)
- If uncertain, default **mutable** (conservative).
- Add `AggregateSlot::ParamReadOnly(u32)` next to `Param` in `lower_ctx.rs`;
  document that it is **only** for by-value `in` aggregates with elided copy.
- In `LowerCtx::new`, after building `pending_in_aggregate_specs`, query the scan;
  for read-only entries, **omit** `alloc_slot`, **omit** `Memcpy`, set
  `AggregateInfo { slot: AggregateSlot::ParamReadOnly(spec.arg_i), layout, naga_ty }`.
- Update **`aggregate_storage_base_vreg`** in `lower_array.rs` to handle
  `ParamReadOnly` identically to `Param` (return `arg_vregs_for(*arg_i)?[0]`).
- Export module from `lib.rs`.

**Out of scope:** exhaustive audit of all `AggregateSlot` match sites (Phase 2);
benchmark file; debug asserts in Store paths (Phase 2).

#### Code organization reminders

- One concept per file; `readonly_in_scan.rs` at top of `lib.rs` mod list near
  other lowering helpers.
- Helpers for “peel store pointer to base expression” at **bottom** of scan file.
- TODO only for deliberate follow-ups (e.g. interproc); none for “finish later”.

#### Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope beyond Phase 1.
- Do **not** suppress warnings or weaken tests.
- If call-site detection for Q4 is unclear, stop and report with file:line notes.

#### Implementation details

- Reuse or duplicate minimal peel logic from `lower_stmt` / `lower_access` rather
  than refactoring those modules in Phase 1 (refactor only if trivial).
- `local_for_in_aggregate_value_param_optional` is already in `lower_ctx.rs`;
  consider `pub(crate)` re-export or duplicate the name→lv resolution in the
  scan by calling the same helper for each candidate arg index.
- Ensure **read-only** path still inserts `aggregate_map[lv]` so all existing
  lowering that keys off `lv` keeps working; only `slot` kind changes.

#### Validate

```bash
cargo build -p lps-frontend
cargo clippy -p lps-frontend -- -D warnings
cargo test -p lps-frontend
scripts/filetests.sh --concise
```

---

### Phase 2: Consumers, debug asserts, benchmark — [sub-agent: supervised]

#### Scope of phase

- **Grep** `AggregateSlot::` and `match.*slot` / `AggregateSlot::Local` /
  `AggregateSlot::Param` across `lps-frontend/src`. Every site must treat
  `ParamReadOnly` correctly:
  - Same as `Param` for **address** / **load** paths.
  - Same as `Local` for **copy** / **memset** / **slot-only** ops only where a
    real stack slot exists — `ParamReadOnly` must **not** use `SlotAddr`; if an
    operation truly needs a writable slot, the scan should have classified
    mutable (document any edge case).
- **`copy_stack_array_slots`**: keep requiring both `Local` or extend to reject
  `ParamReadOnly` with a clear error (should not occur for read-only-only
  patterns); verify call sites.
- **`lower_struct.rs`** / **`lower_call.rs`**: any `AggregateSlot::Local`-only
  arm that applies to **in** param locals — verify.
- Add **`debug_assert!`** (or `debug_assert_eq!`) in **Store** lowering when the
  target aggregate is classified read-only — should be unreachable; use a flag
  on `AggregateInfo` or side table `read_only_in_args: BTreeSet<u32>` on
  `LowerCtx` if needed.
- **`m5-bench.md`:** run before/after on `examples/basic` (rainbow.shader) and
  one domain shader; record methodology, targets (rv32n/rv32c/wasm as
  appropriate), and cycle deltas. If bench harness is missing, document
  commands attempted and blockers.

**Out of scope:** LPIR attribute threading; changing filetest expectations.

#### Code organization reminders

- Prefer small targeted match arms over giant refactors.
- Keep benchmark doc factual (date, git hash optional, numbers).

#### Sub-agent reminders

- Do **not** commit.
- Do **not** disable tests to green.
- Report any behavioural doubt: run focused filetests (`function/param-struct.glsl`,
  `function/call-aggregate-roundtrip.glsl`, array-of-struct params).

#### Validate

```bash
cargo clippy -p lps-frontend -- -D warnings
scripts/filetests.sh --concise
```

---

### Phase 3: Cleanup, plan notes, commit — [sub-agent: main]

#### Scope of phase

- Grep diff for `TODO`, `dbg!`, temporary `allow`.
- Fill **`# Decisions for future reference`** in this file (or `No notable decisions.`).
- Move any stale “open questions” content to **`# Notes`** (historical) only if needed.
- Single **Conventional Commits** commit with body bullets +  
  `Plan: docs/roadmaps/2026-04-22-lp-shader-aggregates/m5-readonly-in-optimisation/plan.md`

#### Validate

Same as Phase 2; working tree clean.

---

## Notes

### Historical: Q1–Q6 (answered — all agreed)

| #   | Question                       | Outcome |
| --- | ------------------------------ | ------- |
| Q1  | Frontend-only                  | Yes     |
| Q2  | `ParamReadOnly` variant        | Yes     |
| Q3  | Store to param local ⇒ mutable | Yes     |
| Q4  | Onward inout/out ⇒ mutable     | Yes     |
| Q5  | debug_assert on violation      | Yes     |
| Q6  | `m5-bench.md`                  | Yes     |

### Decisions for future reference

See `summary.md` in this directory (`Decisions for future reference` section).
