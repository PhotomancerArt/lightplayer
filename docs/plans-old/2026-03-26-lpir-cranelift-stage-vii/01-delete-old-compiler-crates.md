# Phase 1: Delete old compiler crates

## Scope

Delete the four crates that form the old compiler chain and have no remaining
consumers: `lps-cranelift`, `lps-jit-util`, `esp32-glsl-jit`,
`lps-q32-metrics-app`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Delete directories

```bash
rm -rf lp-shader/lps-cranelift
rm -rf lp-shader/lps-jit-util
rm -rf lp-shader/esp32-glsl-jit
rm -rf lp-shader/lps-q32-metrics-app
```

### 2. Root `Cargo.toml`

Remove from `[workspace] members`:

- `"lp-shader/lps-cranelift"`
- `"lp-shader/lps-jit-util"`
- `"lp-shader/esp32-glsl-jit"`
- `"lp-shader/lps-q32-metrics-app"`

Remove from `[workspace] default-members`:

- `"lp-shader/lps-cranelift"`
- `"lp-shader/lps-jit-util"`

Remove any `[profile.*.package.esp32-glsl-jit]` sections.

Remove any comments that reference these crates in the members/default-members
sections (e.g. the cross-target exclusion comment for `esp32-glsl-jit`).

### 3. Check for stale `[patch]` or `[workspace.dependencies]`

Grep for `lps-cranelift`, `lps-jit-util`, `esp32-glsl-jit`,
`lps-q32-metrics-app` in the root `Cargo.toml` beyond members lists.

### 4. Verify workspace resolves

```bash
cargo metadata --format-version=1 > /dev/null
```

This will error if any remaining crate still has a path dependency on a
deleted crate. That is expected — `lps-builtins-gen-app` references
`lps-frontend` which references `lps-cranelift`'s types, but
`lps-frontend` is not deleted yet (Phase 2). The workspace should still
resolve because `lps-frontend` has no path dep on `lps-cranelift`.

## Validate

```bash
cargo metadata --format-version=1 > /dev/null
cargo check -p lp-server
cargo check -p lp-engine
cargo check -p lpvm-cranelift
```

The `lps-builtins-gen-app` and `lps-filetests` may have warnings or
won't be checked yet — that's fine, they are Phase 2 scope.
