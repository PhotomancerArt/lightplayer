## Structs, Arrays, and Textures

- **Idea:** Extend `lps-glsl` beyond current examples to cover structs, arrays, and texture
  access.
- **Why not now:** Current examples do not require them, and each feature adds layout and lvalue
  complexity.
- **Useful context:** Use `lp-shader/lps-filetests/filetests/struct/`, `array/`, and `texture/`
  when this becomes product-relevant.

## WGSL Frontend

- **Idea:** Add a WGSL parser that emits the same typed HIR and LPIR lowering path.
- **Why not now:** GLSL examples and runtime compatibility are the immediate product need.
- **Useful context:** Preserve the semantic boundary documented in `overview.md`; do not force GLSL
  parser abstractions to serve WGSL prematurely.

## Fine-Grained Compile Yielding

- **Idea:** Yield within large function bodies at statement, block, or expression granularity.
- **Why not now:** Coarse stage/function yielding is simpler and may be sufficient.
- **Useful context:** Revisit after Milestone 5 reports longest-step timing on real ESP32 hardware.

## Broader Filetest Compatibility

- **Idea:** Expand from example-shaped GLSL to broad filetest compatibility.
- **Why not now:** The product decision needs example hot-load evidence first.
- **Useful context:** Use feature-by-feature promotion from `lp-shader/lps-filetests/filetests/`.
