# naga-wasm-poc

Spike crate: **GLSL → Naga IR → WASM** with optional **Q32** (`i32` fixed-point) emission, validated by **wasmtime** integration tests.

Not intended for production.

## Build / test

```bash
cargo test -p naga-wasm-poc
```

## `no_std`

The library builds with `#![cfg_attr(not(test), no_std)]` so `cargo check -p naga-wasm-poc` exercises a `no_std` dependency path (including Naga `glsl-in`). Integration tests compile the library with `std` for `wasmtime`.

## Naga version

Uses `naga` from crates.io (`glsl-in` feature). Optional: `[patch.crates-io]` or a path dependency to a local `wgpu` checkout for debugging.

## Scope

- One exported user function: `float add_floats(float a, float b) { return a + b; }` plus a dummy `void main() {}` for the GLSL frontend.
- Emitter supports `Load`/`LocalVariable` (Naga’s `in` param pattern), `FunctionArgument`, and `Binary` `+` only.
