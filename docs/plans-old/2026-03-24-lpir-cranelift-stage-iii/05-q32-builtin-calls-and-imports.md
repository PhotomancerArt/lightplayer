# Phase 5: Q32 Builtin Calls and Import Calls

## Scope

Wire up Q32 float ops that use builtins (fadd, fsub, fmul, fdiv, sqrt,
fnearest) to call `__lp_lpir_*_q32` via FuncRef. Enable import calls
(`Op::Call` where `callee < import_count`) for `glsl::*` and `lpfn::*`
builtins. End-to-end Q32 tests with builtin math.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. LPIR float ops → builtin calls in `emit/scalar.rs`

For the 6 ops with Q32 builtins, the Q32 path emits a function call instead
of a native CLIF instruction. The FuncRef comes from `EmitCtx`.

The challenge: these are LPIR *opcodes* (Fadd, Fsub, etc.), not LPIR imports.
They don't have a `CalleeRef`. We need FuncRefs for the `__lp_lpir_*_q32`
builtins declared in the module.

Approach: in `jit_module.rs`, when `FloatMode::Q32`, declare the 6 LPIR
builtins explicitly and store their FuncRefs in `EmitCtx`:

```rust
pub(crate) struct LpirBuiltinRefs {
    pub fadd: FuncRef,
    pub fsub: FuncRef,
    pub fmul: FuncRef,
    pub fdiv: FuncRef,
    pub fsqrt: FuncRef,
    pub fnearest: FuncRef,
}
```

Add to `EmitCtx`:

```rust
pub(crate) struct EmitCtx<'a> {
    // ... existing fields ...
    pub lpir_builtins: Option<LpirBuiltinRefs>,
}
```

`None` in F32 mode, `Some(refs)` in Q32 mode.

#### Declaring LPIR builtins

In `jit_module.rs`, after creating the module and before the per-function loop:

```rust
let lpir_builtin_func_ids = if mode == FloatMode::Q32 {
    Some(builtins::declare_lpir_builtins(&mut jit_module)?)
} else {
    None
};
```

In `builtins.rs`:

```rust
pub(crate) struct LpirBuiltinFuncIds {
    pub fadd: FuncId,
    pub fsub: FuncId,
    pub fmul: FuncId,
    pub fdiv: FuncId,
    pub fsqrt: FuncId,
    pub fnearest: FuncId,
}

pub(crate) fn declare_lpir_builtins(
    module: &mut JITModule,
) -> Result<LpirBuiltinFuncIds, CompileError> {
    let call_conv = module.isa().default_call_conv();
    let sig_2_1 = q32_sig(call_conv, 2, 1); // (i32, i32) -> i32
    let sig_1_1 = q32_sig(call_conv, 1, 1); // (i32) -> i32

    Ok(LpirBuiltinFuncIds {
        fadd: declare_one(module, "__lp_lpir_fadd_q32", &sig_2_1)?,
        fsub: declare_one(module, "__lp_lpir_fsub_q32", &sig_2_1)?,
        fmul: declare_one(module, "__lp_lpir_fmul_q32", &sig_2_1)?,
        fdiv: declare_one(module, "__lp_lpir_fdiv_q32", &sig_2_1)?,
        fsqrt: declare_one(module, "__lp_lpir_fsqrt_q32", &sig_1_1)?,
        fnearest: declare_one(module, "__lp_lpir_fnearest_q32", &sig_1_1)?,
    })
}
```

Per-function, create FuncRefs from the FuncIds:

```rust
let lpir_builtins = lpir_builtin_func_ids.as_ref().map(|ids| {
    LpirBuiltinRefs {
        fadd: jit_module.declare_func_in_func(ids.fadd, builder.func),
        fsub: jit_module.declare_func_in_func(ids.fsub, builder.func),
        // ...
    }
});
```

#### Emitting builtin calls in scalar.rs

