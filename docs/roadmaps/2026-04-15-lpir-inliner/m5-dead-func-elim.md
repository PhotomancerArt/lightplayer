# M5 — Dead Function Elimination

Remove local functions that aren't reachable from a caller-supplied root
set. Separate from inlining (M3) — the inliner never deletes functions.

## Motivation

After inlining, helper functions have had their bodies copied into all
callers. The originals still exist in the module and still get compiled.
In production (single entry point), these are pure waste — removing them
saves compile time and code size.

Filetests may opt into the pass via `compile-opt(dead_func_elim.mode,
auto)`. The harness looks up entries by name, so anything DFE removes
that the test wants to call by name will fail with "symbol not found".
Mark functions you need preserved with `is_entry`, or keep them
reachable from an `is_entry` root.

## API

`lp-shader/lpir/src/dead_func_elim.rs`:

```rust
pub struct DeadFuncElimResult {
    pub functions_removed: usize,
}

/// Remove local functions not transitively reachable from any root.
pub fn dead_func_elim(
    module: &mut LpirModule,
    roots: &[FuncId],
) -> DeadFuncElimResult;

/// Helper: every function with `is_entry == true`.
pub fn roots_from_is_entry(module: &LpirModule) -> Vec<FuncId>;

/// Helper: look up roots by name.
pub fn roots_by_name(module: &LpirModule, names: &[&str]) -> Vec<FuncId>;
```

`roots` identifies the externally callable functions. Everything else is
a candidate for removal if not transitively reachable from a root via
`CalleeRef::Local` Call ops.

## Configuration

`CompilerConfig::dead_func_elim: DeadFuncElimConfig`:

```rust
pub enum DeadFuncElimMode {
    Auto,   // run when roots are available
    Never,  // skip the pass (default)
}

pub struct DeadFuncElimConfig {
    pub mode: DeadFuncElimMode,
}
```

String key: `dead_func_elim.mode = auto | never`. Plumbed through
`CompilerConfig::apply` and surfaced by `lp-cli shader-debug
--compiler-opt`.

Default `Never` means existing filetests behave exactly as before.

## Algorithm

1. **Build local-call adjacency.** For each function, walk the body and
   collect the set of `CalleeRef::Local(FuncId)` it calls.

2. **BFS from roots.** Starting from `roots`, follow the adjacency to
   find every transitively reachable function.

3. **Remove unreachable.** Delete from `module.functions` any local that
   is not reachable. Stable `FuncId` (M0) makes deletion safe — no other
   ref needs renumbering.

4. **`LpsModuleSig` is left alone.** It's name-keyed and harmless if
   stale; the runtime resolves entries by name and skips missing ones.

## Integration

Wired into all four backend entry points after `inline_module`:

- `lpvm-native::compile_module`
- `lpvm-cranelift::build_jit_module`
- `lpvm-cranelift::object_bytes_from_ir`
- `lpvm-wasm::compile_lpir`
- `lp-cli shader-debug` (`collect_fa_data`, `collect_cranelift_data`)

Each gates the call on `mode != Never`, computes
`roots_from_is_entry(&ir)`, and skips silently when the root set is
empty (e.g. unit-test harnesses that build raw modules).

The GLSL frontend (`lps-frontend/src/lower.rs`) sets `is_entry = true`
on the user-defined `render` function and on the synthesized
`__shader_init` so they survive DFE.

## WASM emitter dependency

DFE leaves gaps in the `FuncId` space. The WASM emitter previously
assumed `Local(FuncId(id))` could be turned into a WASM function index
by `filtered_import_count + id`, which only holds when FuncIds are
contiguous starting at 0. M5 fixes this by threading a `BTreeMap<FuncId,
u32>` through `EmitCtx` and looking up the WASM index by FuncId.

## Validation

```bash
cargo build
cargo test -p lpir
./scripts/glsl-filetests.sh optimizer/dead_func_elim/
./scripts/glsl-filetests.sh           # full suite, no regressions
```

End-to-end filetest:
`lp-shader/lps-filetests/filetests/optimizer/dead_func_elim/dfe-removes-unreachable.glsl`
runs across `rv32n.q32`, `rv32c.q32`, and `wasm.q32` with
`compile-opt(inline.mode, never)` + `compile-opt(dead_func_elim.mode,
auto)` and asserts `unused_dead` / `also_dead` are removed while
`render`, `test_dfe_basic`, and `helper` survive.

## Known limitations / follow-ups

- **`inline.mode=always` + DFE on a small `test_*` function removes the
  test.** When the inliner inlines a small `test_*` function into
  `render`, no caller remains, so DFE drops it. The harness then can't
  call it by name. The clean fix is to mark `test_*` functions as
  `is_entry` in the frontend (or equivalently extend the root set in
  the filetest path). Tracked in
  [`future-work.md`](./future-work.md).
- **Inliner stale call-graph indices when a single caller has multiple
  distinct local callees.** The bottom-up inliner builds the call graph
  once and never refreshes the per-caller op indices, so after the
  first callee is spliced into a caller the recorded sites for
  subsequent callees in the same caller are stale and silently skipped
  by `splice::inline_call_site`. Pre-existing M3 bug, exposed by the
  M5 filetest design exploration. Tracked in
  [`future-work.md`](./future-work.md).

## Estimated scope

Pass itself ~120 lines. Backend wiring ~30 lines per entry point.
Stable ids (M0) and inliner (M3/M4) did the heavy lifting.
