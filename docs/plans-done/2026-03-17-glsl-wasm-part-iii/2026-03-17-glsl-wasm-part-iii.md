# Part iii: WASM codegen — rainbow shader feature completeness

Roadmap: `docs/roadmaps/2026-03-13-glsl-wasm-playground/`

## Prerequisites

Part ii is complete: `lp-glsl-wasm` compiles scalar GLSL to valid WASM
modules, `WasmExecutable` runs them via wasmtime, the filetest runner
dispatches to both `cranelift.q32` and `wasm.q32` targets with an
annotation system tracking expected failures.

Current filetest results:

```
                  pass    fail   unimpl   broken
 cranelift.q32    1499       0      970        0
      wasm.q32       0       0     2248        0
```

## Scope

Incrementally add GLSL features to the WASM codegen until the
`rainbow.shader` example compiles and runs correctly via wasmtime.
Progress is tracked by watching the `wasm.q32` pass count rise and
`@unimplemented(backend=wasm)` annotations get removed from filetests.

End state: `rainbow.shader` compiles via `glsl_wasm()` and produces
correct pixel output when executed via wasmtime.

## Filetest granularity constraint

If any function in a filetest uses an unimplemented feature, the whole
file's compilation fails. This means we won't see filetest passes until
the basic language features work broadly enough that at least some files
contain only supported features. The phasing below prioritizes features
that appear in the most test files.

## Current codegen limitations

The WASM codegen handles:

- Literals (int, uint, float Q32, bool)
- Variables (local.get)
- Binary ops: `+`, `-` (i32 only), comparisons (`==`, `!=`, `<`, etc.)
- Unary: `-`, `!`
- Scalar variable declarations with initializers
- Return statements
- Function parameters

Missing (needed for rainbow.shader):

- **Assignment expressions** (`x = expr`)
- **Type-aware binary ops** (int mul vs Q32 float mul)
- **Q32 multiply and divide** (inline i64 intermediate)
- **User function calls** (WASM `call` with function index)
- **Control flow** (if/else, for loops)
- **Vectors** (vec2/3/4 constructors, component access, swizzle, ops)
- **Type constructors and coercion** (int→float, float(), vec3(), etc.)
- **Logical operators** (`&&`, `||`)
- **Ternary** (`? :`)
- **Const variables** (`const bool`)
- **Builtin function calls** via WASM imports
- **Out parameters** (for `lpfx_psrdnoise`)
- **Compound assignment** (`+=`, `-=`, `*=`, `/=`)

## Known bugs to fix

1. **Unary minus operand order.** Current code emits operand then 0
   then `i32.sub`, computing `x - 0 = x` instead of `0 - x = -x`.
   Must swap: emit 0 first, then operand, then `i32.sub`.

## Architecture decisions

### Type-aware expression emission

The current `emit_rvalue` returns `()` — it just pushes a value onto
the WASM stack with no type tracking. Binary ops (e.g. `*`) need to
know whether both operands are int (use `i32.mul`) or float-in-Q32
(use Q32 mul with i64 intermediate).

Add a `WasmRValue` type that `emit_rvalue` returns:

```rust
pub struct WasmRValue {
    /// GLSL type of the value(s) on the stack.
    pub ty: Type,
    /// Number of WASM values pushed onto the stack.
    /// 1 for scalars, 2-4 for vectors, etc.
    pub stack_count: u32,
}
```

This parallels lp-glsl-cranelift's `RValue` pattern. The type
information drives dispatch in binary ops, coercion, and return
handling.

### Multi-local vector representation

Vectors use multiple WASM locals (one per component):

- `vec2 v` → 2 locals: `v_0`, `v_1`
- `vec3 v` → 3 locals: `v_0`, `v_1`, `v_2`
- `vec4 v` → 4 locals: `v_0`, `v_1`, `v_2`, `v_3`

Extend `LocalInfo` to track component count and base index:

