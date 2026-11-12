# M4: Matrices, Textures, and Builtins

## Objective

Close the high-volume language surface that remains after control flow, functions, and aggregates: matrices, texture-like inputs, and the builtin library expected by filetests.

## Features

- matrix types and constructors
- matrix indexing and column/element access
- vector/matrix arithmetic covered by filetests
- matrix builtins used by the corpus
- texture uniforms and texture sampling operations used by LightPlayer
- builtin overload expansion for scalar, vector, and matrix cases
- conversions and numeric promotion rules needed by the above

## Implementation Notes

Represent matrices as aggregate values with a clear canonical layout, most likely column-major to match GLSL expectations. Avoid spreading matrix layout assumptions across semantic analysis and lowering.

Texture support should be product-driven. If filetests encode texture metadata through harness directives, consume that at the filetest/runtime boundary rather than adding GLSL preprocessor behavior.

Builtin handling should become table-driven enough that adding an overload does not require a large match expression in several files. This can stay simple: a compact registry of builtin names, signatures, and lowering strategy is enough.

## Filetest Gate

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise matrix builtins texture
```

Then run the combined success-path set:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl scalar vec operators control function lpfn array struct global const uniform matrix builtins texture
```

## Done

- matrix filetests pass for supported numeric types
- texture examples and texture filetests render/sample correctly
- builtin coverage is broad enough that remaining failures are isolated, named gaps
- firmware size and compile time are re-measured against the earlier experiment

