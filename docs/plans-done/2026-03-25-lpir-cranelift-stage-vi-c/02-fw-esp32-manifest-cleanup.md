# Phase 2: `fw-esp32` manifest cleanup

## Scope of phase

Remove **orphan** optional dependencies that pointed at the old compiler stack.
The live path is `lp-server` → `lp-engine` → `lpir-cranelift` (no direct
`fw-esp32` → compiler edge).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Edit `lp-fw/fw-esp32/Cargo.toml`

Delete the entire block (lines may shift):

```toml
# Dependencies for optional GLSL compilation features
lp-glsl-cranelift = { ... }
lp-glsl-jit-util = { ... }
lp-glsl-builtins = { ... }
cranelift-codegen = { ... }
cranelift-frontend = { ... }
cranelift-module = { ... }
cranelift-control = { ... }
target-lexicon = { ... }
```

Keep the comment above `lp-server` about **not** enabling optimizer/verifier on
`fw-esp32` if it still applies (it refers to transitive Cranelift cost via
`lp-engine` / `lp-server` features — still valid).

### 2. Verify nothing references removed crates

```bash
rg 'lp-glsl-cranelift|lp-glsl-jit-util|cranelift-codegen|cranelift-frontend|cranelift-module|cranelift-control|target-lexicon' lp-fw/fw-esp32
```

Expect **no** matches in `src/` or `Cargo.toml`.

### 3. Optional `lp-engine` line

`lp-engine` is listed as optional but **no `[features]` entry** enables it;
`lp-server` already depends on `lp-engine`. Removing the direct `lp-engine`
optional dependency is consistent with orphan cleanup **if** nothing in
`fw-esp32/src` references `lp_engine` behind a feature (verify with `rg`). If
you prefer minimal diffs, drop only the compiler block in this phase and handle
`lp-engine` in a follow-up.

## Validate

ESP32 firmware build (from `justfile`; uses `release-esp32` profile):

```bash
just install-rv32-target
just build-fw-esp32
```

Or:

```bash
cd lp-fw/fw-esp32 && cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6
```

Fix linker or `no_std` errors that appear only after manifest cleanup (unlikely
if deps were truly unused).