```rust
pub struct LocalInfo {
    pub base_index: u32,
    pub ty: Type,
    pub component_count: u32,  // 1 for scalar, 2-4 for vector
}
```

Component access (`.x`, `.y`) maps to `local.get(base_index + offset)`.
Vector operations operate component-wise over the local range.

WASM multi-value returns handle vec2/3/4 return types (the WASM
function signature declares multiple results).

### Q32 multiply and divide (inline)

Q32 multiply: `(i64(a) * i64(b)) >> 16`, truncated to i32.
Q32 divide: `(i64(a) << 16) / i64(b)`, truncated to i32.

These are emitted inline as WASM instructions:

```wasm
;; Q32 mul: (a * b) >> 16
local.get $a
i64.extend_i32_s
local.get $b
i64.extend_i32_s
i64.mul
i64.const 16
i64.shr_s
i32.wrap_i64
```

No imports needed. This matches the semantics of the Cranelift Q32
strategy's wrapping mode. Saturating modes can be added later.

### Builtin function calls via WASM imports

Generated WASM modules declare builtins as imports:

```wasm
(import "builtins" "__lp_q32_sin" (func $__lp_q32_sin (param i32) (result i32)))
(import "builtins" "__lp_q32_cos" (func $__lp_q32_cos (param i32) (result i32)))
```

The import module name is `"builtins"`. Function names match the
`BuiltinId::name()` strings from `lp-glsl-builtin-ids`.

For wasmtime tests, imports are satisfied by host functions that call
the native Rust implementations in `lp-glsl-builtins`. Phase iv
replaces these with a precompiled `lp-glsl-builtins.wasm` module for
the browser.

The codegen tracks which builtins are used during compilation and only
emits imports for those actually needed.

### Out parameters via WASM linear memory

Out/inout parameters require memory for passing mutable references.
Allocate a WASM linear memory with a simple bump allocator. Out params
get a pointer to a memory slot; the caller reads back after the call.

This is only needed for `lpfx_psrdnoise` (which has a `gradient` out
parameter) in rainbow.shader. Implement as needed rather than up-front.

## Phases

### Phase 1: Type-aware expressions + scalar fixes

1. Introduce `WasmRValue` type returned by `emit_rvalue`.
2. Fix unary minus operand order.
3. Add type inference to binary op dispatch: if both operands are
   `Int`/`UInt`/`Bool`, use integer WASM ops; if either is `Float`,
   use float-mode ops (Q32 or f32).
4. Implement integer `*`, `/`, `%` (`i32.mul`, `i32.div_s`,
   `i32.rem_s`).
5. Implement Q32 float multiply (inline i64 intermediate).
6. Implement Q32 float divide (inline i64 intermediate).
7. Implement Q32 modulo (`a - floor(a/b) * b`).
8. Implement assignment expressions (`Expr::Assignment`):
   `emit_rvalue(rhs)`, `local.tee(idx)` for simple variables.
9. Implement compound assignment (`+=`, `-=`, `*=`, `/=`):
   `local.get(idx)`, emit rhs, emit op, `local.set(idx)`.
10. Validate: int/float scalar tests passing.

### Phase 2: Type constructors, coercion, logical ops

1. Implement scalar type constructors (`int()`, `float()`, `bool()`,
   `uint()`) — these are `FunCall` expressions with type names.
2. Implement implicit type coercion: int→float (Q32: `i32.const(16)`,
   `i32.shl`; float: `f32.convert_i32_s`). bool→int (identity in i32).
3. Implement logical `&&` with short-circuit evaluation (WASM `if`).
4. Implement logical `||` with short-circuit evaluation.
5. Implement ternary `? :` (WASM `if/else/end` with value).
6. Validate: bool tests, type conversion tests passing.

### Phase 3: Control flow

