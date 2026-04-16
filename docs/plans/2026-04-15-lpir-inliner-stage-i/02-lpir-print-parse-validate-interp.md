# Phase 2 — LPIR print, parse, validate, interpreter, tests

## Scope of phase

Complete `lpir` crate: printing and parsing `CalleeRef`, validation of `Call` targets via `ImportId`/`FuncId`, interpreter local call dispatch, and update all `lpir` unit tests (`src/tests/*.rs`).

## Code organization reminders

- Match existing style in `print.rs` / `parse.rs` (indentation, keyword names).
- Validation control-flow stack unchanged except `Call` target check (match enum, bounds on ImportId, key present for Local).

## Implementation details

- **`print.rs`:** `callee_name` match on enum; iterate `module.functions` with `.iter()` (pairs `(FuncId, &IrFunction)`). Preserve any ordering expectations (e.g. sorted by `FuncId` for stable output).
- **`parse.rs`:** build `CalleeRef::Import(ImportId(i))` / `Local(FuncId(i))` per name table; remove `import_count + local_index` flat math.
- **`validate.rs`:** resolve local callee via `FuncId`; `total` / indexing fixes where it assumed `Vec` index space.
- **`interp.rs`:** replace `callee_as_function` + `functions[fi]` with `FuncId` map lookup; dereference `callee` op field (may need `*` if pattern matched refs).
- **Tests:** replace every `CalleeRef(n)` with enum constructors; fix `m.functions[0]` → get by `FuncId` or iterate.

## Tests to write

- Existing tests updated; add one test that parses/reprints a module with mixed import + local call if not already covered.

## Validate

Target state for this phase: **`cargo test -p lpir` passes.** (Still OK if the rest of the workspace is red until later phases.)