```rust
Op::Fadd { dst, lhs, rhs } => {
    let a = use_v(builder, vars, *lhs);
    let b = use_v(builder, vars, *rhs);
    match ctx.float_mode {
        FloatMode::F32 => def_v_expr(builder, vars, *dst, |bd| bd.ins().fadd(a, b)),
        FloatMode::Q32 => {
            let refs = ctx.lpir_builtins.as_ref().expect("Q32 builtins");
            let call = builder.ins().call(refs.fadd, &[a, b]);
            let result = builder.inst_results(call)[0];
            def_v(builder, vars, *dst, result);
        }
    }
}
```

Same pattern for Fsub, Fmul, Fdiv (binary), Fsqrt, Fnearest (unary).

### 2. Import calls in `emit/call.rs`

The existing Call handler rejects imports. Update it to use
`ctx.import_func_refs`:

```rust
Op::Call { callee, args, results } => {
    let import_count = ctx.ir.imports.len() as u32;
    let func_ref = if callee.0 < import_count {
        ctx.import_func_refs[callee.0 as usize]
    } else {
        let local_idx = (callee.0 - import_count) as usize;
        ctx.func_refs[local_idx]
    };

    let arg_vals: Vec<Value> = func.pool_slice(*args)
        .iter()
        .map(|v| use_v(builder, vars, *v))
        .collect();

    let call = builder.ins().call(func_ref, &arg_vals);
    let result_regs = func.pool_slice(*results);
    let result_vals = builder.inst_results(call).to_vec();
    for (vreg, val) in result_regs.iter().zip(result_vals) {
        def_v(builder, vars, *vreg, val);
    }
}
```

### 3. Tests

**`test_q32_fadd_builtin`** — basic Q32 addition:

```
func @add(v0:f32, v1:f32) -> f32 {
  v2:f32 = fadd v0, v1
  return v2
}
```

Call with Q32 1.0 (65536) and Q32 2.0 (131072). Verify result is Q32 3.0
(196608). This exercises the `__lp_lpir_fadd_q32` builtin through the JIT
symbol lookup.

**`test_q32_fmul_builtin`** — Q32 multiplication:

```
func @mul(v0:f32, v1:f32) -> f32 {
  v2:f32 = fmul v0, v1
  return v2
}
```

Call with Q32 2.0 and Q32 3.0, verify Q32 6.0 (393216).

**`test_q32_fdiv_builtin`** — Q32 division:

```
func @div(v0:f32, v1:f32) -> f32 {
  v2:f32 = fdiv v0, v1
  return v2
}
```

Call with Q32 6.0 and Q32 2.0, verify Q32 3.0 (196608).

**`test_q32_import_call_sin`** — calling a glsl builtin through import:

```
import @glsl::sin(f32) -> f32

func @apply_sin(v0:f32) -> f32 {
  v1:f32 = call @glsl::sin(v0)
  return v1
}
```

Call with Q32-encoded 0.0, verify result is Q32 ~0.0 (sin(0) = 0).
Call with Q32-encoded π/2, verify result is approximately Q32 1.0.
Use a tolerance since Q32 sin is approximate.

**`test_q32_combined`** — expression using multiple ops:

```
func @quadratic(v0:f32) -> f32 {
  v1:f32 = fmul v0, v0
  v2:f32 = fconst 2.0
  v3:f32 = fmul v2, v0
  v4:f32 = fadd v1, v3
  v5:f32 = fconst 1.0
  v6:f32 = fadd v4, v5
  return v6
}
```

Compute x^2 + 2x + 1 at x = Q32(3.0). Expected: 16.0 = Q32(1048576).
Verify with some tolerance for Q32 arithmetic.

### 4. Test helper

Add a helper for Q32 test values:

```rust
fn q32(f: f32) -> i32 {
    crate::q32::q32_encode(f)
}

fn q32_to_f64(v: i32) -> f64 {
    v as f64 / 65536.0
}

fn assert_q32_approx(actual: i32, expected_f64: f64, tolerance: f64) {
    let actual_f64 = q32_to_f64(actual);
    assert!(
        (actual_f64 - expected_f64).abs() < tolerance,
        "Q32 mismatch: got {actual_f64} (raw {actual}), expected {expected_f64}"
    );
}
```

## Validate

```
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift
```
