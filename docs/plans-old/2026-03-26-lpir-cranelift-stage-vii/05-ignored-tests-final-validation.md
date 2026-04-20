# Phase 5: Ignored tests, final validation, summary, commit

## Scope

Re-check ignored tests, full validation sweep, grep for leftovers, write
`summary.md`, move plan, commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Re-check ignored test

In `lp-shader/lps-filetests/tests/lpfn_builtins_memory.rs`:

- Remove `#[ignore = "..."]` from
  `shader_lpfn_saturate_vec3_writes_scratch_then_reads_it`.
- Run the test:

```bash
cargo test -p lps-filetests --test lpfn_builtins_memory
```

- If it **passes**: leave it enabled.
- If it **fails** (WASM ABI mismatch): re-add `#[ignore]` with an updated
  comment pointing to the LPIR features roadmap (not Stage VII):

```rust
#[ignore = "WASM multi-return ABI mismatch for vec3: LPIR emits 3 i32 returns, builtin uses result-pointer. See docs/roadmaps/2026-03-25-lpir-features/"]
```

### 2. Grep for leftover references

```bash
rg 'lp.glsl.cranelift|lp.glsl.jit.util|esp32.glsl.jit|lp.glsl.q32.metrics|lp.glsl.frontend' \
    --type rust --type toml lp-shader/ lp-core/ lp-fw/ Cargo.toml justfile
```

Should return **zero** matches in code/config files (comments about history in
Rust doc-comments are acceptable if they provide useful context, but prefer to
remove).

### 3. Grep for debug / temporary code

```bash
git diff --name-only | xargs rg 'TODO|FIXME|dbg!|println!' || true
```

Remove any that were introduced during this plan.

### 4. Format

```bash
cargo +nightly fmt
```

### 5. Full validation matrix

```bash
# Host workspace
cargo check --workspace --exclude fw-esp32 --exclude fw-emu --exclude lps-builtins-emu-app --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app
cargo test -p lp-engine
cargo test -p lp-server
cargo test -p lpvm-cranelift
cargo test -p lps-filetests -- test_glsl
cargo clippy -p lp-engine -p lp-server -p lpvm-cranelift --all-features -- -D warnings

# Builtins generation still works
cargo run -p lps-builtins-gen-app

# Firmware builds
just build-fw-emu
just build-fw-esp32

# Firmware tests
cargo test -p fw-tests
```

### 6. Plan summary

Write `docs/plans/2026-03-26-lpvm-cranelift-stage-vii/summary.md`:

- What was deleted (crates, line counts)
- What was migrated (gen-app types, filetests parse call)
- Docs updated
- Any re-ignored tests and their follow-up location

### 7. Move plan to `plans-done`

```bash
mv docs/plans/2026-03-26-lpvm-cranelift-stage-vii docs/plans-done/
```

### 8. Commit

```
refactor(lps): delete old compiler chain (Stage VII)

- Delete lps-cranelift, lps-jit-util, lps-frontend, esp32-glsl-jit, lps-q32-metrics-app
- Inline FunctionSignature/Type into lps-builtins-gen-app; drop lps-frontend dep
- Replace CompilationPipeline::parse with TranslationUnit::parse in filetests
- Remove old-backend generation paths from builtins gen-app
- Clean up workspace Cargo.toml, justfile, scripts, Dockerfile, IDE config
- Update README, AGENTS.md, cursor rules
```

## Validate

All commands in section 5 pass; grep in section 2 returns zero matches.
