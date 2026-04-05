# LPIR / Cranelift JIT — feature gaps (from filetests)

Notes from `jit.q32` (and related) GLSL filetest failures versus `rv32.q32` / `wasm.q32`. Use this
as a backlog when extending the Naga → LPIR → Cranelift path.

## Context

- **JIT** uses host native calling conventions; **rv32** / **wasm** use their own ABIs. A bug can
  show up only on `jit.q32` (e.g. Apple AArch64 multi-scalar returns vs Rust `extern "C"` struct
  returns — addressed via inline asm + register read in `lpir-cranelift` `invoke.rs`).
- Many failures are **unsupported in lowering**, not wrong codegen: the compiler errors with
  explicit messages (`unsupported type`, `unsupported expression`, etc.).

## Recently addressed (keep in mind when debugging)

- **AArch64 JIT multi-return**: read `x0`–`x3` after `blr` for 2–4 `i32` returns; `repr(C)` struct
  returns do not match Cranelift’s per-return GPR layout on Apple.
- **Vector component assign / read**: `Store(AccessIndex(…))` and `AccessIndex` on Naga’s
  `Pointer → Vector` (locals and pointer parameters) in `lps-frontend` (`lower_stmt.rs`,
  `lower_expr.rs`). Unblocks e.g. `function/param-in.glsl`, `operators/preinc-component.glsl`.

## Gap categories

### 1. Aggregate types in Naga → LPIR lowering

| Area                         | Symptom / example                        | Direction                                                                                                                                                               |
|------------------------------|------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| **Matrices** (`mat2`–`mat4`) | `unsupported type: Matrix { … }`         | Type lowering, IR representation (column-major scalars vs struct), ops, builtins (`matrix-inverse`, `matrix-compmult`), filetests under `matrix/`, `builtins/matrix-*`. |
| **Arrays**                   | `unsupported type for LPIR: Array { … }` | Stack slots / element addressing, bounds, `const/array-size/*`, `array/phase/*`.                                                                                        |
| **Structs**                  | `unsupported type … Struct { … }`        | Layout, member access, `struct/*` filetests.                                                                                                                            |

These block large **filetest trees** that already pass on backends that support the same GLSL
through different pipelines.

### 2. Builtins and relational ops on vectors

- Example: `unsupported expression: Relational { fun: IsNan, argument: … }` on `vec4` (
  `builtins/common-isnan.glsl`, similar for `isinf`).
- **Work**: extend `lower_expr` / math lowering so vector relational builtins decompose
  component-wise (or call `__lp_*` vector helpers), aligned with wasm/rv32 behavior.

### 3. `lpir-cranelift` host `call()` / metadata limits

- **`GlslType`** in `lpir` metadata today has no matrix (or array) variants — Level-1
  `JitModule::call` / `decode_q32_return` are built for scalars and vectors only.
- **`invoke_i32_args_returns`**: `n_ret` capped at **4** words today. A `mat2` return is 4×`f32` in
  Q32 (= 4 words) at the boundary; **`mat3` / `mat4`** need **9** or **16** words unless the ABI
  uses a different return strategy (e.g. implicit return area / `enable_multi_ret_implicit_sret`, or
  caller-allocated buffer).
- **Work**: extend metadata + flatten/decode for matrix returns; extend invoke (AArch64 can use more
  int return regs; larger arities may need stack return area matching Cranelift’s ABI flags).

### 4. Statements / control / calls

- Anything that lowers to **stores through pointers** that are not yet modeled (e.g. some `out` /
  `inout` + complex lvalues) may still hit `store to non-local pointer` or related errors — *
  *component** stores for vector locals/params are covered; **matrix element** component store is
  explicitly rejected for now (`lower_stmt.rs`).
- **User calls** with `out`/`inout` already use stack slots + copy-back; edge cases (arrays,
  matrices as `out`) tie to aggregate support above.

### 5. Postfix increment / decrement on components

- **`operators/postinc-component.glsl`** (`v.x++`): **wrong numeric result** on `jit.q32` (e.g.
  expected `3.0`, got `4.0`) — semantics bug in how postfix side effects are ordered relative to the
  old value, **not** the component load/store path itself.
- **Work**: find Naga’s lowering for postfix on `AccessIndex` / pointer lvalues and match GLSL
  rules (save old scalar, then update component, expression value is old).

### 6. Filetest harness / concurrency (investigation)

- Report: full suite with `LP_FILETESTS_THREADS=1` sometimes shows **0/N “pass”** for files that *
  *pass when run alone** or under a narrower path (e.g. `scalar/`). Could be **shared mutable state
  ** (globals, caches, JIT module reuse), **working directory**, or **summary accounting** — needs a
  dedicated repro and fix; not the same as “unsupported feature” failures.

## Filetest directories that mostly map to the gaps above

Rough mapping (many files per dir):

- `matrix/**` — matrix type + ops + builtins.
- `array/**`, `const/array-size/**` — arrays + const sizing.
- `struct/**` — structs.
- `builtins/common-isnan.glsl`, `common-isinf.glsl`, `edge-*` — builtins / edge cases on vectors and
  math.
- `function/param-out*.glsl`, `param-inout.glsl`, `return-matrix.glsl` — aggregates + calling
  conventions.
- `operators/incdec-*`, `postinc-component.glsl` — inc/dec, postfix fix.
- `type_errors/**` — expect diagnostics; ensure still correct as front-end evolves.
- `uvec*/`, `vec/**` (generated / compare) — often vector ops; some may fail for missing ops rather
  than types.

## Suggested order of attack (opinionated)

1. **Postfix component inc/dec** — small, test already exists; fixes visible correctness.
2. **Matrix type in LPIR + signatures** — unlocks a large filetest surface; pair with **invoke /
   metadata** for `mat2` first (4 words), then larger matrices + sret strategy.
3. **Vector relational builtins** (`isnan`, `isinf`, …) — localized lowering.
4. **Arrays**, then **structs** — more IR and control-flow surface area.
5. **Parallel filetest flake** — reproduce with logging; fix isolation or reporting.

## Code touchpoints (bookmark)

- `lp-shader/lps-frontend/src/lower_stmt.rs` — statements, `Store`, calls.
- `lp-shader/lps-frontend/src/lower_expr.rs` — expressions, `AccessIndex`, loads.
- `lp-shader/legacy/lpir-cranelift/src/invoke.rs` — host JIT calling; `call.rs` / `values.rs` —
  flatten/decode.
- `lp-shader/lpir/src/glsl_metadata.rs` — `GlslType` / function metadata for host calls.
- Cranelift ABI / flags: `enable_multi_ret_implicit_sret`, return-area pointer (`x8` on AArch64)
  when stack returns are required.
