### What was built

- Implemented LPIR lowering for supported GLSL `texelFetch(sampler2D, ivec2, 0)` with descriptor loads, optional coordinate clamps, byte addressing, `Load16U`/`Unorm16toF`, and vec4 channel fill plus filetests across mainline Q32 backends.

### Decisions for future reference

#### Safe texelFetch bounds by default

- **Decision:** Generate clamp-to-edge bounds guards for `texelFetch` by default, with an explicit unchecked compiler option.
- **Why:** Default behavior must avoid arbitrary shared-memory reads; unchecked mode exists for performance measurement.
- **Rejected alternatives:** Always unchecked (unsafe); runtime trap (not supported by current LPIR/runtime surface).
- **Revisit when:** A richer runtime validation/trap mechanism exists or measured clamp cost requires a different policy.
