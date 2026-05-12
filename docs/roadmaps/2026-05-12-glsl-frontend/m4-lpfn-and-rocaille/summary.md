# M4 Summary - Demo Frontend Switch

## What was built

- Added `lp_shader::ShaderFrontend` with `Naga` and `LpsGlsl` variants.
- Added `CompilePxDesc::with_frontend` so callers can choose the frontend at the high-level shader
  compile boundary.
- Made `lps-frontend` optional behind `lp-shader/naga-frontend`.
- Kept existing defaults on the Naga path:
  - `lp-shader` default features include `naga-frontend`
  - `lpc-engine` default features include `naga-frontend`
  - `lpa-server` default features include `naga-frontend`
  - `fw-esp32` default features include `naga-frontend`
- Added `lpc-engine/lps-glsl-default` so shader nodes can select the native frontend without model
  churn.
- Added `fw-esp32/server-lps-glsl`, a server build that selects `lps-glsl` and does not pull in
  Naga.
- Added `just demo-esp32c6-host-lps-glsl`, which builds firmware with `--no-default-features` and
  pushes `examples/basic2`.

## Code-Size Experiment Hook

The no-Naga firmware graph was checked with:

```bash
cargo tree -p fw-esp32 --no-default-features --features esp32c6,server-lps-glsl --target riscv32imac-unknown-none-elf | rg "naga|lps-frontend" || true
```

That produced no matches.

## Validation

Passed:

```bash
cargo check -p lp-shader --no-default-features
cargo check -p lpc-engine
cargo check -p lpc-engine --no-default-features --features lps-glsl-default
cargo check -p lpa-server --no-default-features --features lps-glsl-default
cargo check -p lpa-server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --no-default-features --features esp32c6,server-lps-glsl
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo test -p lps-glsl
cargo test -p lps-filetests targets
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/basic2-render.glsl lps-glsl/fast-render.glsl lps-glsl/m3-core.glsl
cargo test -p lp-shader --no-default-features compile_px_desc_lps_glsl_simple_shader
just --list | rg "demo-esp32c6-host-lps-glsl|demo-esp32c6-host"
```

Known unrelated/full-suite status:

```bash
cargo test -p lp-shader
```

This still has four failures in existing non-RGBA output tests where `compile_px` tries to synthesize
RGBA16 sample functions for R16/RGB16 render signatures. The new `lps-glsl` focused `lp-shader` test
passes.

## Deferred M4 Language Surface

- Rocaille is still a good near-term target, but this slice intentionally stopped before loops.
- Next useful compiler work is `for` loops, compound assignment, component assignment, and the
  remaining LPFN ABI for `lpfn_psrdnoise`.
