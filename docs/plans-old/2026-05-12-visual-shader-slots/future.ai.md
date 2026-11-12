## Texture Inputs

- **Idea:** Allow visual shader consumed slots to bind texture resources/products
  so shaders can sample upstream texture data as ordinary declared inputs.
- **Why not now:** This plan focuses on scalar/value inputs, especially time.
  Texture inputs need careful resource/product semantics and shader ABI support.
- **Useful context:** The visual shader input work should leave consumed-slot
  resolution centralized in `ShaderNode`, which is the right place to add
  texture input materialization later.

