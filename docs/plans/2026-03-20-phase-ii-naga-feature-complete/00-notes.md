# Phase II: Naga WASM Feature Complete — Notes

## Scope

Expand the Naga WASM backend from scalar-only to full feature set. End state:
`rainbow.glsl` renders in the web demo. All existing wasm.q32 filetests pass.

Part of: `docs/roadmaps/2026-03-20-naga/phase-ii.md`

## Current state (post Phase I)

- `lp-glsl-naga` parses GLSL → `naga::Module` + `FunctionInfo` metadata
- `lp-glsl-wasm` emits WASM from `naga::Module` for scalars only
- **432/432** scalar filetests pass on `wasm.q32`

## Phase 01 progress (control flow)

- `Break` / `Continue` with `EmitCtx` (`loop_stack`, `break_target_depth`,
  `body_entry_depth`, `if` depth). `Continue` uses `br(0)` when
  `depth < body_entry_depth` (inside Naga `continuing`).
- `LogicalNot` on `Sint` / `Uint` → `i32.eqz` (Naga comparison as int).
- Do-while: split trailing `if (!cond) { break; }` out of the inner
  `continue` block (see `01-control-flow-fixes.md`, section 4).
- `scalar/bool/ctrl-do-while.glsl`: wasm `@ignore` removed; passes.
- Full `control/` still mostly `@ignore(backend=wasm)` until remaining phases;
  `control/do_while/continue.glsl` verified on `wasm.q32`.
- **154 test files** failing, **1210 unexpected test failures** remaining
- `Break`/`Continue` statements not emitted
- No vectors, builtins, function calls, LPFX

## Failure categories (154 files, by directory)

```
 30 builtins      — Expression::Math not handled
 13 lpfx          — Statement::Call + WASM imports not handled
 10 control/while — Break/Continue missing
  9 control/do_while — Break/Continue missing
  8 function      — Statement::Call (user fn) + CallResult not handled
  3 control/ternary — Expression types / LogicalNot for non-Bool
  1 control/if    — minor (LogicalNot for Sint)
  1 array         — arrays not supported (out of Phase II scope)
  1 debug/rainbow — needs everything
  ~80 vec/*       — Compose, Splat, Swizzle, AccessIndex, multi-return not handled
```

## Naga IR constructs needed

### Vectors (scalarized emission)

WASM has no vector instructions; vectors are N flat scalars on the WASM stack
(multi-value returns) and in WASM locals.

Key expressions:
- `Expression::Compose { ty, components }` — build vec from scalar handles
- `Expression::Splat { size, value }` — broadcast scalar to vec
- `Expression::Swizzle { size, vector, pattern }` — reorder/extract components
- `Expression::AccessIndex { base, index }` — extract single component
- `Expression::Access { base, index }` — dynamic component access

Scalarized strategy: when emitting a vector expression, emit each component
in sequence. A vec3 produces 3 values on the WASM stack. Local variables for
vectors use N consecutive WASM locals.

### Builtins (Expression::Math)

`Expression::Math { fun, arg, arg1, arg2, arg3 }` with `fun: MathFunction`.

Key builtins for rainbow.glsl:
- `Floor`, `Fract`, `Abs`, `Clamp`, `Min`, `Max`, `Mix`, `SmoothStep`, `Step`
- `Sin`, `Cos`, `Atan2`
- `Exp`, `Log`, `Pow`, `Sqrt`, `InverseSqrt`
- `Mod` (handled as `Binary::Modulo` by naga)

Q32 mode: these need WASM imports to `builtins` module (calling the
`lp-glsl-builtins-wasm` implementation). Map `MathFunction` variants to
`BuiltinId` for the import name.

Float mode: use `f32.floor`, `f32.sqrt` etc. where WASM has native ops;
import the rest.

### User-defined function calls

`Statement::Call { function, arguments, result }`:
- Emit argument expressions
- Emit `call $func_idx`
- If `result` is `Some(h)`: `h` is `Expression::CallResult(func_handle)`.
  The call leaves the result on the stack; subsequent use of that handle
  emits the stored value.

Requires: a function index map (`Handle<Function>` → WASM func index).

### LPFX builtins (external imports)

`lpfx_*` functions (e.g. `lpfx_psrdnoise`, `lpfx_worley`, `lpfx_fbm`)
appear as `Statement::Call` to functions declared via prototypes prepended
to the source. Their Naga `Handle<Function>` points to a function with a
body (the stub `void main() {}` pattern doesn't apply; Naga must see their
declarations).

Approach:
1. In `lp-glsl-naga`: prepend GLSL forward declarations for all LPFX
   functions before parsing. Use `#line 1` after prototypes to reset line
   numbers.
