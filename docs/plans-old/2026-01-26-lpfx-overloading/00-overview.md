# Plan: Support for LPFX Function Overloading

## Overview

Add support for function overloading in LPFX functions, allowing multiple implementations with the same GLSL name but different parameter signatures. This enables porting lygia functions that use overloading (e.g., `lpfx_hsv2rgb(vec3)` and `lpfx_hsv2rgb(vec4)`).

## Phases

1. Update `find_lpfx_fn` to support overload resolution
2. Update codegen call sites to pass `arg_types`
3. Update codegen tool to generate multiple entries for overloads
4. Update `check_lpfx_fn_call` to use new `find_lpfx_fn`
5. Test and cleanup

## Success Criteria

- Multiple overloads of the same function name can be registered
- Overload resolution correctly selects implementation based on argument types
- `lpfx_hsv2rgb(vec3)` and `lpfx_hsv2rgb(vec4)` both work correctly
- Existing filetests pass
- Codegen tool validates distinct signatures for overloads
- All code compiles without warnings
- Code formatted with `cargo +nightly fmt`
