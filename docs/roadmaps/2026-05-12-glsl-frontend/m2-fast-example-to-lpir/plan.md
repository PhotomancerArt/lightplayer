# M2 Plan - Fast Example to LPIR

## Goal

Compile the existing `examples/fast/shader.glsl` through `lps-glsl` into validated LPIR and execute
the same shader shape through the `rv32lpn.q32` filetest target.

M2 is an end-to-end proof, not broad filetest compatibility. It should establish the semantic rails
that later milestones extend: typed HIR, source-spanned diagnostics, scalarized LPIR lowering,
uniform loads, and filetest target integration.

## Scope

In scope:

- Parse enough function bodies for straight-line `return expr;` functions.
- Build a small typed HIR for literals, names, constructors, builtin calls, binary arithmetic, and
  returns.
- Map GLSL scalar/vector types into `LpsType` metadata and scalar LPIR lanes.
- Resolve function parameters and simple top-level uniforms.
- Lower uniforms as VMContext `Load` operations using `LpsModuleSig` std430 layout.
- Lower `mod(x, y)` as `x - y * floor(x / y)` using LPIR arithmetic, avoiding an import for this
  first slice.
- Validate generated LPIR before returning from `compile`.
- Route `rv32lpn.q32` to the real `CompileOutput`.
- Add a filetest fixture derived from `examples/fast/shader.glsl`.

Out of scope:

- Branches, loops, local declarations, assignment, swizzles, arrays, structs, out/inout, and LPFN.
- Recovery after parse errors beyond halt-on-first-error.
- Full builtin registry design.
- Replacing the Naga target or changing default filetest targets.

## Architecture

M2 adds four frontend layers after the M1 lexer/index:

1. `body` parses body tokens into a compact expression/statement AST.
2. `hir` resolves names and attaches `LpsType` to expressions.
3. `lower` scalarizes HIR values into LPIR virtual registers.
4. `compile` packages `(LpirModule, LpsModuleSig)` and validates LPIR.

Values are represented as flattened lanes early. A `vec4` return is four `f32` LPIR return values,
while metadata still exposes the public GLSL signature as `LpsType::Vec4`.

## Resumability

`CompileJob` keeps phase boundaries explicit:

- `Lex`
- `Index`
- `Body`
- `Lower`
- `Done`

M2 can still execute a whole phase per `step`, but the state names are the durable scheduler surface.
Future milestones can split `Body` or `Lower` by function without changing the public API.

## Filetest Proof

Add `lp-shader/lps-filetests/filetests/lps-glsl/fast-render.glsl` with the fast shader body and:

```glsl
// set_uniform: time = 2.25
// run: render(vec2(0.0, 0.0)) ~= vec4(0.25, 0.0, 0.0, 1.0)
```

Run it with:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/fast-render.glsl
```

## Validation

Run:

```bash
cargo test -p lps-glsl
cargo test -p lps-filetests targets
cargo check -p lps-filetests
cargo check -p lps-filetests-app
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/fast-render.glsl
```

## Completion Criteria

- `lps_glsl::compile(examples/fast)` returns LPIR and metadata.
- Generated LPIR validates.
- `rv32lpn.q32` executes the new fast-render filetest.
- M1 indexing tests still pass for the current examples.
