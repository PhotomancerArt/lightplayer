# Phase 6: Integrate into Function Call Routing

## Description

Update the function call routing in `frontend/codegen/expr/function.rs` to check for LP library functions after GLSL builtins but before user-defined functions.

## Implementation

### File: `frontend/codegen/expr/function.rs`

Update `emit_function_call()` function to add LP library function check:

```rust
// Check if it's a built-in function
if crate::frontend::semantic::builtins::is_builtin_function(func_name) {
    return emit_builtin_call_expr(ctx, func_name, args, span.clone());
}

// Check if it's an LP library function
if crate::frontend::semantic::lp_lib_fns::is_lp_lib_fn(func_name) {
    return emit_lp_lib_fn_call_expr(ctx, func_name, args, span.clone());
}

// User-defined function
emit_user_function_call(ctx, func_name, args, span.clone())
```

### Add Helper Function

Create `emit_lp_lib_fn_call_expr()` similar to `emit_builtin_call_expr()`:
- Translate arguments
- Validate types using `check_lp_lib_fn_call()`
- Call `emit_lp_lib_fn_call()` from codegen module
- Handle errors with span information

## Success Criteria

- Function call routing checks LP library functions in correct order
- LP library function calls are properly routed to codegen
- Type checking works correctly
- Error messages include proper source locations
- Code formatted with `cargo +nightly fmt`

## Notes

- Maintain the order: constructors -> GLSL builtins -> LP library functions -> user functions
- Follow error handling pattern from `emit_builtin_call_expr()`
- Ensure span information is preserved for error reporting
