# Phase 3: Numeric-Aware Math Libcall Helpers

## Current state

`helpers.rs` has two methods:

- `get_math_libcall(func_name: &str)` → f32 → f32 TestCase call
- `get_math_libcall_2arg(func_name: &str)` → (f32, f32) → f32 TestCase call

Both always emit float signatures and TestCase external names. The Q32
transform later rewrites them.

## Proposed change

Add Q32-aware variants that emit builtin calls directly when in Q32 mode.

### Option A: Branch inside existing methods

```rust
pub fn get_math_libcall(&mut self, func_name: &str) -> Result<FuncRef, GlslError> {
    if self.is_q32() {
        return self.get_q32_math_builtin(func_name, 1);
    }
    // existing float implementation
}
```

### Option B: Separate methods, callers choose

Keep existing methods untouched; add new methods that callers use
in the Q32 branch. This is more explicit but requires changing every
call site.

**Recommendation**: Option A. It's the least invasive — call sites don't
change, the branching is centralized in helpers.rs.

## get_q32_math_builtin

New private method:

```rust
fn get_q32_math_builtin(
    &mut self,
    func_name: &str,
    arg_count: usize,
) -> Result<FuncRef, GlslError> {
    use crate::backend::builtins::map_testcase_to_builtin;

    let builtin_id = map_testcase_to_builtin(func_name, arg_count)
        .ok_or_else(|| GlslError::new(E0400, format!("No Q32 builtin for '{func_name}'")))?;

    self.gl_module.get_builtin_func_ref(builtin_id, self.builder.func)
}
```

This uses `map_testcase_to_builtin` from its new home in
`backend/builtins/` (moved in Phase 2) and the existing
`get_builtin_func_ref` on GlModule. No new infrastructure.

## Name mapping

The existing `map_testcase_to_builtin` in
`backend/transform/q32/converters/math.rs` already maps:

| Float name | Args | Q32 BuiltinId |
|-----------|------|---------------|
| sinf | 1 | LpQ32Sin |
| cosf | 1 | LpQ32Cos |
| tanf | 1 | LpQ32Tan |
| asinf | 1 | LpQ32Asin |
| acosf | 1 | LpQ32Acos |
| atanf | 1 | LpQ32Atan |
| atan2f | 2 | LpQ32Atan2 |
| sinhf | 1 | LpQ32Sinh |
| coshf | 1 | LpQ32Cosh |
| tanhf | 1 | LpQ32Tanh |
| asinhf | 1 | LpQ32Asinh |
| acoshf | 1 | LpQ32Acosh |
| atanhf | 1 | LpQ32Atanh |
| powf | 2 | LpQ32Pow |
| expf | 1 | LpQ32Exp |
| logf | 1 | LpQ32Log |
| exp2f | 1 | LpQ32Exp2 |
| log2f | 1 | LpQ32Log2 |
| sqrtf | 1 | LpQ32Sqrt |
| inversesqrtf | 1 | LpQ32Inversesqrt |
| roundf | 1 | LpQ32Round |
| roundevenf | 1 | LpQ32Roundeven |
| fmodf | 2 | LpQ32Mod |

This covers every `get_math_libcall` call site. No new mappings needed.

## Signature handling

`get_builtin_func_ref` uses `declare_func_in_func` which picks up the
function's declared signature (already i32-based for Q32 builtins).
The signature is automatically correct.

## Implementation notes

- `map_testcase_to_builtin` was moved to `backend/builtins/` in Phase 2
  so it survives transform removal in Plan E.
- The `get_q32_math_builtin` method needs `&mut self` because
  `get_builtin_func_ref` takes `&mut GlModule`. This matches the existing
  `get_math_libcall` signature.
