# M2 — Struct Lowering: Summary

Completed 2026-04-23. Struct acceptance corpus and default-target filetests pass on `wasm.q32`, `rv32c.q32`, and `rv32n.q32`.

## What landed

- `aggregate_layout(module, ty)` — single source of truth for array/struct ABI and slot layout (`naga_util` / earlier phases).
- `lower_aggregate_write::store_lps_value_into_slot` — unified write of an `LpsType` at `(base, offset)` with memcpy fast path.
- `lower_struct.rs` — struct locals: member load/store paths, zero-fill, call-result handling; **global struct** paths: `peel_struct_access_index_chain_to_global`, `load_struct_path_from_global`, `store_struct_path_into_global` (VMContext / std430).
- Frontend: struct arms in `naga_util`, `LowerCtx`, `lower_expr`, `lower_stmt`, `lower_access`, `lower_call` (args/returns); **AccessIndex** on `Load(GlobalVariable)` for uniform/private struct reads; stores through global struct member chains for **private** globals only.
- **Guard**: global peel/load/store runs only when the global’s root Naga type is `Struct` — vector/matrix/scalar globals keep the existing `Pointer`/`GlobalVariable` lowering.

## Filetest deltas

Approximate `git diff --stat` for `lp-shader/lps-filetests/filetests/` plus targeted frontend files in this milestone:

- Removed stale `@unimplemented(wasm.q32)`, `@unimplemented(rv32c.q32)`, `@unimplemented(rv32n.q32)` across the M2 struct corpus (`struct/*`, `function/param-struct.glsl`, `function/return-struct.glsl`, `uniform/struct.glsl`, `global/type-struct.glsl`).
- Full-suite `./scripts/filetests.sh --fix --assume-yes` also dropped stale markers in a few non-struct files that were unexpectedly passing (`control/ternary/types.glsl`, `function/param-default-in.glsl`, `function/scope-local.glsl`).
- `struct/define-nested.glsl`: removed invalid self-referencing `struct Node { Node next; }` (parse error); kept `// @unimplemented(jit.q32)` on runnable cases.
- `global/type-struct.glsl`: corrected `test_type_struct_nested` expectation (11.0 not 12.0); made four read-only tests **self-contained** (each writes globals before read) so they pass with **fresh instance per `// run:`**; `jit.q32` markers unchanged where present.

`@unimplemented(jit.q32)` remains on struct corpus tests that still skip JIT (out of M2 acceptance).

## Bugs found and fixed in M2 / phase 06

- Global struct member access: uniform `Load`+`AccessIndex` and private global member stores needed VMContext path lowering (phase 06).
- `define-nested.glsl` contained invalid GLSL that failed the whole file at parse.
- `type-struct` used wrong arithmetic expectation for one test; several cases assumed cross-`run:` global state while the harness uses a **new instance per directive** — fixed by in-function setup.
- Initial global peel was applied to all globals; restricted to **struct-root** globals to avoid breaking scalar/vector global `AccessIndex` (regression on `global/access-from-main.glsl` and similar).
- `just test`: `fw-tests` `scene_render_position_emu` failed once (environment/timing); passed on immediate rerun — treat as flaky if it recurs.

## Known follow-ups (β re-marks)

- None for the struct corpus on `wasm.q32` / `rv32c.q32` / `rv32n.q32`.
- Optional: ensure WASM instantiation zero-fills VMContext for deterministic single-`run:` behavior (RV32 was already “correct” zeros; WASM full-file order could mask missing setup).

## Plan

`00-notes.md`, `01-design.md`, `02-aggregate-layout-refactor.md` … `06-enable-and-validate.md`.
