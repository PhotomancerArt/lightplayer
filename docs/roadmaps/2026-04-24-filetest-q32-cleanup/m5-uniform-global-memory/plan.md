# M5: Uniform and global memory — plan & status

Scope source: [m5-uniform-global-memory.md](../m5-uniform-global-memory.md), [00-notes.md](./00-notes.md), Section C in [broken.md](../../../reports/2026-04-23-filetest-triage/broken.md).

**Decisions (this roadmap):** Keep dummy `layout(binding=0)` for Naga compatibility; LightPlayer uses name/path-based uniform layout, not `binding` values. `global-future/*` out of scope. Validate **wasm.q32, rv32c.q32, rv32n.q32** only (jit deprecated for this work).

## Checklist

| Item | Status | Notes |
|------|--------|--------|
| **C — `global/type-array.glsl`** | **Done (2026-04-24)** | Implemented private global array subscript `Store` through `Access`: `AggregateSlot::Global` + `store_through_access` → `store_array_element_dynamic` (VMContext base + std430 array layout, clamped index). All 8 `// run` lines pass on wasm/rv32c/rv32n; stale expect-fail annotations removed. |
| **C — `global/forward-reference.glsl`** | **Done (2026-04-24)** | **Root cause:** Naga emits duplicate [`GlobalVariable`] handles for forward decl + later initializer; layout gave **two** VMContext regions so loads used the uninitialized handle while `__shader_init` wrote the other (especially visible on `mat3`). **Fix:** `compute_global_layout` merges by `(name, address space)` and assigns one `byte_offset` per logical global. **Filetests:** Early `// run:` lines that assumed “uninitialized” reads were updated to match product rule (initializers run before any entry): `+10` test now expects `52`, vec2 test expects `(6,11)`. `@broken` on `test_forward_reference_mat` removed. |
| **C — `uniform/defaults.glsl`** | **Done (2026-04-24)** | `test_initialize_uniform_usage`: expected `2.0` was **wrong arithmetic** for harness default **zero** uniforms (`0+0+0.5+0.5 = 1.0`). Annotation updated; not a layout/host bug. |
| **C — `uniform/no-init`, `uniform/pipeline`, `uniform/readonly`, `uniform/write-error`** | **Done (2026-04-24)** | Residual issues were **expectation / assertion type** mismatches: `uint` results must be asserted with `100u` / `1u` (actual `LpsValueF32::U32` vs expected `I32`). Combined/pipeline and write-error float expectations corrected for all-uniforms-zero. |
| **`function/call-order.glsl` (A / cross-cutting)** | **Deferred** | `InvalidMemoryAccess` on **rv32n** only: leave for native ABI/stack investigation; **do not** expand scope in M5 unless a tiny local fix appears. |
| **Remove stale Section C markers** | **Done** | `@broken` removed from forward-reference / uniform Section C files where expectations were fixed; `jit.q32` `@unimplemented` lines unchanged (jit deprecated for this roadmap). |
| **Validation** | **Done** | `cargo test -p lps-frontend`, `cargo clippy -p lps-frontend -- --no-deps -D warnings`; Section C filetests (paths below). |

## Commands (reference)

```bash
# Section C, three q32 targets
./scripts/glsl-filetests.sh global/type-array.glsl -t wasm.q32,rv32c.q32,rv32n.q32 --concise
./scripts/glsl-filetests.sh global/forward-reference.glsl uniform/defaults.glsl uniform/no-init.glsl \
  uniform/pipeline.glsl uniform/readonly.glsl uniform/write-error.glsl \
  -t wasm.q32,rv32c.q32,rv32n.q32 --concise
```

## Code touchpoints (this milestone)

- **Global array subscript store:** `lps-frontend` — `AggregateSlot::Global`, `aggregate_storage_base_vreg` (`lower_array.rs`), `store_through_access` `Expression::GlobalVariable` arm (`lower_access.rs`); uniform subscript store remains **rejected** (`cannot write to uniform variable`).

## Recommended next subtask

1. **Optional:** Teach filetest comparison (or `LpsValueF32::eq`) to treat equal `int`/`uint` bit patterns as matching when GLSL returns `int(...)` but the ABI surfaces `U32`, so tests can keep `== 100` instead of `== 100u`.  
2. **Deferred (unchanged):** `function/call-order.glsl` rv32n — out of M5 scope.

## Blockers summary

- **None** for Section C items above on wasm.q32 / rv32c.q32 / rv32n.q32 after merge + expectation fixes.
