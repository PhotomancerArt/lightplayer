# M5 — LPIR Dead Function Elimination — Notes

Plan for the `dead_func_elim` pass: a small post-inline cleanup that drops
local functions with zero remaining call sites that aren't in the
caller-supplied root set. Implements
[m5-dead-func-elim.md](../../roadmaps/2026-04-15-lpir-inliner/m5-dead-func-elim.md).

## Scope of work

1. **`dead_func_elim` pass** in `lpir/src/dead_func_elim.rs`:
   - Inputs: `&mut LpirModule`, `roots: &[FuncId]`.
   - Algorithm: count local call sites per function (walk all bodies),
     mark reachable transitively from roots, remove unmarked entries
     from `module.functions`. Stable `FuncId` (M0) makes deletion safe.
   - Returns `DeadFuncElimResult { functions_removed: usize }` plus a
     `log::info!` summary like the inliner.
2. **`DeadFuncElimConfig`** added to `CompilerConfig`, mirroring
   `InlineConfig`:
   - `mode: DeadFuncElimMode` ∈ {`Auto`, `Never`}, default `Never`.
   - String keys `dead_func_elim.mode` plumbed through
     `CompilerConfig::apply` and `COMPILER_CONFIG_APPLY_HELP`.
3. **Backend wiring** (4 spots — same shape as M4):
   - `lpvm-native::compile_module`,
     `lpvm-cranelift::build_jit_module`,
     `lpvm-cranelift::object_bytes_from_ir`,
     `lpvm-wasm::compile_lpir`.
   - After the existing `inline_module` call, when `mode != Never`,
     compute roots and call `dead_func_elim`.
4. **Roots resolution.** GLSL frontend currently does **not** set
   `is_entry`. Production wiring needs an explicit signal. Two clean
   options (Q2): wire `is_entry` in `lps-frontend`, or carry an
   `entry_names` list in `CompilerConfig`. Filetests stay on `Never`.
5. **Tests:**
   - Rust unit tests in `lpir/src/tests/dead_func_elim.rs` (BTreeMap
     module, root reachability, multiple roots, no-op when nothing
     dead, removal of import-callers preserved).
   - One filetest under `filetests/optimizer/dead_func_elim/` exercising
     the `compile-opt(dead_func_elim.mode, auto)` + forced inline path
     end-to-end.
6. **Docs:** update `m5-dead-func-elim.md` to match current code shape
   (BTreeMap, no `OptPass` enum, roots-by-name in callers).

## Current state of the codebase

- `LpirModule { imports: Vec<ImportDecl>, functions: BTreeMap<FuncId,
  IrFunction> }` — keyed by stable `FuncId` (M0 done).
- `IrFunction { is_entry: bool, ... }` — set by `parse.rs` from textual
  `is_entry` directives and by some hand-rolled builder paths, but
  **not** by the GLSL frontend (`lps-frontend`).
- `CalleeRef::Local(FuncId)` references survive arbitrary
  insertion/removal in `module.functions` (no renumbering).
- `CompilerConfig { inline: InlineConfig }` lives in
  `lpir/src/compiler_config.rs`; `apply(key, value)` parses string
  overrides; `COMPILER_CONFIG_APPLY_HELP` documents them for
  `shader-debug --compiler-opt`.
- `inline_module(&mut module, &config.inline) -> InlineResult` is wired
  into all 4 backend entry points (M4). Each clones the IR, runs the
  inliner, then proceeds. Same pattern fits dead-func-elim.
- Filetests directly invoke arbitrary user functions by name (e.g.
  `test_call_simple_single_arg()`). Anything dead-func-elim removes
  that the harness wanted to call would break the test.
- Runtime instances also look up entries by name
  (`module.entry_offset(name)`), not by `is_entry`.

## Questions & Answers

- **Q1 — pass takes `roots: &[FuncId]` (not `&[&str]`).** ✓
  Pass works in `FuncId` space; provide a small `roots_by_name(&module,
  &[&str]) -> Vec<FuncId>` helper for callers with names.
- **Q2 — root resolution:** **(A) `is_entry` flag**, with prerequisite
  fix to `lps-frontend/src/lower.rs` that marks the GLSL entry point
  function with `is_entry = true`. Backends call
  `roots_from_is_entry(&module)` to populate the root set.
- **Q3 — default `dead_func_elim.mode = Never`.** ✓ Production opts in;
  filetests work unchanged.
- **Q4 — leave `LpsModuleSig` alone.** ✓ Sig is name-keyed; staleness
  is harmless.
- **Q5 — defer "inline-and-delete-as-we-go".** ✓ Already captured in
  `future-work.md`; revisit if peak memory becomes a real problem.
- **Q6 — filetest uses `compile-opt(inline.mode, always)` +
  `compile-opt(dead_func_elim.mode, auto)`.** ✓ Realistic production
  combo; harness asserts correctness; `functions_removed` visible via
  `log::info!`.
- **Q7 — `lp-cli shader-debug` prints `functions_removed`.** ✓ One
  extra log line, gated on `mode != Never`.
