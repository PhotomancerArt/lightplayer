# Phase 2: Stop Cloning func_id_map Per Function

## Problem

`transform_single_function` clones `func_id_map` and `old_func_id_map` every
iteration because `TransformContext` owns them:

```rust
let mut transform_ctx = TransformContext {
    module: q32_module,
    func_id_map: func_id_map.clone(),      // ~2 KB per clone × 11 functions
    old_func_id_map: old_func_id_map.clone(),
};
```

This accounts for a significant portion of the 31,820 bytes attributed to
`glsl_jit_streaming` (the Vec::clone and String::clone sub-entries).

## Fix

Change `TransformContext` to borrow the maps instead of owning them:

File: `lp-glsl/lp-glsl-compiler/src/backend/transform/pipeline.rs`

```rust
// Before:
pub struct TransformContext<'a, M: Module> {
    pub module: &'a mut GlModule<M>,
    pub func_id_map: HashMap<String, FuncId>,
    pub old_func_id_map: HashMap<FuncId, String>,
}

// After:
pub struct TransformContext<'a, M: Module> {
    pub module: &'a mut GlModule<M>,
    pub func_id_map: &'a HashMap<String, FuncId>,
    pub old_func_id_map: &'a HashMap<FuncId, String>,
}
```

Then update all callers:

1. `transform_single_function` in `gl_module.rs` — pass `&func_id_map` instead
   of `func_id_map.clone()`

2. `apply_transform_impl` in `gl_module.rs` — pass `&func_id_map` instead of
   `func_id_map.clone()`

3. The Q32 transform's `transform_function` — it accesses `ctx.func_id_map`
   and `ctx.old_func_id_map`. Since these are now references, any code that
   calls `.get()` or iterates will work unchanged (HashMap methods work on
   `&HashMap`). If any code takes ownership (unlikely), it will need to clone
   at the point of use.

## Expected savings

~10-15 KB from eliminating per-function map clones (11 functions × ~1-2 KB each).

## Validate

```bash
cd lp-glsl/lp-glsl-compiler && cargo test --features std
```

Also run the Q32 transform tests specifically since we're changing TransformContext:

```bash
cd lp-glsl/lp-glsl-compiler && cargo test --features std -- q32
cd lp-glsl/lp-glsl-compiler && cargo test --features std -- transform
```
