## Status (2026-03-12, updated)

- **Fix applied:** Removed hashbrown `alloc` feature (it pulled in `rustc-std-workspace-alloc`).
  Downgraded to hashbrown 0.15. build-std now works.
- **Unwinding wired up:** fw-esp32 uses nightly + panic=unwind + unwinding crate + lp-server
  oom-recovery. Build succeeds via `just build-fw-esp32`.

---

### Options for ESP32 Unwinding (ordered by effort)

#### 1. Downgrade hashbrown to avoid feature unification (low effort)

Std bundles hashbrown 0.16.1. If we use a different version (0.15.x or 0.14.x), cargo won't
unify features between std's hashbrown and ours. The `rustc-dep-of-std` feature stays confined
to std's copy.

Action: Downgrade workspace hashbrown to 0.15, update cranelift/regalloc2 forks to match.
Then: nightly + build-std + panic=unwind + unwinding crate.

Risk: hashbrown API changes between 0.15/0.16.

#### 2. Use `unwinding` with panic=abort + arena allocator (medium effort)

Skip build-std entirely. Use nightly only for `#[lang = eh_personality]` (unwinding crate's
`personality` feature). Keep panic=abort.

- `.eh_frame` IS emitted with panic=abort (Rust ≥1.92)
- `unwinding::panic::catch_unwind` can catch and return control
- Destructors won't run (no landing pads with panic=abort)
- Use arena/bump allocator for shader compilation; reset arena on OOM

Action: Add unwinding dep (features: unwinder, fde-static, personality, panic).
Add eh_frame linker script. Implement arena allocator for compilation scope.

Pro: No build-std needed. No alloc conflict.
Con: Arena allocator implementation; destructors don't run on unwind.

#### 3. Transactional allocator (medium-high effort)

No unwinding at all. Track allocations in a "transaction." On OOM, longjmp back to catch
point and bulk-free the transaction. See docs/reports/2026-03-12-oom-recovery/transactional-alloc.md.

Pro: No build-std, no nightly features needed. Works with stable+abort.
Con: Per-allocation overhead, longjmp safety concerns, design complexity.

#### 4. Patch hashbrown locally (low-medium effort)

Fork hashbrown 0.16.1, remove or rename the `rustc-dep-of-std` feature to break the
unification. Use `[patch.crates-io]` to override.

Action: Fork, modify Cargo.toml, add patch.

Risk: Maintenance burden of a hashbrown fork.

#### 5. Wait for cargo build-std fix (no effort, unknown timeline)

Track [cargo#8222](https://github.com/rust-lang/cargo/issues/8222). The cargo team knows
about the feature unification problem with build-std.

#### 6. Propagate Result instead of panicking (high effort)

Change Cranelift/glsl_jit_streaming to return Result instead of panicking on OOM.
Allocator returns null, callers propagate errors.

Pro: Works with panic-abort, no new dependencies.
Con: Large refactor across Cranelift and the GLSL pipeline.
