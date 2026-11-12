# Phase 5: Cleanup + Validation

## Goal
Final pass to ensure everything is consistent, documented, and the full
test suite still passes.

## Steps

### 5.1 Cargo check full workspace
```
cargo check
```
Ensure the new crate doesn't break anything else.

### 5.2 Run all lpfx tests
```
cargo test -p lpfx
```
All tests from phases 3 and 4 pass.

### 5.3 Verify no_std compliance
The crate must compile without std. Confirm:
- `#![no_std]` is present in `lib.rs`
- No imports from `std::` anywhere in the crate
- `extern crate alloc;` used for `String`, `Vec`, `BTreeMap`

### 5.4 Public API review
Ensure the public surface is intentional:
- `lpfx::FxModule` — entry point
- `lpfx::FxManifest`, `FxMeta`, `FxResolution` — manifest types
- `lpfx::FxInputDef`, `FxInputType`, `FxValue`, `FxPresentation`,
  `FxChoice` — input types
- `lpfx::FxError` — error type
- `lpfx::parse_manifest` — standalone parsing fn (useful for tooling)

Raw types (`RawManifest`, etc.) should be `pub(crate)`.

### 5.5 Run full workspace tests
```
cargo test
```
Nothing broken by the new crate.

## Validation
- `cargo check` clean
- `cargo test` clean
- `cargo test -p lpfx` — all tests pass
- No `std` references in lpfx source
