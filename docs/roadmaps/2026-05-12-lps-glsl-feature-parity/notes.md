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
- parser support for declarations, assignment forms, control flow, calls, and many expression operators
- HIR with typed expressions, locals, uniforms, builtins, user calls, import calls, and partial aggregate support
- LPIR lowering for scalar/vector values
- `rv32lpn.q32` filetest target
- `just demo-esp32c6-host-lps-glsl`

The vertical slice is real:

- `examples/basic` runs through `lps-glsl`
- same shader source as the Naga demo path
- compile time improved from about `578ms` to about `165ms` on device for the `3922` byte shader
- firmware size dropped from `85.24%` to `59.22%`, with the caveat that this is not a fair size comparison until parity is much closer

## Current Limits

The initial control/function/struct slices are now real enough to compile the demos and a focused set of filetests, but the aggregate foundation is still incomplete.

The important remaining gaps are:

- no single place model for swizzles, fields, indexes, globals, uniforms, and writable call arguments
- array/struct support is still more lane-flattened and case-based than it should be for feature parity
- arrays of structs, structs containing arrays, multidimensional arrays, and dynamic aggregate indexing need a stronger layout/access model
- aggregate `out`/`inout` needs the same machinery as ordinary assignment instead of bespoke writeback paths
- globals, uniform aggregates, and mutable global lifecycle semantics need to be tied into the same layout model
- aggregate return support may require a focused ABI design if filetests demand it
- matrix layout should be represented by the same aggregate shape layer instead of ad hoc column/vector handling
- limited builtin coverage
- diagnostics have spans, but not yet the friendly line/indicator presentation everywhere

## Aggregate Foundation Notes

The current hard part is not parsing `struct` or `float a[4]`. It is preserving one coherent notion of data shape through semantic analysis and lowering.

Recommended model:

- derive an `lps-glsl` `TypeShape` / `LayoutView` from `LpsType`
- delegate byte layout to the existing `lps_shared::layout` and LPVM data/path helpers
- expose frontend-only value-shape facts, such as scalar lane order and matrix column shape, beside shared byte-layout facts
- represent access as `PlaceRoot + PlacePath`, not as separate enum variants for every combination
- let lowering choose lane-flat or slot-backed representation behind the place API
- treat pointer ABI changes as a stop-and-design boundary, because they can affect LPIR/backends and firmware size

Existing layout anchors:

- `lp-shader/lps-shared/src/layout.rs`: std430 `LpsType` size, alignment, and array stride
- `lp-shader/lpvm/src/lpvm_data_q32.rs`: byte-backed data with path access over `LpsType`
- `lp-shader/lps-frontend/src/lower_aggregate_layout.rs`: example frontend adaptor over shared layout logic
- `lp-shader/lps-frontend/src/naga_util.rs`: current aggregate layout metadata for the Naga path

## Naga Frontend Inspiration

The Naga-backed frontend has already solved several aggregate problems that should influence `lps-glsl`:

- `lower_ctx::AggregateSlot` and `AggregateInfo` are a good conceptual model: aggregate values have storage, layout metadata, and a type identity.
- `lower_call.rs` uses one pointer argument for aggregate `in`, `out`, and `inout` cases, with optional sret for aggregate returns. This is the likely ABI shape if `lps-glsl` needs true aggregate pointer behavior.
- `lower_array.rs` centralizes array element address calculation, including dynamic index clamping and row-major multidimensional flattening.
- `lower_aggregate_write.rs` has the right fast path: whole slot-backed aggregate copies should become `Memcpy` instead of scalar-by-scalar stores when possible.
- `lower_lvalue.rs` validates the need for a writable actual abstraction with optional post-call writeback for projected scalar/vector leaves.

The warning from the Naga path is that much of the complexity comes from peeling Naga expression trees after the fact. `lps-glsl` can do better by constructing a typed `PlaceRoot + PlacePath` during semantic analysis and carrying it into lowering directly.

Useful reference material:

- `docs-archive/roadmaps/2026-04-22-lp-shader-aggregates/`
- especially the pointer ABI foundation and struct lowering notes

The archived roadmap was written for the old Naga-backed frontend and broader LPIR/backends. It should inform the design, not dictate it.

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
- Arrays, structs, pointers, and data shape should be designed deliberately rather than accreted as late special cases.

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
- aggregate returns or aggregate `inout` require a new pointer ABI rather than frontend-only copy-in/copy-out
- firmware size regresses enough to threaten the original benefit
- a feature wants a large architecture split beyond the roadmap