1. Implement if/else: WASM `if/else/end` (structured control flow maps
   directly from GLSL's structured if/else).
2. Implement for loops: WASM `block { loop { ... br_if ... br ... } }`.
   Init runs before the block. Condition is `br_if` to exit. Update
   runs at end of loop body. `br` back to loop header.
3. Implement while loops: same pattern as for without init/update.
4. Implement do-while loops: `block { loop { body... condition...
   br_if 0 (back to loop)... } }`.
5. Implement break (`br` to enclosing block) and continue (`br` to
   loop header).
6. Validate: control flow tests passing.

### Phase 4: User function calls

1. Build a function index map during module compilation: each function
   gets a WASM function index (imports first, then user functions).
2. Implement `Expr::FunCall` for user-defined functions: look up
   function index, emit arguments, `call $func_idx`.
3. Handle return value types (scalar returns are single values).
4. Implement `const` variable declarations: evaluate at compile time
   or emit as regular locals initialized with const values. `const bool`
   specifically needed for rainbow.shader's `CYCLE_PALETTE`.
5. Implement global const references in expressions.
6. Validate: function call tests, const tests passing.

### Phase 5: Vectors

1. Extend `LocalInfo` with component count, extend `add_local` to
   allocate multiple WASM locals for vec2/3/4.
2. Extend type mapping: vectors → multiple WASM values.
3. Implement vector constructors:
    - `vec2(x, y)`, `vec3(x, y, z)`, `vec4(x, y, z, w)` — push
      each component.
    - `vec3(scalar)` — replicate scalar to all components.
    - `vec3(vec2, scalar)`, `vec4(vec3, scalar)`, etc. — mixed.
4. Implement component access (`.x`, `.y`, `.z`, `.w`) via
   `local.get(base + offset)`.
5. Implement swizzle (`.xy`, `.rgb`, `.xyzw`, etc.) — emit multiple
   `local.get` calls for each swizzle component.
6. Implement vector variable load/store: emit `local.get`/`local.set`
   for each component.
7. Implement vector arithmetic (component-wise): `vec + vec`,
   `vec - vec`, `vec * vec`, `vec / vec`.
8. Implement scalar-vector promotion: `scalar * vec` → replicate
   scalar, then component-wise mul. Also `vec * scalar`.
9. Implement vector assignment and compound assignment.
10. Implement vector return (WASM multi-value return: function
    signature declares N results, return pushes N values).
11. Implement vector parameters (function signature with N params per
    vector, caller pushes components individually).
12. Implement vector comparison (`==`, `!=`): component-wise compare,
    reduce with `&&`.
13. Validate: vec2/3/4 tests, ivec, uvec, bvec tests passing.

### Phase 6: Builtin functions via WASM imports

1. Add import section handling to the module builder. Track used
   builtins during codegen, emit import declarations only for those
   actually called.
2. Implement builtin call emission: when a `FunCall` resolves to a
   builtin, emit `call $import_idx` with the builtin's import index.
3. Map GLSL builtin names to `BuiltinId` for the import name.
4. In the wasmtime `WasmExecutable`, provide host functions for each
   declared import. The host functions call the native Rust builtin
   implementations from `lp-glsl-builtins`.
5. Handle vector-argument builtins: `clamp(vec3, float, float)`,
   `mix(vec3, vec3, float)` — may need flattened signatures
   (each component as a separate i32 param) or memory-based passing.
6. Implement the standard builtins needed by rainbow.shader:
   `clamp`, `abs`, `mod`, `fract`, `floor`, `exp`, `cos`, `sin`,
   `smoothstep`, `mix`, `atan`, `min`.
7. Validate: builtin tests passing.

### Phase 7: LPFX functions + out parameters

1. Implement WASM linear memory: declare a memory section, implement a
   simple bump allocator for out-parameter slots.
2. Implement out/inout parameters: caller allocates memory slot, passes
   pointer as i32 param. Callee writes via `i32.store`. Caller reads
   back via `i32.load`.
3. Implement LPFX function calls (`lpfx_worley`, `lpfx_fbm`,
   `lpfx_psrdnoise`) as WASM imports, same as standard builtins.
4. Handle LPFX vector returns (these return vec2/vec3 — need multi-value
   or memory-based returns from imports).
5. Validate: LPFX tests passing.

### Phase 8: Rainbow shader end-to-end

1. Compile `rainbow.shader` via `glsl_wasm()`. Fix any remaining
   compilation errors.
2. Execute all functions via wasmtime. Compare output to the Cranelift
   Q32 backend for a set of sample inputs (fragCoord, outputSize, time).
3. Write an integration test that compiles rainbow.shader and verifies
   `main()` output for several pixel/time combinations.
4. Remove `@unimplemented(backend=wasm)` from all filetests that now
   pass.
5. Run full filetest suite, verify no regressions on cranelift.q32.

### Phase 9: Cleanup and validation

1. Run `cargo build` (full workspace).
2. Run `cargo test` (full workspace).
3. Run `cargo +nightly fmt`.
4. Fix any warnings.
5. Verify `just build-fw-esp32` still works.
6. Update READMEs:
    - `lp-shader/lp-glsl-wasm/README.md`: document supported features,
      builtin import mechanism, vector representation.
    - `lp-shader/lp-glsl-filetests/README.md`: update with current
      wasm.q32 pass counts and annotation patterns.
    - `lp-shader/README.md`: update crate table if needed.

## Feature → filetest mapping (approximate)

| Feature                    | Test directories unlocked           | Est. tests |
|----------------------------|-------------------------------------|------------|
| Assignment, int mul/div    | scalar/int/*, scalar/uint/*         | ~28        |
| Q32 float mul/div          | scalar/float/*                      | ~13        |
| Type constructors/coercion | scalar/*/from-*.glsl                | ~12        |
| Logical ops, ternary       | scalar/bool/*, control/ternary/*    | ~29        |
| If/else                    | control/if/*, control/if_else/*     | ~10        |
| For/while/do-while         | control/for/*, control/while/*, etc | ~40        |
| User function calls        | function/*                          | ~40        |
| Vectors                    | vec/*, uvec*/*                      | ~200+      |
| Builtins                   | builtins/*                          | ~63        |
| Matrices                   | matrix/*                            | ~68        |

Note: matrices are NOT required for rainbow.shader and are out of scope
for this plan. They can remain `@unimplemented(backend=wasm)`.

## Validate

```
cargo build
cargo test
cargo build -p lp-glsl-wasm
cargo test -p lp-glsl-wasm
cargo test -p lp-glsl-filetests
scripts/glsl-filetests.sh
cargo +nightly fmt --check
just build-fw-esp32
```

Target: wasm.q32 pass count should be in the hundreds (all non-matrix,
non-array, non-struct tests). Rainbow shader compiles and runs correctly.

## Risk

**Medium-high.** This is the largest phase — it's the bulk of the WASM
codegen implementation. Risk areas:

- **Vector multi-local representation.** Component-wise codegen for
  every expression type is a large surface area. Swizzle and mixed
  constructors are particularly fiddly.

- **Builtin import ABI.** Vector-argument builtins (e.g. `clamp(vec3,
  float, float)`) need careful ABI design. The Cranelift backend passes
  vectors as multiple scalar parameters (sret for return). The WASM
  backend must match whatever the builtins expect.

- **Out parameters and linear memory.** Introducing WASM memory adds
  complexity (memory management, load/store patterns). Only needed for
  `lpfx_psrdnoise`'s gradient out param, so scope is limited.

- **Short-circuit evaluation.** `&&` and `||` require structured
  control flow (WASM `if`) to avoid evaluating the right operand when
  unnecessary. This interacts with the expression emission model.

## Non-goals

- Matrix support (mat2/3/4) — not needed for rainbow.shader
- Array support — not needed for rainbow.shader
- Struct support — not needed for rainbow.shader
- Float numeric mode — Q32 only for now
- Browser/playground integration — that's phase iv
- Performance optimization — correctness first
