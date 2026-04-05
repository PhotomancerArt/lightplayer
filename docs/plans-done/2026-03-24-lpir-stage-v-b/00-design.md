# Stage V-B: Filetest Failure Fixes — Design

## Scope

Fix ~40 failing GLSL filetests: 5 correctness bugs (P0), test maintenance
(P2). Deferred: LPFX overloads, array types, Q32 round midpoint (P1).

## File structure

```
lp-shader/
├── lp-glsl-wasm/src/emit/
│   └── control.rs                # FIX: continue depth calculation
├── lp-glsl-naga/src/
│   ├── lower.rs                  # UPDATE: two-pass function registration
│   ├── lower_ctx.rs              # UPDATE: handle Pointer-typed args (inout)
│   ├── lower_stmt.rs             # UPDATE: inout call-site slot plumbing
│   ├── lower_expr.rs             # UPDATE: Load/Store through pointer args
│   └── expr_scalar.rs            # UPDATE: As with Bool target type
└── lp-glsl-filetests/filetests/
    ├── function/overload-ambiguous.glsl    # FIX: stale run directives
    ├── function/recursive-static-error.glsl # FIX: stale run directives
    ├── debug/rainbow.glsl                  # FIX: bless expected values
    └── scalar/float/from-int.glsl          # FIX: expectation for INT_MAX
```

## Bug analysis

### Bug 1 — `continue` in nested loops (WASM emitter)

Cranelift passes; WASM fails. The LPIR is correct.

`control.rs::innermost_loop_continue_depth` always returns `Ok(0)`. When
`continue` is inside an `if` or nested construct within a loop body, `br(0)`
exits the `if` block instead of the loop's inner body block.

**Fix:** Change `innermost_loop_continue_depth` to accept `wasm_open` and
compute `wasm_open - (outer_open_depth + 2)`. The `+2` accounts for the
`loop` and inner `block` above the outer break `block`.

```
WASM loop structure:
  block $break          ; outer_open_depth = here
    loop $loop          ; +1
      block $body       ; +2 — continue targets this block's end
        if              ; +3
          continue      ; br(wasm_open - (outer_open_depth + 2))
                        ;   = (outer+3) - (outer+2) = 1  ✓
        end
      end $body         ; (closed at continuing_offset)
      ...continuing...
      br 0              ; back to $loop
    end $loop
  end $break
```

### Bug 2 — Bool casts (Naga lowering)

`As` expressions with Bool target are rejected as "non-32-bit byte convert".

**Fix:** In `expr_scalar.rs`, add a case for `ScalarKind::Bool` target:

- `bool(float)` in Q32 → `Ine(src, const_0)` (Q32 zero is 0i32)
- `bool(int)` / `bool(uint)` → `Ine(src, const_0)`
- `bool(bool)` → identity `Copy`

### Bug 3 — Ternary implicit float→int conversion

`test_ternary_float_to_int_conversion` returns 675020 (raw Q32 of 10.3)
instead of 10. The Naga `As` wrapping the ternary result isn't applied.

**Root cause:** Likely the same As-handling code path as Bug 2 — the As
expression that converts the ternary result from float to int may be hitting
the same "non-32-bit" error or a Q32 code path that doesn't truncate.

**Fix:** Verify after Bug 2 fix. If still broken, trace the specific Naga
expression tree for this test case.

### Bug 4 — Forward declarations

Functions defined after their call site produce "undefined function" errors.

**Fix:** Two-pass lowering in `lower.rs`:

1. First pass: iterate `naga_module.functions`, register each function's
   `CalleeRef` in `func_map`. (This already happens — lines 26-29.)
2. The actual bug may be in the Naga frontend (function parsing order) rather
   than lowering. Need to verify where `naga_module.functions` comes from
   and whether prototype-only declarations are included.

If the issue is that Naga doesn't include prototype-defined functions in
`module.functions` until their body is parsed: the fix is in the GLSL
frontend or in how `NagaModule::functions` is populated.

### Bug 5 — `inout` parameters (Pointer types)

**Strategy:** Slot-based copy-in/copy-out (matches Cranelift ABI).

**Changes needed:**

**`lower_ctx.rs`** — In `LowerCtx::new`, when a function argument has type
`Pointer { base, space: Function }`:

- Resolve the base type to IR types.
- Allocate an LPIR **slot** (not a vreg) for the parameter.
- Add a single `IrType::I32` param (the address) to the function signature.
- In the function prologue, store the address into a local vreg.

**`lower_expr.rs`** — When `Expression::Load { pointer }` hits a
`FunctionArgument` that's a pointer param:

- Emit `Op::Load` from the address vreg.

**`lower_stmt.rs`** — `Statement::Store` to a pointer-typed FunctionArgument:

- Emit `Op::Store` to the address vreg.

**`lower_stmt.rs`** — At call sites (`lower_user_call`), for `inout`/`out`
arguments:

- Allocate a slot in the caller.
- For `inout`: store the current value into the slot.
- Pass `SlotAddr` as the argument.
- After the call: load from the slot back into the local variable's vregs.

### Test maintenance (P2)

- **Stale `@unimplemented` markers:** Run `--fix` on all filetests.
- **Commented-out function refs:** Fix run directives in
  `overload-ambiguous.glsl` and `recursive-static-error.glsl`.
- **Rainbow blessed values:** Re-bless from cranelift.q32 output.
- **float(INT_MAX) expectation:** Update to ~= 32768.0 (Q32_MAX rounds up).
