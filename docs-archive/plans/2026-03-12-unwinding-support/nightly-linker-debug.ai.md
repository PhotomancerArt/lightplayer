# ESP32 Nightly + build-std Debug Findings

**Date:** 2026-03-12 (updated)

## Summary

Previous findings incorrectly attributed failures to "nightly linker errors." The actual root cause
is a **build-std feature unification bug** in cargo. Nightly + esp-hal builds **fine** without
build-std.

## Corrected Isolation Results

| Toolchain | panic   | build-std     | Result |
|-----------|---------|---------------|--------|
| stable    | abort   | N/A (ignored) | OK     |
| nightly   | abort   | disabled      | **OK** |
| nightly   | abort   | core, alloc   | FAIL (E0464 alloc conflict) |
| nightly   | unwind  | core, alloc   | FAIL (E0464 alloc conflict) |

**Finding:** Nightly itself is NOT the problem. `build-std` causes an alloc conflict via
feature unification. Without build-std, nightly compiles and links fw-esp32 successfully.

## Root Cause: build-std Feature Unification

This is a known cargo bug: [rust-lang/cargo#8222](https://github.com/rust-lang/cargo/issues/8222)
and [wg-cargo-std-aware#63](https://github.com/rust-lang/wg-cargo-std-aware/issues/63).

When `-Zbuild-std` rebuilds the standard library from source:

1. std depends on `hashbrown 0.16.1` with feature `rustc-dep-of-std`
   (confirmed: `~/.rustup/toolchains/nightly-*/lib/rustlib/src/rust/library/std/Cargo.toml`)
2. Our workspace also depends on `hashbrown 0.16.1` (features: `alloc`, `default-hasher`)
3. Cargo **unifies features** across all instances of the same crate version
4. `rustc-dep-of-std` gets enabled for our hashbrown, pulling in `rustc-std-workspace-alloc`
5. `rustc-std-workspace-alloc` re-exports `alloc` under the name `alloc`
6. hashbrown's `extern crate alloc` now has two candidates → **E0464**

The linker errors previously documented likely came from stale build artifacts or a different
build state where compilation partially succeeded.

## Compile Errors (actual)

```
error[E0464]: multiple candidates for `rmeta` dependency `alloc` found
 --> hashbrown-0.16.1/src/lib.rs:61:1
  |
61 | extern crate alloc;
  = note: candidate #1: liballoc-*.rmeta          (real alloc from build-std)
  = note: candidate #2: librustc_std_workspace_alloc-*.rmeta  (workspace shim)

error[E0282]: type annotations needed
    --> hashbrown-0.16.1/src/raw/mod.rs:1627:35
```

## Why build-std is Needed for Unwinding

The `unwinding` crate needs `panic=unwind` to generate landing pads (cleanup code that calls
destructors during stack unwinding). The target `riscv32imac-unknown-none-elf` defaults to
`panic=abort`. Setting `panic = "unwind"` in Cargo.toml requires core/alloc to be compiled
with the same strategy, which means rebuilding them via `-Zbuild-std`.

Without build-std, the pre-built core/alloc use `panic=abort`. Mixing strategies causes:
```
error: the linked panic runtime `panic_abort` is not compiled with this crate's panic strategy `unwind`
```

## Possible Fixes

### 1. Use a different hashbrown version than std (simplest)

Std uses hashbrown 0.16.1. If we downgrade to 0.15.x or use 0.14.x, cargo won't unify features
between our hashbrown and std's, avoiding the conflict.

Risk: API differences, need to update cranelift/regalloc2 forks.

### 2. Use `unwinding` with `panic=abort` (no build-std)

The `unwinding` crate can mechanically unwind the stack even with `panic=abort`:
- `.eh_frame` IS emitted by default as of Rust 1.92 (PR #143613)
- The unwinder can walk frames and find catch points
- BUT: landing pads (Drop impls) won't run — destructors are skipped

This means `catch_unwind` can catch OOM and return control, but intermediate resources leak.
Pair with an arena/bump allocator for shader compilation to handle cleanup.

### 3. Wait for cargo build-std fix

Track [cargo#8222](https://github.com/rust-lang/cargo/issues/8222). The cargo team is aware
that build-std feature unification is broken.

### 4. Patch hashbrown locally

Fork hashbrown, remove `rustc-dep-of-std` feature or rename the `alloc` dep to avoid the
name collision. Use `[patch.crates-io]` to override.

## Commands Used

```bash
# Works (nightly, no build-std)
cd lp-fw/fw-esp32
# Comment out build-std in .cargo/config.toml first
cargo +nightly build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6

# Fails (nightly, with build-std)
# Uncomment build-std in .cargo/config.toml
cargo +nightly build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6

# Works (stable, build-std ignored)
cargo +stable build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6
```