2. In `lp-glsl-wasm`: detect LPFX calls by function name prefix (`lpfx_`)
   or by matching against a known list. Emit as WASM imports to the
   `builtins` module instead of intra-module calls.

### Control flow

`Statement::Break` and `Statement::Continue` are already partially modeled
by the `Loop` structure. In Naga's IR:
- `while(cond) { body }` becomes `Loop { body: [if(!cond) break; ...body], continuing: [] }`
- `for(init; cond; inc) { body }` becomes `init; Loop { body: [if(!cond) break; ...body], continuing: [inc] }`
- `break` → `Statement::Break` inside the body block
- `continue` → `Statement::Continue` exits the body block to `continuing`

Current emit.rs has `Loop` but returns error on `Break`/`Continue`.
Fix: `Break` → `br 2` (exit the outer `block`), `Continue` → `br 0`
(re-enter inner `block` which jumps to continuing).

Actually re-reading `emit.rs`: the current loop structure is:
```
block $exit
  loop $loop
    block $body
      <body>
    end $body
    <continuing>
    <break_if → br_if $exit>
    br $loop
  end $loop
end $exit
```

So `break` = `br 2` (to `$exit`), `continue` = `br 0` (to end of `$body`,
falls through to `continuing`).

### `out` parameters

`rainbow.glsl` uses `out vec2 gradient` in `lpfx_psrdnoise`. Naga may model
this as a pointer parameter or as a local variable with post-call store-back.
Need to check how Naga's GLSL frontend handles `out` parameters for external
(stub) functions.

### Web demo integration

`lp-app/web-demo/src/lib.rs` calls `glsl_wasm(source, options)` and returns
`module.bytes`. It already compiles after Phase I (`GlslWasmError` handling).
For Phase II, the emitted WASM will include `builtins` imports; the web-demo
JavaScript must provide those imports at instantiation time. The web-demo
`www/index.html` already loads `lp_glsl_builtins_wasm.wasm` for this purpose.

## Questions

### Q1: How to handle Expression::Math for Q32?

**Context**: In Q32 mode, `floor(x)` on a Q16.16 value is `x & 0xFFFF0000`.
`sin(x)`, `cos(x)`, etc. need lookup tables or polynomial approximations in
the `lp-glsl-builtins-wasm` WASM module.

**Approach**: Emit WASM imports for all `MathFunction` variants in Q32 mode.
Map `MathFunction::Floor` → import `__lp_floor`, `MathFunction::Sin` →
import `__lp_sin`, etc. The `lp-glsl-builtins-wasm` crate already provides
these.

For Float mode, use native WASM instructions where available (`f32.floor`,
`f32.sqrt`, `f32.nearest`, `f32.ceil`, `f32.trunc`, `f32.abs`,
`f32.copysign`, `f32.min`, `f32.max`) and import the rest.

### Q2: LPFX prototype injection — format?

**Context**: Need to tell Naga about `lpfx_*` functions so it can parse calls
to them without seeing their bodies.

**Approach**: Prepend GLSL function prototypes:
```glsl
float lpfx_psrdnoise(vec2 pos, vec2 per, float rot, out vec2 gradient, uint quality);
float lpfx_worley(vec2 pos, uint quality);
float lpfx_fbm(vec2 pos, int octaves, uint quality);
// ... etc
#line 1
```

The `#line 1` directive resets line numbers so error messages reference the
user's source correctly.

### Q3: Vector local allocation?

**Context**: A `vec3` local needs 3 consecutive WASM locals. The current
`LocalAlloc` allocates 1 local per `LocalVariable` handle.

**Approach**: In `LocalAlloc::new()`, when the `LocalVariable`'s type is a
vector, allocate N consecutive locals (N = dimension). The `resolve` method
returns the base index; vector component k is at `base + k`.

### Q4: How should multi-value returns work?

**Context**: A function returning `vec3` needs to return 3 i32/f32 values.
WASM multi-value is supported.

**Approach**: The function type already declares multiple results via
`glsl_type_to_wasm_components()`. For `Return { value }`, emit the vector
expression (which pushes N values), then `return`.

### Q5: What about `CallResult` for vector-returning functions?

**Context**: `Statement::Call { result: Some(h) }` where `h` is
`Expression::CallResult(func)`. The call leaves N values on the stack for
a vec return. Subsequent `emit_expr(h)` must retrieve those N values.

**Approach**: After the call, store the N result values into N temporary
locals. When `emit_expr(CallResult(func))` is encountered, emit
`local.get` for each stored local.

This requires pre-allocating "call result temp" locals for each `Call`
statement that has a result.
