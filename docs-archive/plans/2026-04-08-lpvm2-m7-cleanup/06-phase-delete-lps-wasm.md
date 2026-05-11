# Phase 6: Delete Legacy lps-wasm Crate

## Scope

Delete the `lp-shader/legacy/lps-wasm/` crate containing the old WASM emitter.
This has been replaced by `lpvm-wasm` which implements the `LpvmEngine` traits.

## Verification Before Deletion

### Check for Consumers

```bash
# Check if anything still imports from lps-wasm
rg "lps_wasm" lp-shader/ --glob "*.rs"

# Check Cargo.toml dependencies
rg "lps-wasm" Cargo.toml lp-shader/*/Cargo.toml
```

### Expected State

- `lps-wasm` should have NO consumers outside `legacy/`
- `lpvm-wasm` should be the only WASM implementation in use

## Files to Delete

```
lp-shader/legacy/lps-wasm/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── emit/
        ├── builtin_wasm_import_types.rs
        ├── control.rs
        ├── func.rs
        ├── imports.rs
        ├── memory.rs
        ├── mod.rs
        ├── ops.rs
        └── q32.rs
```

## Validate

```bash
# After deletion
cargo check --workspace --lib

# Specifically test lpvm-wasm still works
cargo check -p lpvm-wasm --lib
cargo test -p lpvm-wasm --lib

# Filetests using wasm
cargo test -p lps-filetests -- --target wasm.q32
```

## Phase Notes

- `lps-wasm` was an early WASM emitter for filetests
- `lpvm-wasm` now implements `LpvmEngine`/`LpvmModule`/`LpvmInstance`
- `lpvm-wasm` uses `lps-builtins-wasm.wasm` for builtins (not `lps-builtins-wasm` crate)
