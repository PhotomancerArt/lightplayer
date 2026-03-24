# Phase 1: Verify compiler compiles to wasm32-unknown-unknown

## Scope

Verify that `lp-glsl-frontend` and `lp-glsl-wasm` (and all transitive deps) compile to `wasm32-unknown-unknown`. This is a feasibility gate before creating the web-demo crate. Fix any issues found.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Ensure wasm32-unknown-unknown target is installed

```bash
rustup target add wasm32-unknown-unknown
```

### 2. Try compiling lp-glsl-frontend

```bash
cargo build -p lp-glsl-frontend --target wasm32-unknown-unknown
```

Expected: should work — it's `#![no_std]`, all deps are no_std-compatible.

If `wasm-encoder` causes issues (it's a dep of `lp-glsl-wasm`, not `lp-glsl-frontend`), this step should pass regardless.

### 3. Try compiling lp-glsl-wasm

```bash
cargo build -p lp-glsl-wasm --target wasm32-unknown-unknown
```

Potential issue: `wasm-encoder = "0.245"` without `default-features = false`. If it pulls in std features, add:

```toml
wasm-encoder = { version = "0.245", default-features = false }
```

### 4. Fix any issues

Common problems and fixes:

- **`wasm-encoder` std dependency**: Add `default-features = false` and enable only needed features.
- **`hashbrown` hashing**: Already uses `default-hasher` feature, should be fine.
- **`libm`**: Pure Rust math, no_std — should work.
- **`glsl` parser / `nom`**: Used with `default-features = false` — should work.

### 5. Verify both crates build cleanly

```bash
cargo build -p lp-glsl-frontend --target wasm32-unknown-unknown
cargo build -p lp-glsl-wasm --target wasm32-unknown-unknown
```

Both must succeed with no errors.

## Validate

```bash
rustup target add wasm32-unknown-unknown
cargo build -p lp-glsl-frontend --target wasm32-unknown-unknown
cargo build -p lp-glsl-wasm --target wasm32-unknown-unknown
cargo build  # host build still works
cargo test   # no regressions
```
