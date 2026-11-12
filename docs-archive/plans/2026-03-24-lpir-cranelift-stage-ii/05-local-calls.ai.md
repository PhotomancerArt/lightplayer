# Phase 5: Local Function Calls and Multi-Return

## Scope

Implement `Op::Call` for local function callees in `emit/call.rs`. Wire up
`FuncRef` creation in `jit_module.rs`. Add tests for local calls and
multi-return through calls.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. FuncRef setup in `jit_module.rs`

After creating the `FunctionBuilder`, declare all local functions as
`FuncRef`s in the current function:

```rust
let func_refs: Vec<FuncRef> = fn_ids.iter()
    .map(|fid| jit_module.declare_func_in_func(*fid, builder.func))
    .collect();
```

These are indexed by local function index (0-based). A `CalleeRef(n)` where
`n >= ir.imports.len()` maps to `func_refs[n - ir.imports.len()]`.

Since Stage II rejects imports (and `ir.imports` is empty), `CalleeRef(n)`
maps directly to `func_refs[n]`.

Pass `&func_refs` in `EmitCtx`.

### 2. `emit/call.rs` — Call handling

```rust
pub(crate) fn emit_call(
    op: &Op,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctx: &EmitCtx,
) -> Result<bool, CompileError>
```

#### Call

```rust
Op::Call { callee, args, results } => {
    let import_count = ctx.ir.imports.len() as u32;
    if callee.0 < import_count {
        return Err(CompileError::unsupported(
            "import calls not yet supported (Stage III)"
        ));
    }

    let local_idx = (callee.0 - import_count) as usize;
    let func_ref = ctx.func_refs[local_idx];

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

#### Return (already exists, move here from scalar.rs or wherever it landed)

```rust
Op::Return { values } => {
    let slice = func.pool_slice(*values);
    let mut vs = Vec::with_capacity(slice.len());
    for v in slice {
        vs.push(use_v(builder, vars, *v));
    }
    builder.ins().return_(&vs);
}
```

### 3. Tests

**`test_local_call`** — call a helper function:

```
func @double(v0:f32) -> f32 {
  v1:f32 = fadd v0, v0
  return v1
}

func @quad(v0:f32) -> f32 {
  v1:f32 = call @double(v0)
  v2:f32 = call @double(v1)
  return v2
}
```

JIT both functions. Call `quad(3.0)`, verify result is `12.0`.

Note: `jit_from_ir` declares all functions before defining any, so forward
references work. The test calls the second function by its `FuncId`.

**`test_multi_return_call`** — function returning multiple values:

```
func @swap(v0:f32, v1:f32) -> f32, f32 {
  return v1, v0
}

func @double_swap(v0:f32, v1:f32) -> f32, f32 {
  v2:f32, v3:f32 = call @swap(v0, v1)
  v4:f32, v5:f32 = call @swap(v2, v3)
  return v4, v5
}
```

Call `double_swap(1.0, 2.0)`, verify result is `(1.0, 2.0)` (swapped twice).

**`test_recursive_call`** — recursive factorial:

```
func @factorial(v0:i32) -> i32 {
  v1:i32 = iconst 1
  v2:i32 = ile_s v0, v1
  if v2
    return v1
  end
  v3:i32 = isub_imm v0, 1
  v4:i32 = call @factorial(v3)
  v5:i32 = imul v0, v4
  return v5
}
```

Call `factorial(5)`, verify result is `120`.

**`test_call_with_control_flow`** — call inside a loop:

```
func @add1(v0:i32) -> i32 {
  v1:i32 = iadd_imm v0, 1
  return v1
}

func @count_up(v0:i32, v1:i32) -> i32 {
  loop
    v2:i32 = ige_s v0, v1
    if v2
      break
    end
    v0 = call @add1(v0)
  end
  return v0
}
```

Call `count_up(0, 5)`, verify result is `5`.

### 4. Test helper for multi-return

For tests that call multi-return functions, use Cranelift's auto struct-return.
On x86-64 with SystemV, functions returning >2 values use a hidden first
pointer argument. We need a small test helper that allocates a buffer and calls
the function:

```rust
fn call_multi_return_f32_2(jit: &JITModule, fid: FuncId, a: f32, b: f32) -> (f32, f32) {
    let code_ptr = unsafe { jit.get_finalized_function(fid) };
    // Multi-return may use struct-return ABI depending on platform.
    // For 2x f32 on x86-64 SystemV, they fit in xmm0/xmm1 — no struct return.
    let f: extern "C" fn(f32, f32) -> (f32, f32) = unsafe { mem::transmute(code_ptr) };
    f(a, b)
}
```

For functions with more return values that do use struct-return, we may need
`lps-jit-util::call_structreturn`. Check whether our test cases stay
within register-return limits. 2x f32 should be fine on all platforms.

## Validate

```
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift
```
