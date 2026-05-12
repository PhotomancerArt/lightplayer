# M2 Summary - Fast Example to LPIR

## Completed

- Added a real `lps-glsl` compile output containing `LpirModule` plus `LpsModuleSig`.
- Extended `CompileJob` with durable `Body` and `Lower` phases after M1 lex/index.
- Added a small function-body parser for straight-line `return expr;` functions.
- Added typed HIR for literals, names, constructors, scalar `mod`, unary minus, and scalar binary
  arithmetic.
- Added LPIR lowering for scalarized function parameters and returns.
- Added simple uniform metadata and VMContext `Load` lowering using std430 offsets.
- Lowered `mod(x, y)` inline as `x - y * floor(x / y)`.
- Routed `rv32lpn.q32` through the real `lps-glsl` output in the filetest compile seam.
- Added `lps-glsl/fast-render.glsl`, derived from `examples/fast/shader.glsl`, with a uniform-backed
  run assertion.

## Validation

Passed:

```bash
cargo test -p lps-glsl
cargo test -p lps-filetests targets
cargo check -p lps-filetests
cargo check -p lps-filetests-app
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/fast-render.glsl
cargo run -p lps-filetests-app -- test --target rv32n.q32,rv32lpn.q32 --concise lps-glsl/fast-render.glsl
```

Before the final filetest run, `scripts/build-builtins.sh` was needed because the local RV32 builtins
artifact was missing.

## Remaining M2 Edges

- The filetest fixture mirrors `examples/fast/shader.glsl`; the example file itself still has no
  filetest directives.
- Function bodies are still intentionally narrow: no locals, branches, loops, swizzles, structs,
  arrays, or user function calls yet.
- Host timing output is still deferred; the compiler phase boundaries are in place for adding it
  without reshaping the API.
