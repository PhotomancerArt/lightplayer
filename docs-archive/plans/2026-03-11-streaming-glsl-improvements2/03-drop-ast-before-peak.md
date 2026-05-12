# Phase 3: Drop AST Before Compilation Peak

## Problem

In the batch path, `TypedShader` (the parsed AST) is dropped before the
compilation loop — CLIF generation and compilation are separate phases.

In the streaming path, `sorted_functions` holds `&TypedFunction` references
into `TypedShader`, keeping the entire AST alive through `define_function`
(where peak happens). This manifests as 12,082 bytes of `T::clone_one`
(Expr::clone, Declaration::clone) at peak — these are clones made during
CLIF generation that reference AST types still alive.

The current structure:

```rust
struct StreamingFuncInfo<'a> {
    name: String,
    typed_function: &'a TypedFunction, // borrows TypedShader
    float_func_id: FuncId,
    q32_func_id: FuncId,
    float_sig: Signature,
    linkage: Linkage,
    ast_size: usize,
}

for func_info in &sorted_functions {  // TypedShader borrowed for entire loop
    compile_single_function_to_clif(func_info.typed_function, ...);
    transform(...);
    define_function(...);  // PEAK — TypedShader still alive
}
```

## Fix

Don't store `&TypedFunction` in `StreamingFuncInfo`. Instead, look up each
function by name at the start of each iteration:

```rust
struct StreamingFuncIds {
    name: String,
    float_func_id: FuncId,
    q32_func_id: FuncId,
    linkage: Linkage,
    ast_size: usize,
}

for func_ids in &sorted_func_ids {
    let typed_func = typed_ast.user_functions.iter()
        .find(|f| f.name == func_ids.name)
        .or_else(|| typed_ast.main_function.as_ref()
            .filter(|_| func_ids.name == MAIN_FUNCTION_NAME))
        .unwrap();

    let float_func = compiler.compile_single_function_to_clif(typed_func, ...);
    // typed_func borrow ends here

    let q32_func = transform(float_func, ...);
    define_function(q32_func, ...);
}
```

This alone doesn't help — `typed_ast` is still a local variable alive for
the entire function. But it enables the next step: splitting the loop.

After CLIF generation + transform for function N, if we can prove
`typed_ast` is no longer needed for this iteration, we can potentially
`drop(typed_ast)` after processing the LAST function's CLIF generation
but before its `define_function`.

In practice, the simplest version: extract CLIF generation into a first
pass and `define_function` into a second pass, with `drop(typed_ast)`
between them. This is a "semi-streaming" approach:

```rust
// Pass 1: Generate all CLIF (TypedShader alive)
let mut clif_functions: Vec<(StreamingFuncIds, Function)> = Vec::new();
for func_ids in &sorted_func_ids {
    let typed_func = lookup(&typed_ast, &func_ids.name);
    let float_func = compile_to_clif(typed_func, ...);
    let q32_func = transform(float_func, ...);
    clif_functions.push((func_ids, q32_func));
}
drop(typed_ast);
drop(float_module);

// Pass 2: Compile to machine code (one at a time)
for (func_ids, q32_func) in clif_functions.drain(..) {
    define_function(q32_func, ...);
}
```

The tradeoff: all Q32 CLIF IRs are alive at the pass 1→2 boundary, but
TypedShader and float_module are dropped. Whether this helps depends on
whether AST + float_module > sum of all CLIF IRs.

For this test workload, the AST is small. This may not help much here but
would matter more for larger shaders.

## Alternative: true streaming with owned data

Clone the minimal AST data needed per function into a self-contained struct,
so the loop doesn't borrow TypedShader at all. Then drop TypedShader before
the loop:

```rust
struct OwnedFuncInfo {
    name: String,
    typed_function: TypedFunction, // OWNED clone
    float_func_id: FuncId,
    q32_func_id: FuncId,
    linkage: Linkage,
}
```

This costs one clone of each TypedFunction but allows dropping the rest of
TypedShader (global_constants, function_registry, etc.) before the loop.
Whether it helps depends on how much of TypedShader is non-function data.

## Expected savings

Hard to estimate precisely. The T::clone_one 12 KB at peak may or may not
be caused by the AST being alive — those clones might be temporaries from
CLIF generation that haven't been freed yet. Investigation needed.

## Risk

Medium. The semi-streaming approach is safe but reduces the streaming
benefit. The owned-data approach adds cloning cost.
