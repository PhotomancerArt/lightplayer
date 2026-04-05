# Phase 3: Workspace and build system cleanup

## Scope

Clean up all build system references to deleted crates: workspace `Cargo.toml`,
`justfile`, scripts, Docker, IDE config.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. `justfile`

**`rv32_packages` variable** (line ~6):

- Remove `esp32-glsl-jit` from the string.
- If `lps-builtins-emu-app` is the only remaining entry, simplify or
  remove the variable if it is no longer used by any recipe.

**`build-rv32-jit-test` recipe** (line ~137–139):

- Delete entirely — it builds `lps-builtins-emu-app` and `esp32-glsl-jit`.
  The `lps-builtins-emu-app` build is still needed but should be in its
  own recipe or folded into `build-rv32-builtins`.

**`build-rv32` recipe** (line ~131):

- Remove `build-rv32-jit-test` dependency. Keep `build-fw-esp32` and
  `build-rv32-emu-guest-test-app`.

**`build-rv32-release`** — same as `build-rv32`.

**`build-glsl` and `build-glsl-release`** (lines ~184–187):

- Remove `--package lps-cranelift --package lps-jit-util` from the
  `cargo build` commands.

**`clippy-glsl` and `clippy-glsl-fix`** (lines ~260–263):

- Remove `--package lps-cranelift --package lps-jit-util`.

**`clippy-rv32-jit`** (line ~212):

- Delete recipe entirely (builds `esp32-glsl-jit`).

**`test-glsl`** (line ~293):

- Remove `--package lps-cranelift --package lps-jit-util` from the
  `cargo test` command.

**Workspace clippy** (line ~206):

- Remove `--exclude esp32-glsl-jit` from `cargo clippy --workspace` — crate
  no longer exists. Also remove `--exclude lps-builtins-emu-app` only if
  that crate is now host-compatible (check; likely still rv32-only, so keep
  the exclude).

### 2. `scripts/lp-build.sh`

Check contents. If it only references old crates / is superseded by `justfile`,
delete it. If it has useful ESP32 flash commands, clean up references to
`esp32-glsl-jit`.

### 3. `Dockerfile.rv32-jit`

Delete:

```bash
rm Dockerfile.rv32-jit
```

### 4. `.idea/lp2025.iml`

Remove `<sourceFolder>` entries for deleted crate paths:

- `lps-cranelift`
- `lps-jit-util`
- `esp32-glsl-jit`
- `lps-q32-metrics-app`
- `lps-frontend`

### 5. Stale comments in source

Grep for remaining references to deleted crates in Rust source comments:

```bash
rg 'lps-cranelift|lps-jit-util|esp32-glsl-jit|lps-q32-metrics|lps-frontend' --type rust lp-shader/ lp-core/ lp-fw/
```

Update or remove stale comments. Key files from earlier analysis:

- `lpir-cranelift/src/builtins.rs` — comment mentioning "shared with lps-cranelift registry"
- `lpir-cranelift/src/object_module.rs` — comment "Same triple as lps-cranelift"
- `lps-exec/src/lib.rs` — mentions legacy lps-cranelift / lps-jit-util
- `lps-exec/src/executable.rs` — "copied from lps-cranelift"
- `lps-builtins/src/host/mod.rs`, `macros.rs` — JIT delegate comments
- `lps-builtins/build.rs` — old build.rs reference
- `lps-diagnostics/src/lib.rs` — "retain copies" comment
- `lpvm/src/lib.rs` — "copied from lps-cranelift"

## Validate

```bash
cargo metadata --format-version=1 > /dev/null
cargo check --workspace --exclude fw-esp32 --exclude fw-emu --exclude lps-builtins-emu-app --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app
just build-fw-emu
just build-fw-esp32
```
