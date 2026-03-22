# Naga WASM POC — implementation summary

## Delivered

- Crate `spikes/naga-wasm-poc/` as a workspace member.
- `compile(source, export_name, mode)` → WASM bytes; modes `Float` and `Q32`.
- Integration tests (`tests/smoke.rs`) run the module under wasmtime (`f32` and `i32` signatures).
- `no_std`: `#![cfg_attr(not(test), no_std)]` so normal `cargo check` validates the Naga `glsl-in` stack without `std`; tests build the library with `std`.

## Naga IR detail

Naga’s GLSL frontend lowers `in` parameters to `LocalVariable` plus `Store` from `FunctionArgument`; the add uses `Load` of those locals. The emitter builds a `BTreeMap` from `Store` pairs and lowers `Load(LocalVar)` to `local.get` of the corresponding parameter index.

## Dependency choice

`naga` **29.0.0** from crates.io (not a path to `oss/wgpu`) for portability.

## Validate

```bash
cargo test -p naga-wasm-poc
cargo check -p naga-wasm-poc
cargo clippy -p naga-wasm-poc
```
