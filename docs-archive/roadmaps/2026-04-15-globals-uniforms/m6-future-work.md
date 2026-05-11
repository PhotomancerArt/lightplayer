# Milestone 6: Future Work — Inline Tight Loop Rendering

## Goal

Design document and jump-off point for a future roadmap: JIT-compile a
texture-type and shader-type specific render function that inlines the shader
`main` and writes directly into texture memory.

**This milestone is not implemented in this roadmap.** It captures the design
intent and prerequisites so a future roadmap can pick it up.

## Motivation

The current render hot path calls the shader function once per pixel via
`call_direct`. Each call has overhead: function prologue/epilogue, argument
packing, return value unpacking, Q32→u16 conversion. For a 64x64 texture
that's 4096 function calls per frame.

The ideal hot path is a single JIT-compiled function that:
1. Loops over all pixels internally.
2. Inlines the shader `main` body (no call overhead).
3. Writes output directly to the texture buffer (no return value marshaling).
4. Handles globals reset via inline memcpy (or register reload for small
   globals).

This is the kind of optimization only possible with a custom compiler — the
JIT can see the shader body, the texture format, and the loop structure
together.

## Design Sketch

### `out vecX fragColor` Pattern

Instead of `render` returning `vec4`, the shader writes to an `out` variable:

```glsl
out vec4 fragColor;

void main() {
    fragColor = vec4(uv, 0.0, 1.0);
}
```

The JIT-compiled tight loop function would:
1. Compute the texture buffer address for the current pixel.
2. Set up `fragCoord` and other built-ins.
3. Execute the shader body inline.
4. The `out fragColor` store writes directly to the texture buffer at the
   computed address (with format conversion baked in).

### Texture-Aware Codegen

The compiled function knows the texture format (e.g. RGBA16) at compile time.
The `out` store can be lowered to format-specific writes:
- RGBA16: Q32→u16 conversion + 8-byte store per pixel.
- RGBA8: Q32→u8 conversion + 4-byte store per pixel.
- Future: other formats without recompiling the shader, just the wrapper.

### Loop Structure

```
fn render_inlined(vmctx: *mut u8, texture: *mut u8, width: u32, height: u32, time: Q32) {
    // init globals (inline or call)
    // snapshot globals
    for y in 0..height {
        for x in 0..width {
            // reset globals (inline memcpy or register reload)
            // inline shader body:
            //   fragCoord = vec2(x, y) * Q32_SCALE
            //   ... shader ops ...
            //   store fragColor → texture[y * width + x] (format-specific)
        }
    }
}
```

### Prerequisites (built in this roadmap)

- VMContext layout with uniforms/globals/snapshot (M2).
- `__shader_init` function synthesis (M1).
- Globals reset mechanism (M2).
- `out` parameter support in the frontend/codegen.
- Texture format awareness in the engine/codegen interface.

### Open Questions for Future Roadmap

- How does the JIT receive the texture buffer pointer and dimensions? As
  additional arguments to the compiled function, or via the VMContext?
- How is the compiled tight loop function represented in LPIR? A synthetic
  function with loop ops, or a new codegen-level construct?
- Should this be a separate compilation pass (shader → tight loop) or
  integrated into the main compilation?
- How to handle shaders that don't use the `out fragColor` pattern?
- Register pressure: inlining the shader body into a loop may increase
  register pressure. How does the allocator handle this?

## Dependencies

- Globals/uniforms roadmap complete (M1-M4).
- `out` variable support in frontend/codegen (may be part of a broader
  "storage qualifier" effort).

## Scope Estimate

This would be a separate roadmap with estimated 2-4 milestones covering:
loop codegen, shader inlining, texture format support, and benchmarking.
