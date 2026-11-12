# Phase 2 — Cargo feature rename: `alloc-trace` → `profile`

Mechanical rename of the Cargo feature that turns on the alloc
syscall in guest code. **No code logic changes**, just the feature
name. Syscall constants (`SYSCALL_ALLOC_TRACE`, `ALLOC_TRACE_*`)
are unchanged — those are syscall-protocol identifiers, not
build flags.

This phase can run in parallel with phase 1 (skeleton); the file
sets are disjoint.

## Subagent assignment

`generalPurpose` subagent. Pure rename, very tight scope.

## Files to touch

```
lp-fw/fw-emu/Cargo.toml
lp-riscv/lp-riscv-emu-guest/Cargo.toml
lp-riscv/lp-riscv-emu-guest/src/allocator.rs
lp-riscv/lp-riscv-emu-guest/src/syscall.rs
lp-fw/fw-tests/tests/alloc_trace_emu.rs   # build invocation only
lp-cli/src/commands/mem_profile/handler.rs # build invocation only
```

The two trailing files only need their `--features alloc-trace`
strings updated to `--features profile`. They will still drive the
old `with_alloc_trace` API at this point — that gets rewritten in
phase 4 / 5. Phase 2 only touches the **feature name string**.

## Steps

### 1. Rename feature in `lp-riscv-emu-guest/Cargo.toml`

Find the `[features]` section. Rename the `alloc-trace = []`
entry to `profile = []`. Verify nothing else in this Cargo.toml
references the old name.

### 2. Rename feature in `fw-emu/Cargo.toml`

The feature there forwards to the guest crate. Rename:

```toml
alloc-trace = ["lp-riscv-emu-guest/alloc-trace"]
```

to:

```toml
profile = ["lp-riscv-emu-guest/profile"]
```

Verify nothing else in this Cargo.toml references the old name.

### 3. Update cfg gates

In `lp-riscv-emu-guest/src/allocator.rs` and
`lp-riscv-emu-guest/src/syscall.rs`, replace every
`cfg(feature = "alloc-trace")` with `cfg(feature = "profile")`.
Use Grep first to find them all:

```bash
rg 'feature = "alloc-trace"' lp-riscv lp-fw lp-cli
```

Apply the replacement everywhere it shows up under
`lp-riscv-emu-guest/src/`. Do not touch the trailing `lp-fw/fw-tests/`
and `lp-cli/` matches yet — those are command-line invocations,
handled in step 4.

### 4. Update `--features` invocations

In `lp-fw/fw-tests/tests/alloc_trace_emu.rs` and
`lp-cli/src/commands/mem_profile/handler.rs`, update the cargo
build commands to pass `--features profile` instead of
`--features alloc-trace`. Both files have a single match each;
verify with grep above.

## Validation

```bash
# Guest code still compiles with the renamed feature
cargo check -p lp-riscv-emu-guest --features profile \
  --target riscv32imac-unknown-none-elf

# Firmware emu build still works
cargo check -p fw-emu --features profile

# Workspace check (host)
cargo check --workspace

# The existing alloc-trace integration test still passes — it now
# builds fw-emu with --features profile but the rest of the pipeline
# is unchanged.
cargo test -p fw-tests --test alloc_trace_emu

# CLI command still works (it shells out to cargo with the new
# feature name)
cargo run -p lp-cli -- mem-profile examples/basic
```

## Out of scope for this phase

- Renaming the syscall constants (NOT happening, by design).
- Renaming the test file (phase 7).
- Renaming the CLI command (phase 5).
- Removing `mem-profile` and `heap-summary` (phase 6).
