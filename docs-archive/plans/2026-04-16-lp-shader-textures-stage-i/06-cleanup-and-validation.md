# Phase 6 — Cleanup & validation

## Scope

Final cleanup, warnings, formatting, and full validation pass.

## Cleanup

- Grep the git diff for any TODO comments that should not persist beyond
  this plan. The `TODO(M2)` in `render_frame` is expected and should stay.
  The `TODO(phase-4)` placeholder should already be gone.
- Remove any debug prints, unused imports, dead code.
- Run `cargo fmt` on all modified crates.
- Run `cargo clippy` on `lp-shader` and `lps-shared`.

## Validate

```bash
cargo fmt --check
cargo clippy -p lps-shared -p lp-shader --features cranelift -- -D warnings
cargo test -p lps-shared
cargo test -p lp-shader --features cranelift
cargo check  # full default workspace
```

Fix all warnings, errors, and formatting issues.

## Plan cleanup

Add a summary of the completed work to
`docs/plans/2026-04-16-lp-shader-textures-stage-i/summary.md`.

Move the plan files to `docs/plans-done/2026-04-16-lp-shader-textures-stage-i/`.

## Commit

Commit with:

```
feat(lp-shader): add lp-shader crate and texture storage types

- Add TextureStorageFormat enum and TextureBuffer trait to lps-shared
- Create lp-shader crate with LpsEngine, LpsFragShader, LpsTextureBuf
- LpsEngine::compile_frag() centralizes the GLSL compilation pipeline
- LpsTextureBuf wraps LpvmBuffer for guest-addressable texture storage
- render_frame stub (per-pixel loop deferred to roadmap M2)
```
