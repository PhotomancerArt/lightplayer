# Phase 4: Reduce Streaming Bookkeeping

## Problem

`glsl_jit_streaming` has 31,448 bytes attributed to it at peak, vs 15,148 bytes
for `glsl_jit` in the batch path. After subtracting HashMap rehashes (phase 2),
there's still ~10 KB of overhead from:

- `(direct)`: 11,793 bytes / 225 allocs — per-function temporaries, String
  allocations for names, StreamingFuncInfo fields
- `Vec<T,A>::clone`: 8,428 bytes / 67 allocs — cloning of function parameters
  for FunctionSignature entries
- `String::clone`: 987 bytes / 99 allocs — function name duplication

## Fix

Several small wins:

### A. Don't store `float_sig` in StreamingFuncInfo

Currently each function's float signature is cloned into StreamingFuncInfo,
then used once during compile_single_function_to_clif. Instead, recompute it
from the TypedFunction at the point of use:

```rust
// Before (in declaration loop):
sorted_functions.push(StreamingFuncInfo {
    float_sig: float_sig.clone(), // Signature has Vecs inside
    ...
});

// After: don't store it, rebuild when needed
let float_sig = SignatureBuilder::build_with_triple(
    &typed_func.return_type, &typed_func.parameters, pointer_type, triple,
);
```

Signature contains `Vec<AbiParam>` for params and returns, so cloning it
allocates. Recomputing is cheap (just iterating type info).

### B. Reduce function name duplication

The function name is stored in:
- `func_id_map` (key)
- `old_func_id_map` (value)
- `float_func_ids` (key)
- `sorted_functions[i].name`
- `glsl_signatures` (key + FunctionSignature.name)
- `cranelift_signatures` (key)
- `jit_func_id_map` (key)

That's 7 String copies per function name. Some could use `&str` borrowed from
`sorted_names` (which borrows from `typed_ast`). If phase 3 removes the
TypedShader borrow, this gets harder. But even without phase 3, `sorted_names`
is a `Vec<&str>` that's alive for the whole function.

### C. Avoid building jit_func_id_map separately

The final `jit_func_id_map` duplicates data already in `func_id_map`. Could
reuse `func_id_map` directly (after removing builtin entries if needed), or
build it during the declaration loop instead of a separate pass.

### D. Defer glsl_signatures / cranelift_signatures to build_jit_executable

These maps are built during the streaming loop but only consumed by
`build_jit_executable_streaming`. If that function doesn't need them until
after all functions are defined, they could be built lazily or passed
differently to avoid peak overlap.

Actually, checking the trace: these maps ARE at peak because the loop hasn't
finished yet when peak occurs during define_function. Moving their population
to after the loop would mean they're empty at peak. But they need data from
each TypedFunction (return_type, parameters), which requires the AST to be
alive... unless we defer.

Simplest: populate them AFTER the loop in a separate pass over sorted_functions
(which still borrows TypedShader). They'd be empty during define_function calls.

## Expected savings

~3-5 KB in aggregate from these small wins.

## Risk

Low. All are straightforward refactors with no behavioral change.
