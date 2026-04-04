# Phase II: Feature completeness — rainbow.glsl renders in web demo

## Goal

Expand the new WASM backend until `rainbow.glsl` renders correctly in the web
demo. This is the definition of "done" for the WASM path.

## Scope

In scope:
- Vectors (vec2/vec3/vec4): construction, scalarized operations, swizzles
- Standard GLSL builtins via `MathFunction` → inline WASM or BuiltinId import
- LPFX builtins via prototype injection + import dispatch
- Control flow: if/else, for loops
- User-defined functions (multiple functions, calls between them)
- `out` parameters
- Global constants
- Type conversions (float ↔ int, vec4 from vec3+float, etc.)
- Web demo integration (web-demo crate updated to use new stack)
- All existing wasm.q32 filetests passing

Out of scope:
- Cranelift backend changes
- lp-engine changes
- ESP32 integration

## Key decisions

- LPFX prototypes: forward declarations prepended to source, `#line 1` reset
- Builtin dispatch: `MathFunction` enum → match in emitter
- Vector scalarization: component-wise emission during IR walk

## Deliverables

- Full `lp-glsl-wasm` backend supporting rainbow.glsl feature set
- LPFX prototype injection in `lp-glsl-naga`
- Updated `web-demo` crate
- All wasm.q32 filetests passing, cross-validated against cranelift.q32

## Dependencies

- Phase I complete
