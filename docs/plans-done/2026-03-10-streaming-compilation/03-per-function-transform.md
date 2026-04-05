# Phase 3: Per-Function Transform Helper

## Scope

Extract a per-function transform method that can transform a single float-typed
CLIF `Function` into a Q32-typed `Function` using the existing `Transform` trait.
This is the building block for the streaming loop in Phase 4.

Currently, `apply_transform_impl` in `gl_module.rs` does the transform for ALL
functions in a batch. We need a standalone helper that transforms ONE function.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### 1. Add `transform_single_function` to `gl_module.rs`

This is a standalone function (not a method on GlModule) that transforms one
function. It needs:

- The float-typed `Function` (input, consumed or borrowed)
- The `Transform` implementation
- The Q32 module (for `declare_func_in_func` during transform)
- The func_id_map and old_func_id_map
- The new FuncId for this function in the Q32 module

```rust
/// Transform a single function from float types to the target representation.
///
/// Used by the streaming pipeline to transform one function at a time without
/// needing to store all functions' IR simultaneously.
pub fn transform_single_function<M: Module, T: Transform>(
    float_func: &Function,
    transform: &T,
    q32_module: &mut GlModule<M>,
    func_id_map: &HashMap<String, FuncId>,
    old_func_id_map: &HashMap<FuncId, String>,
    target_func_id: FuncId,
) -> Result<Function, GlslError> {
    use crate::backend::transform::pipeline::TransformContext;

    let mut transform_ctx = TransformContext {
        module: q32_module,
        func_id_map: func_id_map.clone(),
        old_func_id_map: old_func_id_map.clone(),
    };

    let mut transformed = transform.transform_function(float_func, &mut transform_ctx)?;

    // Set the function name to match the FuncId (required by define_function)
    use cranelift_codegen::ir::UserFuncName;
    transformed.name = UserFuncName::user(0, target_func_id.as_u32());

    Ok(transformed)
}
```

Place this as a free function in `gl_module.rs` (or a new file if preferred),
not as a method on `GlModule`, since it takes `GlModule` as a parameter via
`TransformContext`.

Note: The `func_id_map.clone()` and `old_func_id_map.clone()` happen because
`TransformContext` owns these maps. This is an existing cost from the current
transform infrastructure. A future optimization could change `TransformContext`
to borrow them, but that's out of scope.

### 2. Verify against existing transform

Add a test that:

1. Creates a float module with a simple function
2. Creates a Q32 module with the Q32 signature declared
3. Calls `transform_single_function`
4. Verifies the returned function has Q32-typed signature
5. Compares the result to what `apply_transform` would produce

```rust
#[test]
#[cfg(feature = "std")]
fn test_transform_single_function() {
    use crate::backend::transform::q32::{FixedPointFormat, Q32Transform};

    let target = Target::host_jit().unwrap();
    let transform = Q32Transform::new(FixedPointFormat::Fixed16x16);

    // Build a simple float function in a float module
    let mut float_module = GlModule::new_jit(target.clone()).unwrap();
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::F32));
    sig.returns.push(AbiParam::new(types::F32));

    // ... build function body, declare in float module ...

    // Create Q32 module with Q32 signature declared
    let mut q32_module = GlModule::new_jit(target).unwrap();
    let q32_sig = transform.transform_signature(&sig);
    let q32_func_id = q32_module
        .module_mut_internal()
        .declare_function("test", Linkage::Local, &q32_sig)
        .unwrap();

    // Build maps
    let mut func_id_map = HashMap::new();
    func_id_map.insert(String::from("test"), q32_func_id);
    let mut old_func_id_map = HashMap::new();
    old_func_id_map.insert(float_func_id, String::from("test"));

    // Transform
    let result = transform_single_function(
        &float_func,
        &transform,
        &mut q32_module,
        &func_id_map,
        &old_func_id_map,
        q32_func_id,
    ).unwrap();

    // Verify Q32 signature
    assert_eq!(result.signature, q32_sig);
}
```

Adapt the test to use actual buildable function IR — look at the existing tests
in `gl_module.rs` and `jit.rs` for patterns (e.g., `build_simple_function`
helper).

## Validate

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std -- test_transform_single
```

Ensure all existing tests still pass:

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std
```
