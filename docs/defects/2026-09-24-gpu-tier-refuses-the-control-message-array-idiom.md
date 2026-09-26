---
status: open
found: 2026-09-24      # how: ci — the first run of the example-shader compile gate
area: lp-gfx-wgpu (GPU tier assembly / naga glsl-in) × projects/test/{events,button}
class: backend-contract-divergence
related:
  - docs/debt/wgpu-refuses-scalar-uniform-arrays.md
  - docs/debt/example-shaders-not-compile-gated.md
  - docs/defects/2026-07-29-uniform-struct-array-runtime-index.md
  - lp-shader/lps-filetests/tests/example_shaders_compile.rs
---
# The GPU tier refuses the ControlMessage uniform-array idiom the CPU tiers accept

**Symptom** — the example-shader compile gate's first run
(`just test-example-shaders`), on `wgpu.f32`:

```
projects/test/events/shader.glsl does not compile on wgpu.f32:
    naga validation: error: Global variable [1] 'events' is invalid
      ┌─ glsl:7:44
    7 │ layout(binding = 1) uniform ControlMessage events[8];
      = Alignment requirements for address space Uniform are not met by [7]
      = The array stride 8 is not a multiple of the required alignment 16
```

and the same for `projects/test/button/shader.glsl`
(`uniform ControlMessage held[1];`). Every CPU target — `rv32n`, `rv32lpn`,
`rv32c`, `wasm`, `interp`, both Xtensa targets, Q32 and F32 — compiles both.

**Root cause** — `struct ControlMessage { uint id; uint seq; }` is 8 bytes
with a natural array stride of 8. naga's GLSL frontend gives a bare
(non-block) uniform array its natural stride; the uniform address space
needs 16, and naga's std140 rounding only runs for interface-block members.
This is exactly the gap `docs/debt/wgpu-refuses-scalar-uniform-arrays.md`
names ("scalar-only structs"); `catalog/patterns/meteor` passes only because
its `Meteor` struct happens to be 16-aligned. What is new is that the gap
covers shipped content: `projects/test/events/shader.glsl` is the reference
the debt register points authors at for uniform struct arrays, and it does
not compile on the GPU tier.

**Fix** — none yet. The paid-down shape is tier-side (the debt entry's
"pack at WGSL assembly time"), not a shader edit: `ControlMessage` is the
engine's control-message slot shape, and the CPU tiers bind it at its
natural layout, so padding the authored struct would move the CPU side too.

**Regression coverage** — the gate carries both pairs in its
`ALLOWED_FAILURES` (`lp-shader/lps-filetests/tests/example_shaders_compile.rs`),
citing this entry. An allowed entry that starts compiling fails the gate, so
the tier-side fix cannot land without removing them and closing this.

**Lesson** — the known debt was scoped as "scalar arrays, likely with
buffers", and nobody checked whether existing content already sat in it.
The filetest `wgpu.f32` target renders on an adapter per directive, so it
was never a cheap compile check; the GPU tier's own compile
(`compile_wgsl`) needs no adapter, and a compile gate over the content can
run it everywhere.
