# Phase 5: Add Codegen Support

## Description

Create codegen module to generate calls to LP library functions. This handles mapping user-facing `lp_*` names to internal `BuiltinId` variants and flattening vector arguments.

## Implementation

### File: `frontend/codegen/lp_lib_fns.rs`

Create new module with:

1. **Function name to BuiltinId mapping**
   ```rust
   fn get_lp_lib_fn_id(name: &str, arg_count: usize) -> Option<BuiltinId>
   ```
   Maps `lpfx_hash(1 arg)` -> `BuiltinId::LpHash1`, etc.

2. **Main codegen function**
   ```rust
   pub fn emit_lp_lib_fn_call<M: Module>(
       ctx: &mut CodegenContext<'_, M>,
       name: &str,
       args: Vec<(Vec<Value>, Type)>,
   ) -> Result<(Vec<Value>, Type), GlslError>
   ```
   - Lookup `BuiltinId` by name and argument count
   - Flatten vector arguments to individual components
   - Get `FuncRef` from module using `get_builtin_func_ref()`
   - Generate function call instruction
   - Return result value(s)

### Vector Argument Flattening

For vector arguments:
- `vec2` -> extract 2 i32 values
- `vec3` -> extract 3 i32 values
- `ivec2` -> extract 2 i32 values
- `ivec3` -> extract 3 i32 values

Use existing vector extraction utilities from codegen context.

### Integration

Update `frontend/codegen/mod.rs` to include the new module.

## Success Criteria

- Module compiles
- `emit_lp_lib_fn_call()` generates correct function calls
- Vector arguments are properly flattened
- Function calls use correct `BuiltinId` variants
- Return values are correctly wrapped
- Code formatted with `cargo +nightly fmt`

## Notes

- Follow pattern from `frontend/codegen/builtins/mod.rs`
- Use `ctx.module.get_builtin_func_ref()` to get function references
- Handle both scalar and vector argument types
- Place helper functions at the bottom of the file
