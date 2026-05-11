# Stage I: Builtin Naming Convention — Notes

## Scope of work

Establish `__lp_<module>_<fn>_<mode>` as the universal naming convention for
all builtins. Rename all existing symbols, update all consumers, make BuiltinId
self-describing. Three modules: `lpir`, `glsl`, `lpfn`.

## Current state

### BuiltinId (lps-builtin-ids)

- **Auto-generated** by `lps-builtins-gen-app`. `#![no_std]`, no deps.
- **Flat enum** with 96 variants: 29 `LpQ32*` (Q32 math) + 67 `Lpfn*` (LPFX).
- **Methods**: `name() -> &'static str`, `builtin_id_from_name(name) -> Option`,
  `all() -> &'static [BuiltinId]`.
- **No structural metadata**: module, function name, and mode are baked into
  each variant's string; there's no `module()`, `mode()`, etc.
- **`glsl_builtin_mapping.rs`** (also generated): `glsl_q32_math_builtin_id`
  maps (GLSL name, arity) → `BuiltinId`. `glsl_lpfn_q32_builtin_id` maps
  (lpfn name, `GlslParamKind` list) → `BuiltinId`.

### Current symbol naming

- **Q32 math**: `__lp_q32_<name>` (e.g. `__lp_q32_sin`, `__lp_q32_add`)
- **LPFX**: `__lpfn_<descriptor>` (e.g. `__lpfn_fbm2_q32`, `__lpfn_hash_1`)
- Two different prefix conventions (`__lp_q32_` vs `__lpfn_`), no uniform
  structure.

### What the generator produces

`lps-builtins-gen-app/src/main.rs` (~1600 lines) walks `builtins/q32/` and
`builtins/lpfn/`, parses with `syn`, and emits:

| Output                     | Path                                                |
|----------------------------|-----------------------------------------------------|
| `BuiltinId` enum + methods | `lps-builtin-ids/src/lib.rs`                        |
| GLSL→BuiltinId mapping     | `lps-builtin-ids/src/glsl_builtin_mapping.rs`       |
| Cranelift registry         | `lps-cranelift/src/backend/builtins/registry.rs`    |
| Cranelift testcase mapping | `lps-cranelift/src/backend/builtins/mapping.rs`     |
| Emulator DCE refs          | `lps-builtins-emu-app/src/builtin_refs.rs`          |
| WASM DCE refs              | `lps-builtins-wasm/src/builtin_refs.rs`             |
| Q32 mod.rs                 | `lps-builtins/src/builtins/q32/mod.rs`              |
| WASM import valtypes       | `lps-wasm/src/codegen/builtin_wasm_import_types.rs` |
| LPFX frontend registry     | `lps-frontend/src/semantic/lpfn/lpfn_fns.rs`        |

### Builtin source files (lps-builtins)

- `builtins/q32/*.rs`: one file per Q32 op. Function identifier = symbol name
  (e.g. `pub extern "C" fn __lp_q32_sin`). `#[unsafe(no_mangle)]`.
- `builtins/lpfn/**/*.rs`: domain folders (color, generative, math, hash).
  Each has `pub extern "C" fn __lpfn_*` with `#[lpfn_impl_macro::lpfn_impl]`.
- Hash functions are mode-independent: `__lpfn_hash_1`, `_2`, `_3` (uint only).

### LPIR imports

- `ImportDecl` on `IrModule`: `module_name` + `func_name` + types.
- Naga lowering registers `std.math` imports: sin, cos, tan, asin, acos, atan,
  atan2, sinh, cosh, tanh, asinh, acosh, atanh, exp, exp2, log, log2, pow,
  ldexp, sqrt, round.
- LPFX imports: `module_name = "lpfn"`, `func_name = "{name}_{naga_index}"`.
- Text format: `import @std.math::sin(f32) -> f32`.

### WASM emitter import resolution

`imports.rs` does: `resolve_builtin_id(decl)` → matches on `module_name`:

- `"std.math"` → `glsl_q32_math_builtin_id(func_name, arity)` → `BuiltinId`
- `"lpfn"` → strip naga suffix → `glsl_lpfn_q32_builtin_id(base, kinds)`
- WASM import: `("builtins", BuiltinId::name())` e.g. `"__lp_q32_sin"`.

### WASM emitter float op handling

Not all Q32 builtins are reached via imports. Some LPIR opcodes are inlined:

- `Fadd/Fsub/Fmul` → inline Q32 i64 arithmetic (no builtin call)
- `Fdiv` → inline Q32 division
- `Fabs/Ffloor/Fceil/Ftrunc` → inline Q32 bit ops
- `Fmin/Fmax` → inline Q32 compare+select
- `Fsqrt` → calls `@std.math::sqrt` import (routed to `__lp_q32_sqrt`)
- `Fnearest` → calls `@std.math::round` import (routed to `__lp_q32_round`)

