# Phase 7: Delete Legacy lps-builtins-wasm Crate

## Scope

Delete the `lp-shader/legacy/lps-builtins-wasm/` crate. This was an old approach
to building builtins as a separate WASM module. The current `lpvm-wasm` uses a
different approach for builtins (inline or separate mechanism).

## Verification Before Deletion

### Check for Consumers

```bash
# Check if anything still imports from lps-builtins-wasm
rg "lps-builtins-wasm|lps_builtins_wasm" lp-shader/ --glob "*.rs"

# Check Cargo.toml
rg "lps-builtins-wasm" Cargo.toml lp-shader/*/Cargo.toml
```

### Note on WASM Builtins

- `lpvm-wasm` may use `lps-builtins-wasm.wasm` file (the artifact)
- This is DIFFERENT from the `lps-builtins-wasm` crate (the build system for it)
- Keep the `.wasm` artifact if still used, delete the crate

### Verify WASM builtins still work

```bash
# Check that lpvm-wasm tests still pass
cargo test -p lpvm-wasm --lib
```

## Files to Delete

```
lp-shader/legacy/lps-builtins-wasm/
├── build.rs
├── Cargo.toml
└── src/
    └── lib.rs
```

## If .wasm Artifact is Still Needed

The `lps-builtins-wasm.wasm` file should be:
- Built by `lps-builtins-gen-app` (not this crate)
- Located in target directory or installed location
- Used by `lpvm-wasm` at runtime

Verify the artifact exists separately from this crate:
```bash
find target -name "lps-builtins-wasm.wasm" 2>/dev/null
```

## Validate

```bash
# After deletion
cargo check --workspace --lib

# WASM backend should still work
cargo test -p lpvm-wasm --lib

# Filetests using wasm builtins
cargo test -p lps-filetests -- wasm.q32
```

## Phase Notes

- This is the BUILD CRATE for builtins-wasm, not the .wasm file itself
- The new build system is in `lps-builtins-gen-app` (if still needed)
- `lpvm-wasm` loads the .wasm artifact, doesn't depend on this crate
