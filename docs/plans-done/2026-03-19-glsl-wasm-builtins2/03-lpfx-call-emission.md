# Phase 3: LPFX call emission

## Scope of phase

Implement `lpfx_call.rs` for LPFX function calls in the WASM backend. Add `memory.rs` for out-param
offset constants. Wire into FunCall dispatch. After this phase, `lpfx_worley`, `lpfx_fbm`, and
`lpfx_psrdnoise` (with `out vec2 gradient`) all compile and run under wasmtime with `builtins.wasm`.

This is the hardest phase. The LPFX ABI differs from Q32 math imports (pre-flattened mixed types,
out-param pointers, memory reads).

## Code organization reminders

- New `lpfx_call.rs` alongside `builtin_call.rs` — separate calling conventions.
- New `memory.rs` for out-param offset constants — keep magic numbers in one place.
- Tests first in test modules; helpers at bottom.
- Temporary debug prints should have a TODO comment.

## Implementation details

### 1. `memory.rs`

File: `lp-shader/lp-glsl-wasm/src/codegen/memory.rs`

Minimal — just constants for the out-param scratch region:

```rust
/// Base offset in shared linear memory for LPFX out-parameter scratch.
/// Currently only psrdnoise gradient (2x i32 = 8 bytes).
pub const LPFX_OUT_PARAM_BASE: u32 = 0;

/// Size of the out-param scratch region in bytes.
pub const LPFX_OUT_PARAM_SIZE: u32 = 8;
```

### 2. FunCall dispatch

File: `lp-shader/lp-glsl-wasm/src/codegen/expr/mod.rs`

Add LPFX branch after the Q32 math import branch:

```rust
} else if options.float_mode == FloatMode::Q32
    && lpfx_fn_registry::is_lpfx_fn(name)
{
    lpfx_call::emit_lpfx_call(ctx, sink, expr, name, args, options)
} else {
    // error
}
```

Add `mod lpfx_call;` and import `lpfx_fn_registry`.

### 3. `lpfx_call.rs`

File: `lp-shader/lp-glsl-wasm/src/codegen/expr/lpfx_call.rs`

Main entry point:
`emit_lpfx_call(ctx, sink, full_call, name, args, options) -> Result<WasmRValue, GlslDiagnostics>`

#### Algorithm

1. **Resolve function:** Infer GLSL arg types → `find_lpfx_fn(name, &arg_types)` → get `LpfxFn` with
   GLSL signature and `BuiltinId`. Extract Q32 impl id.

2. **Get import index:** `ctx.builtin_func_index.get(&builtin_id)`.

3. **Get WASM import signature:** `wasm_import_val_types(builtin_id)` → `(params, results)`.

4. **Emit args:** Iterate over `func.glsl_sig.parameters` alongside the GLSL args:
    - `In`, scalar (`Float`/`Int`/`UInt`) → `emit_rvalue(arg)` — pushes 1 value
    - `In`, vector (`Vec2`/`Vec3`/`Vec4`) → `emit_rvalue(arg)` — pushes N components
    - `Out`, vector → `i32.const LPFX_OUT_PARAM_BASE` — pushes 1 pointer value

5. **Emit call:** `sink.call(func_idx)` — for scalar-return functions, result is on stack.

6. **Handle out-param writeback:** After the call, for each `Out` param:
    - Determine the GLSL local variable the out param refers to (it must be an `Expr::Variable`)
    - Load from memory: `i32.load(LPFX_OUT_PARAM_BASE)`, `i32.load(LPFX_OUT_PARAM_BASE + 4)` for
      vec2
    - Store to the variable's locals: `local.set(var_local + 0)`, `local.set(var_local + 1)`
    - Note: the scalar return value is still on the stack from the call, and the loads push
      additional values. The out-param loads and stores must happen without disturbing the return
      value. Use a scratch local to save the return value if needed, or do the loads/stores before
      consuming the return value.

#### Rainbow LPFX calls (concrete examples)

**`lpfx_worley(scaledCoord * 2, 0u)`**

- GLSL: `[vec2, uint]` → `LpfxWorley2Q32`
- WASM params: `[i32, i32, i32]` (p.x, p.y, seed)
- Emit: `emit_rvalue(scaledCoord * 2)` → 2 i32 on stack; `emit_rvalue(0u)` → 1 i32; `call`
- Return: 1 i32 (scalar float)

**`lpfx_fbm(scaledCoord, 3, 0u)`**

- GLSL: `[vec2, int, uint]` → `LpfxFbm2Q32`
- WASM params: `[i32, i32, i32, i32]` (p.x, p.y, octaves, seed)
- Emit: `emit_rvalue(scaledCoord)` → 2 i32; `emit_rvalue(3)` → 1 i32; `emit_rvalue(0u)` → 1 i32;
  `call`
- Return: 1 i32

**`lpfx_psrdnoise(scaledCoord, vec2(0.0), time, gradient, 0u)`** (after phase 1 seed fix)

- GLSL: `[vec2, vec2, float, out vec2, uint]` → `LpfxPsrdnoise2Q32`
- WASM params: `[i32, i32, i32, i32, i32, i32, i32]` (x, y, period_x, period_y, alpha, gradient_ptr,
  seed)
- Emit: `emit_rvalue(scaledCoord)` → 2 i32; `emit_rvalue(vec2(0.0))` → 2 i32; `emit_rvalue(time)` →
  1 i32; `i32.const 0` (gradient ptr); `emit_rvalue(0u)` → 1 i32 (seed); `call`
- Return: 1 i32 (noise value) on stack
- Post-call: save return value to scratch; `i32.load offset=0` → `local.set(gradient.x)`;
  `i32.load offset=4` → `local.set(gradient.y)`; reload return value from scratch

#### Out-param writeback detail

The tricky part: after `call`, the scalar return value is on the WASM stack. We need to:

1. `local.tee(scratch)` — save return value
2. `drop` — clear the stack (or use `local.set` instead of `tee`)
3. `i32.const 0` + `i32.load offset=0` → `local.set(gradient_local_x)`
4. `i32.const 0` + `i32.load offset=4` → `local.set(gradient_local_y)`
5. `local.get(scratch)` — put return value back on stack

Resolving the `gradient` variable to its local indices: the out-param arg expression must be
`Expr::Variable(ident)`. Look up `ident` in `ctx` to get the base local index. For `vec2`, that's 2
consecutive locals.

If the out-param expression is not a simple variable (e.g. `out` to a swizzle or array element),
error for now — Rainbow only uses `gradient` as a plain `vec2` local.

### 4. Tests

In `lp-glsl-wasm/tests/basic.rs` (compile-only):

- `test_lpfx_worley_compiles` — shader calling `lpfx_worley(vec2(1.0, 2.0), 0u)` compiles
- `test_lpfx_fbm_compiles` — shader calling `lpfx_fbm(vec2(1.0, 2.0), 3, 0u)` compiles
- `test_lpfx_psrdnoise_compiles` — shader calling
  `lpfx_psrdnoise(vec2(1.0), vec2(0.0), 0.5, gradient, 0u)` compiles, produces correct imports
  including memory

In `lp-glsl-wasm/tests/q32_builtin_link.rs` (linked execution):

- `test_lpfx_worley_linked` — compile + link + run, verify non-zero return
- `test_lpfx_psrdnoise_linked` — compile + link + run, verify gradient local is written

## Validate

```bash
cd lp-glsl && cargo test -p lp-glsl-wasm
cargo +nightly fmt
```

No new warnings. LPFX imports appear in the module's import section alongside any Q32 math imports.
