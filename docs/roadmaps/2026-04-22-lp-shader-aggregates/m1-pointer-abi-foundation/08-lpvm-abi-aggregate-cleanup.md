# P8 — `lpvm_abi` aggregate-arm cleanup

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md`.
Depends on: P3 (frontend now emits aggregate pointer args), P6
(`lpvm-emu` no longer relies on `lpvm_abi` for aggregate args), P7
(`lpvm-wasm` no longer relies on it either).
Parallel with: P9 (filetest CHECK rewrites).

## Scope of phase

Remove or collapse the aggregate arms of `lp-shader/lpvm/src/lpvm_abi.rs`
that previously flattened arrays into multiple scalars at the
host/guest boundary. After P3/P6/P7, those arms are dead — every
aggregate boundary now flows through `LpvmDataQ32` and a guest
pointer.

Concretely:

- `flatten_q32_arg` (or similar entry point): aggregate arms either
  return `Unsupported` (if the host code path for aggregates is now
  separate) or — preferred — the aggregate variants are deleted and
  the function's signature/contract narrows to scalar/vec/mat only.
- `decode_q32_return`: same treatment.
- Any `Q32ArgKind::Array(...)` / `Q32ReturnKind::Array(...)` enum
  variants that exist solely to express the old per-scalar marshalling
  are deleted (or downgraded to a `#[deprecated]` shim if downstream
  callers still need the symbol — but only if removal triggers
  cascading edits the phase would otherwise have to make).
- Update any `lpvm_abi` doc comments that describe aggregate marshalling
  to point readers at `LpvmDataQ32` and the per-runtime host paths
  (P6 / P7) instead.

**Out of scope:**

- Touching anything outside `lp-shader/lpvm/`.
- Filetests (P9).

## Code organization reminders

- One concept per file; this is a deletion phase, not an additions
  phase.
- If a helper has only aggregate callers and no scalar callers, delete
  it.
- If a helper has both, keep the scalar path and delete the aggregate
  branch.
- Don't introduce shims unless removal truly cascades into changes
  outside `lpvm/`. If you find a shim becoming necessary, **stop and
  report** — it likely means P6 or P7 didn't fully migrate.

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lpvm/`.
- Do **not** suppress warnings or add `#[allow(...)]`.
- Do **not** weaken or `#[ignore]` tests.
- If deletion cascades into edits in `lpvm-cranelift`, `lpvm-native`,
  `lpvm-emu`, or `lpvm-wasm`, **stop and report** — that's evidence the
  earlier phases didn't fully migrate, and the right fix is a follow-up
  in those phases (not new code here).
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Inventory

Read `lp-shader/lpvm/src/lpvm_abi.rs` end-to-end and list:

- All aggregate-shaped arms (anything that mentions `Array`, `Struct`,
  multi-scalar flattening, or per-element decoding for non-scalar
  values).
- All public types / enums whose variants exist solely for aggregates.
- All public functions whose signatures imply aggregate handling
  (e.g. one that returns `Vec<Val>` for a single LPIR arg of aggregate
  type).

### 2. Delete

For each aggregate arm:

- Confirm via `rg` that no caller in `lpvm-cranelift`, `lpvm-native`,
  `lpvm-emu`, or `lpvm-wasm` still uses it. If a caller remains, that
  caller is one of P4–P7's responsibilities — add it to the report-back
  and **do not** add a shim here.
- Delete the arm.
- Update doc comments.

For each enum variant whose only members were aggregate-shaped:

- If the variant is unused everywhere now, delete it.
- If the enum has a `#[non_exhaustive]` marker or implements
  `Serialize`/`Deserialize`, watch for downstream breakage and call it
  out in the report.

### 3. Narrow signatures

If a function previously returned `Vec<Val>` for a single LPIR arg
(only because aggregates expanded to many), narrow it:

```rust
// Before:
pub fn flatten_q32_arg(...) -> Result<Vec<Val>, AbiError>;

// After:
pub fn flatten_q32_arg(...) -> Result<Val, AbiError>;
```

…**only if** every remaining (scalar) caller now produces exactly one
`Val`. If some scalar arms still produce multiple `Val`s (e.g. f64 →
two i32s on a 32-bit ABI), keep `Vec<Val>` and just delete the
aggregate branches inside.

### 4. Tests

`lpvm`'s existing tests should narrow naturally — delete any tests that
exercised the aggregate flattening behaviour, since that contract no
longer exists.

If a test deletion looks suspicious (e.g. it tested ABI invariants that
should still hold for scalars), preserve the scalar half and drop only
the aggregate half.

## Validate

```
cargo check -p lpvm
cargo test  -p lpvm
just check
just test-glsl
```

If `just test-glsl` fails because a downstream crate still imports a
deleted symbol, **stop and report** — that's a sign P4–P7 left a
caller behind.

## Done when

- Aggregate arms in `lpvm_abi` are deleted (or narrowed to
  `Unsupported` if the code shape requires a placeholder).
- No downstream crate imports the deleted symbols.
- Doc comments updated.
- `cargo test -p lpvm` and `just check` are green.
- No new `#[allow(...)]`; no `#[ignore]`d tests.
