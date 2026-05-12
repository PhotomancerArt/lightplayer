# Phase 1: Shader Sample ABI

## Scope Of Phase

Add a shader-engine hot path for sampling arbitrary points without rendering a texture.

In scope:
- Add synthetic `__render_samples_rgba16(points_ptr, out_ptr, count) -> void`.
- Add LPVM validation and backend call APIs.
- Add `LpsPxShader` API for RGBA16 sample buffers.
- Add tests comparing sample output to texture-rendered pixel centers where practical.

Out of scope:
- Engine fixture integration.
- Final UI/debug probes.
- Supersampling or area filtering.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Add `lp-shader/lp-shader/src/synth/render_samples.rs` rather than expanding `render_texture.rs`.
- Keep tests at the bottom of files.
- Avoid `f32` in pixel/sample hot buffers; input points are Q16.16 and output samples are unorm16.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and deviations.

## Implementation Details

Relevant files:
- `lp-shader/lp-shader/src/synth/render_texture.rs`
- `lp-shader/lp-shader/src/synth/mod.rs`
- `lp-shader/lp-shader/src/engine.rs`
- `lp-shader/lp-shader/src/px_shader.rs`
- `lp-shader/lpvm/src/instance.rs`
- `lp-shader/lpvm/src/lib.rs`
- LPVM backend `instance.rs` files with `call_render_texture`

Expected changes:
- Add `render_samples_fn_name()` and `synthesise_render_samples_rgba16()`.
- Synthetic function parameters are pointer to packed Q16.16 points, pointer to packed RGBA16 output, and sample count.
- Preserve global-reset semantics per sample.
- Add `validate_render_samples_sig_ir`.
- Add `LpvmInstance::call_render_samples`.
- Add backend implementations mirroring `call_render_texture` resolution/caching.
- Add `PxShaderBackend::call_render_samples`.
- Add `LpsPxShader::sample_points_rgba16`.

## Validate

```bash
cargo fmt --check
cargo test -p lp-shader render_samples -- --nocapture
cargo test -p lpvm validate_render_samples -- --nocapture
cargo test -p lpvm-native render -- --nocapture
```