The `__lp_q32_add/sub/mul/div` builtins exist for the **old Cranelift path**
(which calls them directly for Q32 mode), not for the WASM path.

### Interpreter

`ImportHandler::call(module_name, func_name, args)` with string-based dispatch.
`StdMathHandler` matches on `module_name == "std.math"` then dispatches on
`func_name` using `libm`.

### LPIR text format

Parser: `import @{module}::{func}(...)`. Printer: same. Tests reference
`@std.math::fsin`, `@std.math::fabs`, etc. (Note: some test names use `fsin`
rather than `sin` — these are hand-written LPIR test strings, not from Naga.)

## Questions

### Q1: BuiltinId representation — enum or struct?

**Context**: Currently a flat generated enum. The roadmap wants it
"self-describing" with `module()`, `name()`, `mode()` methods.

Two options:

- **(A) Enum with generated derive methods**: Keep enum, add `module() -> Module`,
  `fn_name() -> &str`, `mode() -> Option<Mode>` as generated match arms.
  Exhaustive matching preserved. Generator derives all forms from the
  (module, name, mode) triple.
- **(B) Struct**: `BuiltinId { module: Module, name: &'static str, mode: Option<Mode> }`.
  More flexible, but loses exhaustive matching. Harder to have a compile-time
  known set.

**Answer**: (A) — enum. Exhaustive matching is valuable for
`signature_for_builtin`, `get_function_pointer`, etc. The generator already
exists and can emit the extra methods. Variant naming becomes structured:
`LpGlslSinQ32`, `LpLpirFaddQ32`, `LpLpfnFbm2Q32`, `LpLpfnHash1` (no mode).

### Q2: Classification of current Q32 math builtins

**Context**: The 29 current `LpQ32*` variants need to be split into `lpir`
(IR ops needing library impl) and `glsl` (GLSL std functions). The Cranelift
emitter will call `lpir` builtins for IR opcodes and resolve `glsl` builtins
via import declarations.

**Current LPIR opcodes** (from `op.rs`):
Fadd, Fsub, Fmul, Fdiv, Fneg, Fabs, Fsqrt, Fmin, Fmax, Ffloor, Fceil,
Ftrunc, Fnearest, plus integer and cast ops (ItofS, ItofU, FtoiSatS, FtoiSatU).

**Current LPIR imports** (from `register_std_math_imports` in `lower.rs`):
sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, asinh, acosh,
atanh, exp, exp2, log, log2, pow, ldexp, sqrt, round.

**Overlap**: `sqrt` and `round` are both IR opcodes (Fsqrt, Fnearest) AND
imports (std.math::sqrt, std.math::round). The WASM emitter routes the opcodes
through the import path.

**Not in LPIR imports** (only used by old Cranelift AST path):
add, sub, mul, div, fma, inversesqrt, mod, roundEven, acosh, asinh, atanh.
Wait — acosh/asinh/atanh ARE in the imports. So: add, sub, mul, div, fma,
inversesqrt, mod, roundEven.

**Answer — classification by "has matching LPIR opcode"**:

`lpir` (6 builtins — has matching Op):

- `add` → `__lp_lpir_fadd_q32` (Op::Fadd)
- `sub` → `__lp_lpir_fsub_q32` (Op::Fsub)
- `mul` → `__lp_lpir_fmul_q32` (Op::Fmul)
- `div` → `__lp_lpir_fdiv_q32` (Op::Fdiv)
- `sqrt` → `__lp_lpir_fsqrt_q32` (Op::Fsqrt)
- `roundeven` → `__lp_lpir_fnearest_q32` (Op::Fnearest, ties-to-even)

`glsl` (23 builtins — no matching Op):

- Trig: sin, cos, tan, asin, acos, atan, atan2
- Hyperbolic: sinh, cosh, tanh, asinh, acosh, atanh
- Exponential: exp, exp2, log, log2
- Other: pow, inversesqrt, ldexp, round, fma, mod

Key decisions:

- `sqrt` is `lpir` because LPIR has `Op::Fsqrt`, WASM has `f32.sqrt`,
  Cranelift has `sqrt`. Naga lowering imports it as `@lpir::sqrt`.
- `round` stays `glsl` — GLSL `round()` is half-away-from-zero, different
  from `Op::Fnearest` (ties-to-even).
- `roundeven` is `lpir` as `fnearest` — matches `Op::Fnearest` semantics.
- Import module matches builtin module: `@lpir::sqrt`, `@glsl::sin`,
  `@lpfn::fbm2`. No cross-module import resolution needed.

### Q3: Generator output for old Cranelift crate

**Context**: The generator currently emits `registry.rs` and `mapping.rs` into
`lps-cranelift/`. Renaming builtins will break these files. Options:

- **(A) Update generator to emit new names into old crate too**: More work,
  the old crate may need other updates to compile.
