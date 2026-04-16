# Phase 3: Reduce Streaming Overhead

## Problem

`glsl_jit_streaming` has 26,313 bytes at peak across 296 allocs:

| Bytes | Allocs | Via |
|------:|-------:|-----|
| 11,431 | 122 | (direct) — temporaries, strings, per-function data |
| 8,428 | 67 | Vec\<T,A\>::clone — cloning vectors |
| 2,712 | 15 | RawVecInner::finish_grow — Vec growth |
| 2,456 | 2 | HashMap\<K,V\>::with_capacity — pre-allocated maps |
| 882 | 89 | String::clone — name duplication |
| 404 | 1 | RawTable::reserve_rehash — HashMap rehash |

The pre-allocation (Phase 2 of improvements2) and bookkeeping reduction
(Phase 4 of improvements2) already helped. Remaining targets:

## A. Vec\<T,A\>::clone — 8,428 bytes (67 allocs)

These are likely from:
- `func.signature.clone()` (Signature contains Vec\<AbiParam\>)
- `func_info.typed_function.parameters.clone()` (Vec\<Parameter\>)
- `func_info.typed_function.return_type.clone()` (if vector type)
- Internal clones during `compile_single_function_to_clif`

Some of these are deferred to after `define_function` by Phase 4D of
improvements2 (glsl_signatures/cranelift_signatures populated inside loop).
If we move signature data extraction to before define_function (Phase 2B
of this plan), the signature lives through define_function. But we can
avoid cloning the cranelift signature altogether — `define_function`
consumes the Function, whose signature we could extract before passing:

```rust
let sig = core::mem::take(&mut func.signature);  // move, not clone
// But func needs its signature for define_function...
```

Actually, `define_function` needs the function with its signature intact.
The clone is necessary. However, we could defer the clone: store `func_id`
and rebuild the signature from `typed_function` data after the loop.

**Approach**: After the compile-define loop, rebuild `cranelift_signatures`
from `glsl_signatures` + `SignatureBuilder`, rather than cloning from the
compiled function. This avoids storing Signature clones during peak.

```rust
// After the loop (after peak):
let cranelift_signatures: HashMap<String, Signature> = glsl_signatures.iter()
    .map(|(name, glsl_sig)| {
        let sig = SignatureBuilder::build_with_triple(
            &glsl_sig.return_type, &glsl_sig.parameters,
            pointer_type, triple, numeric_mode.scalar_type(),
        );
        (name.clone(), sig)
    })
    .collect();
```

This moves the Signature allocations out of the peak window.

## B. String::clone — 882 bytes (89 allocs)

Function names are duplicated into:
- `func_ids` HashMap key
- `jit_func_id_map` HashMap key
- `sorted_functions[i].name`
- `glsl_signatures` key + FunctionSignature.name
- `cranelift_signatures` key

That's 5–6 String copies per function. With 11 functions, that's
~60–66 String allocs for names. The remaining 23 allocs come from
other string operations.

**Approach**: `jit_func_id_map` duplicates `func_ids` (minus builtins).
Build `jit_func_id_map` by filtering `func_ids` after the loop instead
of maintaining it separately:

```rust
let jit_func_id_map: HashMap<String, FuncId> = func_ids.iter()
    .filter(|(name, _)| sorted_names.contains(name))
    .map(|(name, id)| (name.clone(), *id))
    .collect();
```

Or better: just pass `func_ids` to `build_jit_executable_streaming` and
filter there. Eliminates the duplicate map entirely.

## C. Direct allocations — 11,431 bytes (122 allocs)

These are harder to trace without more detailed profiling. Likely includes:
- `GlslCompiler::new()` — `FunctionBuilderContext` pre-allocates scratch
- `module.make_context()` — cranelift context for define_function
- Per-function temporaries during CLIF generation

**Approach**: Reuse `GlslCompiler` across iterations (it was moved inside
the loop in an earlier optimization, but with single-module streaming
there's no reason to recreate it):

```rust
let mut compiler = GlslCompiler::new();  // once, before the loop
for func_info in &sorted_functions {
    let func = compiler.compile_single_function_to_clif(...)?;
    ...
}
```

`FunctionBuilderContext` is designed to be reused — it keeps scratch
buffers that avoid reallocation on subsequent uses.

Similarly, reuse the cranelift `Context` across iterations:

```rust
let mut ctx = module.module_internal().make_context();
for func_info in &sorted_functions {
    let func = ...;
    ctx.func = func;
    module.module_mut_internal().define_function(func_info.func_id, &mut ctx)?;
    module.module_internal().clear_context(&mut ctx);
}
```

This avoids allocating a new Context (and its internal buffers) per function.

## Expected savings

- A: ~2–3 KB (Signature clones moved out of peak)
- B: ~0.5 KB (eliminate jit_func_id_map duplication)
- C: ~2–3 KB (reuse compiler + context across iterations)
- Total: ~3–5 KB

## Risk

Low. All changes are mechanical. Reusing GlslCompiler and Context is
the intended usage pattern for cranelift.
