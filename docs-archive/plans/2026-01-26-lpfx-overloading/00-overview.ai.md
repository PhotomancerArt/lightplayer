# Plan: Support for LPFX Function Overloading

## Overview

Add support for function overloading in LPFX functions, allowing multiple implementations with the same GLSL name but different parameter signatures. This enables porting lygia functions that use overloading (e.g., `lpfn_hsv2rgb(vec3)` and `lpfn_hsv2rgb(vec4)`).

## Phases

1. Update `find_lpfn_fn` to support overload resolution
2. Update codegen call sites to pass `arg_types`
3. Update codegen tool to generate multiple entries for overloads
4. Update `check_lpfn_fn_call` to use new `find_lpfn_fn`
5. Test and cleanup

## Success Criteria

- Multiple overloads of the same function name can be registered
- Overload resolution correctly selects implementation based on argument types
- `lpfn_hsv2rgb(vec3)` and `lpfn_hsv2rgb(vec4)` both work correctly
- Existing filetests pass
- Codegen tool validates distinct signatures for overloads
- All code compiles without warnings
- Code formatted with `cargo +nightly fmt`
