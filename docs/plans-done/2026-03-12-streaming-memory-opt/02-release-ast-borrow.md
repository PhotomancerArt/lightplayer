# Phase 2: Release AST Borrow at Peak

## Problem

At peak (during `define_function`), `T::clone_one` shows 12,082 bytes
(363 allocs) of AST data — `Expr::clone`, `Declaration::clone`,
`String::clone`. This is the `TypedShader`'s heap data, alive because
`sorted_functions` borrows `typed_ast` via:

```rust
struct StreamingFuncInfo<'a> {
    typed_function: &'a TypedFunction,  // borrows TypedShader
    ...
}
```

The loop also borrows `typed_ast.function_registry` and
`typed_ast.global_constants` directly.

## Current streaming loop structure

```
sorted_functions = [StreamingFuncInfo { typed_function: &typed_ast.*, ... }]

for func_info in &sorted_functions {          // typed_ast borrowed entire loop
    compile_single_function_to_clif(
        func_info.typed_function,             // borrows typed_ast
        &typed_ast.function_registry,         // borrows typed_ast
        &typed_ast.global_constants,          // borrows typed_ast
    );
    define_function();                        // PEAK — typed_ast alive
    glsl_signatures.insert(
        func_info.typed_function.return_type.clone(),
        func_info.typed_function.parameters.clone(),
    );
}
```

## Fix: Destructure and scope borrows

Two changes remove the cross-iteration borrow:

### A. Remove `typed_function` from StreamingFuncInfo

Replace the borrowed reference with a name-based lookup inside the loop:

```rust
struct StreamingFuncInfo {
    name: String,
    func_id: FuncId,
    ast_size: usize,
    // No lifetime — no borrow on TypedShader
}
```

Look up the function by name at the start of each iteration:

```rust
let typed_func = typed_ast.user_functions.iter()
    .find(|f| f.name == func_info.name)
    .or_else(|| typed_ast.main_function.as_ref()
        .filter(|_| func_info.name == MAIN_FUNCTION_NAME))
    .unwrap();
```

### B. Extract signature data before define_function

```rust
for func_info in &sorted_functions {
    let typed_func = lookup(&typed_ast, &func_info.name);

    let func = compile_single_function_to_clif(
        typed_func,
        &typed_ast.function_registry,
        &typed_ast.global_constants,
        ...
    )?;

    // Extract what we need BEFORE define_function
    let return_type = typed_func.return_type.clone();
    let parameters = typed_func.parameters.clone();
    let sig = func.signature.clone();
    // typed_func borrow ends here

    define_function(func)?;  // PEAK — typed_ast NOT actively borrowed
                             // but still a live local variable

    glsl_signatures.insert(func_info.name.clone(), FunctionSignature {
        name: func_info.name.clone(), return_type, parameters,
    });
    cranelift_signatures.insert(func_info.name.clone(), sig);
}
```

### C. Destructure TypedShader to enable partial drops

The borrows in step B release within each iteration, but `typed_ast`
is still a live local. Its heap data stays alive. To actually free it,
destructure it before the loop:

```rust
let TypedShader {
    main_function,
    user_functions,
    function_registry,
    global_constants,
} = typed_ast;
// typed_ast no longer exists — each field is an independent local
```

Now the loop borrows `user_functions`, `main_function`,
`function_registry`, and `global_constants` independently. These are
still all alive during the loop, but we can drop some between the
declaration phase and the compile-define phase:

After declarations are complete (all functions declared, sorted_functions
built), `function_registry` fields used only for declaration can
potentially be dropped. However, `compile_single_function_to_clif` takes
`&function_registry` and `&global_constants`, so those must survive the
loop.

The real win from destructuring: after the LAST iteration, each field is
dropped independently. And if we find that some TypedShader fields
(e.g., source text metadata) aren't needed for the loop at all, they
can be dropped before the loop starts.

### Expected effect

Steps A+B alone don't reduce peak memory (typed_ast is still alive as a
local). Step C enables the allocator to see the fields as independent
allocations but doesn't automatically free them during the loop. The
savings come from:

- **Removing the lifetime from StreamingFuncInfo**: This reduces the
  size of sorted_functions entries and eliminates pointer indirection.
- **Enabling future partial drops**: If we later identify fields of
  TypedShader that aren't needed during the compile-define loop, the
  destructuring makes them independently droppable.
- **Compiler optimizations**: Without the cross-iteration borrow, the
  compiler has more freedom to reorder drops.

Realistically, this phase saves ~2–5 KB (smaller StreamingFuncInfo,
less borrow pressure) rather than the full 12 KB. The remaining AST
data stays alive because `user_functions`, `function_registry`, and
`global_constants` are all needed every iteration.

## Risk

Low. The lookup-by-name is O(n) per function but n is small (11).
No behavioral change.
