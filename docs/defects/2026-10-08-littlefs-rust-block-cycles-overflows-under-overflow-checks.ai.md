---
status: open
found: 2026-10-08      # test (storage testbed, building the F3 control candidate)
area: littlefs-rust 0.1.0 (crates.io; light-player/littlefs-rust) — `lfs_dir_alloc` / `lfs_alignup`
class: backend-contract-divergence
related:
  - 2026-10-08-littlefs-rust-relocation-cut-leaves-lpfs-unmountable
  - 2026-10-08-littlefs-rust-compacts-every-metadata-commit
  - lp2025/2026-10-08-1017-tree-store-device-round
---
# With `block_cycles` set, littlefs-rust panics on an erased block when overflow checks are on

**Symptom** — `cargo test -p lp-store-bench` (the dev profile, overflow
checks on) failed every F3 test that formats a partition at F3's default
`block_cycles = 100`:

```
panicked at littlefs-rust-core-0.1.0/src/util.rs:62:19:
attempt to add with overflow
   3: littlefs_rust_core::util::lfs_alignup
   4: littlefs_rust_core::dir::commit::lfs_dir_alloc
   5: littlefs_rust_core::fs::format::lfs_format_
```

The same tests pass in a release build, and every bench number taken with
`block_cycles` set (#1051's endurance runs, F3's sweeps) came from release
builds, so none of them hit it.

**Root cause** — `lfs_dir_alloc` reads a newly allocated block's revision
count and, with `block_cycles > 0`, aligns it up to `(block_cycles + 1) | 1`.
On an erased block the revision reads `0xffff_ffff`, and
`lfs_alignup(a, n)` is `lfs_aligndown(a + n - 1, n)`. C's `uint32_t`
arithmetic wraps there by definition (the result is 0); the port spells it
with a plain Rust `+`, which panics under overflow checks and wraps
without them. With `block_cycles = -1` (the firmware today) the align is
skipped, so the path is never reached.

**Fix** — none yet; it belongs in `light-player/littlefs-rust`
(`wrapping_add` / `wrapping_sub` in `lfs_alignup` and wherever else the C
relies on unsigned wrap — `lfs_dir_needsrelocation` already uses
`wrapping_add`). Product exposure is nil: the firmware does not set
`block_cycles`, and `release-esp32` inherits `release`, which builds with
overflow checks off, so a firmware that turned `block_cycles` on would wrap
as C does. A **debug or test build** of anything that sets it (lpfs host
tests, fw-emu debug runs, the bench under `cargo test`) cannot format an
erased partition.

**Regression coverage** —
`tools/lp-store-bench/src/candidates/littlefs_pattern_package.rs`
`littlefs_format_overflows_with_block_cycles_under_overflow_checks`
(debug builds only) pins the panic. F3's unit tests run at
`block_cycles = -1` under overflow checks and at the default only without
them (`cfg(not(debug_assertions))`). If the pin starts failing, the library
was fixed: drop it and the gates.

**Lesson** — a port that is exact under one build profile can still differ
under another: C's defined unsigned wrap becomes a Rust panic only where
overflow checks are on, so the release-only bench runs looked clean. A
setting that changes which library code runs (here `block_cycles`, like the
relocation defect beside this one) needs a debug-build test, not just a
release measurement.
