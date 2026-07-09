# wgpu-preview-poc

Spike for M3 of the GPU preview shader-abstraction roadmap
(`~/.photomancer/planning/lp2025/2026-07-08-gpu-preview-shader-abstraction/`).
**Not intended for production** — wgpu must not enter any production crate;
this crate exists to produce evidence for the M4 design decision.

## What it proves

```
authored GLSL (examples/*, verbatim)
  + M2 canonical lpfn GLSL prelude (lps-builtins/glsl/lpfn/**)
  + generated wrapper main()
  → naga glsl-in → naga validate → naga wgsl-out
  → wgpu fragment shader on a fullscreen triangle → rgba32float offscreen
  → readback → frame diff vs the authoritative Q32 wasm path
    (lp-shader LpsPxShader::render_frame on wasmtime)
```

naga `glsl-in` is driven **directly** (not through `lps-frontend`), so the
`lpfn_` prefix is not reserved and the canonical prelude functions resolve as
plain local GLSL functions.

## Corpus

Five generative example shaders, no `sampler2D` (see `src/corpus.rs` for
what each exercises): `examples/basic`, `examples/basic2`,
`examples/fyeah-sign/idle.glsl`, `examples/fyeah-sign/blast.glsl`,
`examples/rocaille`.

## Run

```bash
cargo test -p wgpu-preview-poc                      # skips GPU tests when no adapter
cargo run -p wgpu-preview-poc --release --bin m3_report
```

The report binary prints markdown divergence/timing tables and writes
reference/GPU/side-by-side PNGs to `target/wgpu-preview-poc/`.

## Findings

See `m3-report.md` in the planning directory (coverage gaps, divergence
table, pipeline timings, M4 recommendation).
