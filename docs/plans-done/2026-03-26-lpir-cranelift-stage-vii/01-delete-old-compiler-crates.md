# Phase 1: Delete old compiler crates

## Scope

Delete the four crates that form the old compiler chain and have no remaining
consumers: `lp-glsl-cranelift`, `lp-glsl-jit-util`, `esp32-glsl-jit`,
`lp-glsl-q32-metrics-app`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Delete directories

```bash
rm -rf lp-glsl/lp-glsl-cranelift
rm -rf lp-glsl/lp-glsl-jit-util
rm -rf lp-glsl/esp32-glsl-jit
rm -rf lp-glsl/lp-glsl-q32-metrics-app
```

### 2. Root `Cargo.toml`

Remove from `[workspace] members`:
- `"lp-glsl/lp-glsl-cranelift"`
- `"lp-glsl/lp-glsl-jit-util"`
- `"lp-glsl/esp32-glsl-jit"`
- `"lp-glsl/lp-glsl-q32-metrics-app"`

Remove from `[workspace] default-members`:
- `"lp-glsl/lp-glsl-cranelift"`
- `"lp-glsl/lp-glsl-jit-util"`

Remove any `[profile.*.package.esp32-glsl-jit]` sections.

Remove any comments that reference these crates in the members/default-members
sections (e.g. the cross-target exclusion comment for `esp32-glsl-jit`).

### 3. Check for stale `[patch]` or `[workspace.dependencies]`

Grep for `lp-glsl-cranelift`, `lp-glsl-jit-util`, `esp32-glsl-jit`,
`lp-glsl-q32-metrics-app` in the root `Cargo.toml` beyond members lists.

### 4. Verify workspace resolves

```bash
cargo metadata --format-version=1 > /dev/null
```

This will error if any remaining crate still has a path dependency on a
deleted crate. That is expected — `lp-glsl-builtins-gen-app` references
`lp-glsl-frontend` which references `lp-glsl-cranelift`'s types, but
`lp-glsl-frontend` is not deleted yet (Phase 2). The workspace should still
resolve because `lp-glsl-frontend` has no path dep on `lp-glsl-cranelift`.

## Validate

```bash
cargo metadata --format-version=1 > /dev/null
cargo check -p lp-server
cargo check -p lp-engine
cargo check -p lpir-cranelift
```

The `lp-glsl-builtins-gen-app` and `lp-glsl-filetests` may have warnings or
won't be checked yet — that's fine, they are Phase 2 scope.
