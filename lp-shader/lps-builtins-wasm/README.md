# lps-builtins-wasm

`cdylib` target `wasm32-unknown-unknown` that exports all compiler builtins from `lps-builtins`.

## Memory

`build.rs` passes `--import-memory` to the linker when `TARGET=wasm32-unknown-unknown`: the module
does **not** define its own `memory`; the host must supply `env.memory` (verify with
`wasm-objdump -x` on `lps_builtins_wasm.wasm`).

## `std` on wasm32

This crate enables `lps-builtins`’s `std` feature so the `log`/alloc path links cleanly;
`wasm32-unknown-unknown` provides the global allocator required for `cdylib`. A future `no_std` +
custom `GlobalAlloc` build is possible if size matters.

## Build

From repo root:

```bash
./scripts/build-builtins.sh   # runs codegen + builds RISC-V emu + this wasm
```

Or manually:

```bash
cd /path/to/workspace
cargo run -p lps-builtins-gen-app
cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release
```

Output: `target/wasm32-unknown-unknown/release/lps_builtins_wasm.wasm`

## `builtin_refs.rs`

Auto-generated; do not edit. Same generator as `lps-builtins-emu-app/src/builtin_refs.rs`.
