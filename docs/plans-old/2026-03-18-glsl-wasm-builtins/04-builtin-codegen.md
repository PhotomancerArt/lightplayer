# Phase 4: Builtin codegen — inline vs import

## Current implementation (partial)

- **Inline float `genType`:** `abs`, `min`, `max`, `clamp`, `mix`, `step`, `sign`, `mod`, `smoothstep` in `expr/builtin_inline.rs` — Float mode uses `f32.abs` / `f32.min` / `f32.max`; Q32 uses i32 compare + `if`/`else` where needed and fixed-point math via `emit_binary_op`. `mix` Float path uses `broadcast_temp_f32` for `(y-x)*a`; Q32 `mix` uses `minmax_scratch_i32` for one component temp. **`fract`:** Float mode only (`floor` + subtract); Q32 errors. **`mix`:** bool blend overload not supported. Same total stored-slot budget ≤ 8 as `clamp` (x/y/a spans). User functions win over these names when present in `func_index_map`.
- **`Expr::FunCall` (Q32 imports):** After user calls and inline builtins, `glsl_q32_math_builtin_id`-mapped calls go to `expr/builtin_call.rs`: flatten float `genType` args (scalar broadcast, vectors component-wise), then per-component `call` to the import index from `WasmCodegenContext::builtin_func_index`.
- **`ldexp(genType, int)`:** Special-cased: one shared exponent `i32` for all components.
- **Limits:** Vector `fma` not emitted (needs >8 scratch slots). `clamp`/`min`/`max` need total stored slots ≤ 8 (e.g. `vec3`+`vec3`+`vec3` rejected). LPFX / other `is_builtin_function` paths without import mapping still error.

## Remaining scope

- **`Expr::FunCall`:** After type constructors and user functions, handle remaining `is_builtin_function` / `is_lpfx_fn`.
- **Inline builtins (match Cranelift):** Extend `builtin_inline.rs` — e.g. `floor`, `exp` where they are compositions / simple op sequences; no import. **`q32_builtin_import_suppressed`** in `builtin_scan` must stay aligned with anything inlined so unused `__lp_q32_*` imports are not emitted (e.g. `mod` uses inline Q32, not `LpQ32Mod`).
- **LPFX imports:** Pointer / struct-return signatures (`06-lpfx-out-params.md`).
- **Q32:** Ensure inline paths use fixed `emit_q32_*` helpers where applicable (mul/div/add sat already exist).

## Code organization reminders

- Split large match tables: `inline.rs` vs `import.rs` or by family (common, trig, lpfx).
- Tests first in test modules; helpers at bottom.

## Implementation details

- **Do not** import vector-specific symbols if decision is scalar-only — expand to per-component `call` for `sin(vec3)` etc.
- LPFX calls may need extra args (e.g. seed); follow frontend signature and Cranelift lowering order.

## Validate

- `test_q32_sin_compiles` in `lps-wasm/tests/basic.rs` (WASM module with `sin` → import `call`; host linker still required to run).
- Later: `clamp(0.5, 0.0, 1.0)`, vector `sin(vec3(...))` with wasmtime + `builtins.wasm` or stubs, and one vector case `sin(vec3(...))` if ready.
- `cargo test -p lps-wasm`
- Filetests: start with `filetests/builtins/` subsets — `./scripts/glsl-filetests.sh builtins/ --target wasm.q32` (exact path per repo layout).
