# Phase 3: User-defined function calls

## Scope

Implement `Statement::Call` for user-defined functions (intra-module calls)
and `Expression::CallResult` for reading their return values. This unblocks
the `function/` test directory.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Function index mapping

Currently, `emit_module` assigns each function a sequential WASM func index
(`func_i`). To emit `call $idx`, we need to map `Handle<Function>` to WASM
func index.

Build a `BTreeMap<Handle<Function>, u32>` before emitting any function bodies:

```rust
let func_index_map: BTreeMap<Handle<Function>, u32> = naga_module
    .functions
    .iter()
    .enumerate()
    .map(|(i, (handle, _))| (*handle, i as u32))
    .collect();
```

Pass this into `emit_block`/`emit_stmt`/`emit_expr` (or store in `EmitCtx`).

Note: when imports are added (Phase 4), import functions occupy indices
`0..N-1` and user functions shift to `N..`. The `func_index_map` must
account for this offset.

### 2. Create emit_call.rs

```rust
pub fn emit_user_call(
    module: &Module,
    func: &Function,
    target: Handle<Function>,
    arguments: &[Handle<Expression>],
    result: Option<Handle<Expression>>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
    func_index_map: &BTreeMap<Handle<Function>, u32>,
) -> Result<(), String> {
    // 1. Emit all arguments (each may be scalar or vector, pushing N values)
    for &arg_h in arguments {
        emit_expr(module, func, arg_h, wasm_fn, mode, alloc)?;
    }

    // 2. Emit call instruction
    let wasm_idx = *func_index_map.get(&target)
        .ok_or_else(|| String::from("call to unknown function"))?;
    wasm_fn.instruction(&Instruction::Call(wasm_idx));

    // 3. If result exists, store to call-result temp locals
    if let Some(result_h) = result {
        let result_dim = call_result_dim(module, target);
        if result_dim > 0 {
            let base = alloc.resolve_call_result(result_h)
                .ok_or_else(|| String::from("no temp for call result"))?;
            for i in (0..result_dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
        }
    }

    Ok(())
}
```

### 3. Expression::CallResult

When a later expression references `CallResult(func_handle)`, emit the
stored temp locals:

```rust
Expression::CallResult(func_handle) => {
    let dim = call_result_dim(module, *func_handle);
    let base = alloc.resolve_call_result(expr)
        .ok_or_else(|| String::from("unresolved CallResult"))?;
    for i in 0..dim {
        wasm_fn.instruction(&Instruction::LocalGet(base + i));
    }
    Ok(())
}
```

### 4. Call result temp allocation in locals.rs

In `LocalAlloc::new()`, scan the function body for `Statement::Call` with
`result: Some(h)`. For each, determine the return type dimension of the
called function and allocate that many temp locals.

```rust
let mut call_result_map: BTreeMap<Handle<Expression>, u32> = BTreeMap::new();

fn scan_calls(block: &Block, func: &Function, module: &Module,
              call_result_map: &mut BTreeMap<Handle<Expression>, u32>,
              next: &mut u32, extra: &mut Vec<ValType>, mode: FloatMode) {
    for stmt in block.iter() {
        if let Statement::Call { function, result: Some(result_h), .. } = stmt {
            let called = &module.functions[*function];
            let dim = called.result.as_ref()
                .map(|r| type_slot_count(module, r.ty))
                .unwrap_or(0);
            if dim > 0 {
                call_result_map.insert(*result_h, *next);
                let vt = result_valtype(module, called, mode);
                for _ in 0..dim {
                    extra.push(vt);
                }
                *next += dim;
            }
        }
        // Recurse into nested blocks
    }
}
```

Add `pub fn resolve_call_result(&self, expr: Handle<Expression>) -> Option<u32>`.

### 5. Statement::Call in emit_stmt

Add match arm:

```rust
Statement::Call { function, arguments, result } => {
    emit_call::emit_user_call(
        module, func, *function, arguments, *result,
        wasm_fn, mode, alloc, &ctx.func_index_map,
    )
}
```

### 6. Void function calls

If `result` is `None`, just emit args + call. No result storage.

If the called function returns void (no results in type signature), nothing
to store.

## Validate

```bash
scripts/glsl-filetests.sh --target wasm.q32 "function/"
scripts/glsl-filetests.sh --target wasm.q32 "scalar/"
scripts/glsl-filetests.sh --target wasm.q32 "vec/"
cargo check -p lps-wasm
```

The `function/call-simple.glsl`, `function/define-simple.glsl`,
`function/return-scalar.glsl`, `function/return-void.glsl` should pass.
Tests involving `out`/`inout` parameters may need additional handling.
