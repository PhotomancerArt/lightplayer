# Phase 6: Cleanup + validation

## Scope

Final pass: workspace builds, `no_std` compliance, lints, warnings.

## Code organization reminders

- Remove any temporary code, TODOs, debug prints.
- Ensure public API is intentional -- internal helpers are `pub(crate)`.
- Keep related functionality grouped together.

## Steps

### 6.1 Workspace build

```bash
cargo check
cargo test
```

The new crates must not break anything in the existing workspace.

### 6.2 `no_std` compliance

For both `lpfx` and `lpfx-cpu`:
- `#![no_std]` is present in `lib.rs`
- No imports from `std::`
- `extern crate alloc;` for `String`, `Vec`, `BTreeMap`

### 6.3 Public API review

**`lpfx`:**
- `TextureId`, `TextureFormat`, `CpuTexture` -- texture types
- `FxEngine`, `FxInstance` -- traits
- `FxModule`, `FxManifest`, `FxMeta`, `FxResolution` -- manifest types
- `FxInputDef`, `FxInputType`, `FxValue`, `FxPresentation`, `FxChoice` -- input types
- `FxError` -- error type
- `parse_manifest` -- standalone parsing fn

**`lpfx-cpu`:**
- `CpuFxEngine` -- concrete engine
- `CpuFxInstance` -- concrete instance
- Backend state types are `pub(crate)`.

### 6.4 Clippy + warnings

```bash
cargo clippy -p lpfx -p lpfx-cpu
```

Fix any new warnings. Don't fix pre-existing workspace warnings.

### 6.5 Run all lpfx tests

```bash
cargo test -p lpfx -p lpfx-cpu
```

All tests pass.

## Plan cleanup

Add `summary.md` to this plan directory.
Move plan files to `docs/plans-done/`.

## Commit

```
feat(lpfx): add CPU rendering backend (M1)

- Add FxEngine/FxInstance traits, TextureId, TextureFormat, CpuTexture to lpfx
- Add lpfx-cpu crate with cranelift backend (feature-gated)
- GLSL -> LPIR -> LpvmModule compilation pipeline
- Input-to-uniform mapping with input_ prefix convention
- Q32 per-pixel render loop via DirectCall::call_i32_buf
- Update noise.fx uniforms to input_* prefix
- Integration test: noise.fx renders non-trivial pixels
```