- **(B) Stop generating for old crate**: Remove the old crate outputs from the
  generator. The old crate breaks on this branch.
- **(C) Keep generating old names for old crate**: Maintain compatibility but
  defeats the purpose of unified naming.

**Answer**: (B) — stop generating for old crate. The old crate is being
abandoned. Removing its generator outputs simplifies the rename. Tests
exercising the old Cranelift path will fail — that's acceptable.

### Q4: LPIR module name rename

**Context**: Currently `std.math`. Roadmap says rename to `glsl`.

**Affected locations**:

- `lower.rs`: `register_std_math_imports` → module_name changes from
  `"std.math"` to `"glsl"`
- `lower_ctx.rs`: import_map keys change from `"std.math::{name}"` to
  `"glsl::{name}"`
- `lower_math.rs`: `push_std_math` key format changes
- `imports.rs` (WASM): `resolve_builtin_id` matches on `"glsl"` instead of
  `"std.math"`
- `StdMathHandler`: matches on `"glsl"` (or rename handler)
- `interp.rs` tests: `@std.math::fsin` → `@glsl::fsin` etc.
- `lower_print.rs` tests: assertion strings change
- `lower_interp.rs` tests: `CombinedImports` dispatch changes
- LPIR text format parser/printer: no changes needed (format is
  `@{module}::{func}`, module name is a runtime string)

**Answer**: Rename `"std.math"` → split into `"glsl"` and `"lpir"` based on
Q2 classification. `register_std_math_imports` splits registration by module.
Import map keys change accordingly (`"glsl::{name}"`, `"lpir::{name}"`).
WASM emitter matches on `"glsl"`, `"lpir"`, `"lpfn"`.
Rename `StdMathHandler` → something that handles both `"glsl"` and `"lpir"`
modules (e.g. `BuiltinImportHandler`). Straightforward search-and-replace
across affected files.

### Q5: LPFX symbol naming details

**Context**: Current LPFX symbols use `__lpfn_<descriptor>` with inconsistent
separator patterns. New convention is `__lp_lpfn_<fn>_<mode>`.

**Hash functions** are mode-independent (integer-only):

- `__lpfn_hash_1` → `__lp_lpfn_hash1` (no mode suffix)
- `__lpfn_hash_2` → `__lp_lpfn_hash2`
- `__lpfn_hash_3` → `__lp_lpfn_hash3`

**Mode-dependent LPFX** with vec variants:

- `__lpfn_saturate_q32` → `__lp_lpfn_saturate_q32`
- `__lpfn_saturate_vec3_q32` → `__lp_lpfn_saturate_vec3_q32`
- `__lpfn_hsv2rgb_vec4_q32` → `__lp_lpfn_hsv2rgb_vec4_q32`

**Tile/vec variants**:

- `__lpfn_fbm3_tile_q32` → `__lp_lpfn_fbm3_tile_q32`
- `__lpfn_srandom3_vec_q32` → `__lp_lpfn_srandom3_vec_q32`

**Answer**: Keep existing LPFX descriptors as-is, just change prefix from
`__lpfn_` to `__lp_lpfn_`. No function name changes within LPFX.
Mode suffix stays in place where present; hash keeps no mode suffix.
Examples: `__lpfn_fbm2_q32` → `__lp_lpfn_fbm2_q32`,
`__lpfn_hash_1` → `__lp_lpfn_hash_1`.

### Q6: Scope of function renaming in lps-builtins

**Context**: The actual Rust function identifiers in `lps-builtins` ARE the
symbol names (via `#[unsafe(no_mangle)]`). Renaming symbols means renaming the
Rust functions themselves.

- `builtins/q32/sin.rs`: `pub extern "C" fn __lp_q32_sin` →
  `pub extern "C" fn __lps_sin_q32`
- `builtins/lpfn/hash.rs`: `pub extern "C" fn __lpfn_hash_1` →
  `pub extern "C" fn __lp_lpfn_hash1`
- All test code referencing these functions by name needs updating.

**Answer**: Straightforward rename of Rust function identifiers.

Approach:

1. **Generated files**: Update generator logic, re-run
   `cargo run -p lps-builtins-gen-app`. Handles `lib.rs`,
   `glsl_builtin_mapping.rs`, `builtin_refs.rs`, `q32/mod.rs`,
   `builtin_wasm_import_types.rs` automatically.
2. **Source function renames in `lps-builtins`**: Unique strings, use
   text search-and-replace. Each `__lp_q32_sin` → `__lps_sin_q32` is
   a unique substitution.
3. **String references** (`imports.rs`, `StdMathHandler`, test files): Text
   search-and-replace. `rg "std.math"` finds everything.

The generator discovers functions by `fn __*` with `#[unsafe(no_mangle)]`,
still works. PascalCase derivation: `__lps_sin_q32` → `LpGlslSinQ32`.
