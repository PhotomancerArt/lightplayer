# lps-glsl Feature Parity Notes

Date: 2026-05-12

## Scope

This roadmap is for moving `lp-shader/lps-glsl` from the current vertical slice to practical feature parity with the existing shader filetests and example projects.

The target is still the on-device compiler path:

- `no_std + alloc`
- no Naga dependency in the `server-lps-glsl` firmware path
- no host precompile workaround
- `rv32lpn.q32` remains the short-term comparison target beside the existing Naga/native targets

The goal is not exact Naga internals. The goal is to accept and execute the language surface the project relies on: structs, arrays, nested control flow, functions, `inout`, matrix/vector math, and the shader builtins covered by filetests and examples.

Out of scope unless a filetest proves otherwise:

- GLSL preprocessor
- GPU-stage metadata that is irrelevant to LightPlayer runtime execution
- perfect diagnostic recovery
- WGSL implementation itself

## Current State

`lps-glsl` already has the important skeleton:

- lexer/token/source span plumbing
- resumable `LpCompileJob` shape
- top-level indexing for uniforms, consts, and functions
- body parser for declarations, simple assignment, `if`, and `return`
- HIR with typed expressions, locals, uniforms, builtins, user calls, and import calls
- LPIR lowering for scalar/vector values
- `rv32lpn.q32` filetest target
- `just demo-esp32c6-host-lps-glsl`

The vertical slice is real:

- `examples/basic` runs through `lps-glsl`
- same shader source as the Naga demo path
- compile time improved from about `578ms` to about `165ms` on device for the `3922` byte shader
- firmware size dropped from `85.24%` to `59.22%`, with the caveat that this is not a fair size comparison until parity is much closer

## Current Limits

The parser currently accepts only a narrow M3 statement set:

- declarations
- simple name assignment
- `if`
- `return`

HIR/lowering currently handles scalar and vector values, but not the full aggregate surface:

- no loops, `break`, or `continue`
- no compound assignment, increment/decrement, ternary, or full logical short-circuiting
- no general lvalue model for swizzles, members, array indexes, or nested paths
- no arrays, structs, matrices, or global mutable values
- limited overload and qualifier handling
- limited builtin coverage
- diagnostics have spans, but not yet the friendly line/indicator presentation everywhere

## Filetest Inventory

Approximate current filetest distribution:

- total: `756`
- `vec`: `193`
- `scalar`: `69`
- `control`: `69`
- `matrix`: `68`
- `builtins`: `67`
- `function`: `58`
- `texture`: `35`
- `array`: `30`
- `global`: `25`
- `const`: `25`
- `operators`: `22`
- `lpvm`: `22`
- `lpfn`: `14`
- `uniform`: `13`
- `struct`: `8`
- plus smaller debug, VM context, type error, and future-oriented groups

Current focused `lps-glsl` fixtures:

- `lps-glsl/basic-rainbow-surface.glsl`
- `lps-glsl/basic2-render.glsl`
- `lps-glsl/fast-render.glsl`
- `lps-glsl/m3-core.glsl`
- `lps-glsl/rainbow.glsl`

## User Notes

- Time estimates should be grounded in how bounded this work is. Avoid day/week language; plan by filetest categories and milestone gates.
- The expected working estimate is closer to a concentrated autonomous implementation pass, roughly in the user's 4-8 hour range.
- Keep files small and organized.
- Work mostly autonomously and stop when actual product or semantic questions come up.
- The filetests should become the gate before hardware testing.
- The implementation has a reference path: existing Naga frontend behavior plus existing filetests.

## Working Assumptions

These are the assumptions this roadmap uses. They are intended to be easy to correct before or during implementation.

1. Successful execution parity matters before exact diagnostic parity.
   Suggested answer: yes. Error tests should get useful source spans and clear messages, but exact Naga wording is not a blocker.

2. Texture filetests are in the parity set if they describe runtime LightPlayer behavior.
   Suggested answer: yes. Preprocessor and GPU-stage metadata stay out of scope, but texture sampling and texture-like inputs are part of examples/product behavior.

3. `type_errors` and other negative tests should be implemented after success-path language coverage.
   Suggested answer: yes. Keep halt-on-first-error acceptable; improve formatting as we touch parser/semantic paths.

4. WGSL support should shape the boundary but not slow down GLSL parity.
   Suggested answer: yes. Preserve a language-neutral HIR/semantic layer. Do not build a generic parser framework prematurely.

5. Filetest annotations are allowed only for intentionally out-of-scope language or harness features.
   Suggested answer: yes. Do not mark hard GLSL features as unsupported just to move the number.

## Stop Conditions

Autonomous work should stop and ask for direction when:

- a filetest depends on preprocessor behavior or GPU metadata that may be intentionally out of scope
- matching Naga would require changing LightPlayer runtime semantics
- an LPIR/runtime ABI change is needed, especially around textures, imports, or aggregate values
- firmware size regresses enough to threaten the original benefit
- a feature wants a large architecture split beyond the roadmap

