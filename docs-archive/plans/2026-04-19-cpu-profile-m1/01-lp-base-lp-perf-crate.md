# Phase 1 — `lp-base/` + `lp-perf` crate

Create the new workspace directory `lp-base/` and the `lp-perf` crate
inside it. This is the foundation everything else in m1 (engine
emission, lpvm-native emission, fw-emu wiring) depends on.

**No engine integration yet** — that's phases 3 & 5–6. This phase
produces a self-contained crate that compiles in all three feature
modes (default, `syscall`, `log`), with unit tests for the macros.

This phase can run in parallel with phases 2 and 4 (disjoint files).

## Subagent assignment

`generalPurpose` subagent. Tightly scoped: new crate from scratch
following the design doc verbatim.

## Files to create

```
lp-base/                                   # NEW DIRECTORY
├── README.md                              # NEW
└── lp-perf/                               # NEW CRATE
    ├── Cargo.toml                         # NEW
    ├── tests/
    │   └── macros.rs                      # NEW
    └── src/
        ├── lib.rs                         # NEW
        └── sinks/
            ├── mod.rs                     # NEW
            ├── noop.rs                    # NEW
            ├── syscall.rs                 # NEW
            └── log_sink.rs                # NEW

Cargo.toml (root)                          # UPDATE: members += "lp-base/lp-perf"
```

## Contents

### `lp-base/README.md`

Brief, sets the convention:

```markdown
# lp-base — foundational cross-cutting crates

Crates in this directory provide infrastructure used across multiple
domain groups (lp-core, lp-shader, lp-fw, lp-riscv). They are
intentionally **prefix-free** (`lp-perf`, not `lpb-perf`) — the
absence of a group prefix is the convention's signal that a crate is
not owned by any single domain.

Inhabitants:
- `lp-perf` — perf-event tracing macros (cfg-gated sinks).

Future inhabitants likely include `lpfs` (filesystem abstraction
extraction) and similar.
```

### `lp-base/lp-perf/Cargo.toml`

Per the design doc's "The `lp-base/lp-perf` crate → `Cargo.toml`"
section. Verbatim from there.

### `lp-base/lp-perf/src/lib.rs`

Per design doc. Verbatim.

### `lp-base/lp-perf/src/sinks/mod.rs`

Per design doc. Verbatim. Includes the `compile_error!` for
mutually-exclusive features.

### `lp-base/lp-perf/src/sinks/noop.rs`

Per design doc. Verbatim.

### `lp-base/lp-perf/src/sinks/syscall.rs`

Per design doc. Verbatim. Note: this file only compiles when
`feature = "syscall"` is enabled, AND it depends on
`lp-riscv-emu-shared` for `SYSCALL_PERF_EVENT`. The constant must
exist (phase 2) for the `feature = "syscall"` build to succeed.

If phase 2 hasn't landed yet when this file is written: still
write it; phase 1 only validates the *default* (noop) build path.
The `syscall` feature is exercised only by phase 7 (fw-emu) and
phase 8 (e2e test). Add a comment noting the cross-phase dep.

### `lp-base/lp-perf/src/sinks/log_sink.rs`

Per design doc. Verbatim.

### `lp-base/lp-perf/tests/macros.rs`

Two integration tests, one per non-syscall feature path. The syscall
path can't be tested in a host-target unit test (it'd issue ECALL on
x86); it's covered by phase 8's e2e test.

```rust
// Default (no features) — emit calls compile to noop.
#[test]
fn noop_macros_compile() {
    use lp_perf::{emit_begin, emit_end, emit_instant, EVENT_FRAME};
    emit_begin!(EVENT_FRAME);
    emit_end!(EVENT_FRAME);
    emit_instant!(EVENT_FRAME);
}
```

For the `log` feature, gate a second test behind a `#[cfg(feature
= "log")]` block in the same file:

```rust
#[cfg(feature = "log")]
#[test]
fn log_macros_emit_to_log() {
    // Use a simple log capture (manual or via env_logger::builder
    // .is_test(true)) and assert at least one trace line was
    // produced. Keep it lightweight — we're verifying the call path
    // resolves, not log-crate internals.
}
```

(Full impl can be deferred to phase 8 if simpler — Phase 1 mainly
needs the noop test to confirm the macro shape is right.)

### Root `Cargo.toml` update

Add `"lp-base/lp-perf"` to `[workspace] members`. Maintain alphabetical
ordering if the existing list is sorted.

## Validation

```bash
# Default features (noop sink)
cargo build -p lp-perf
cargo test  -p lp-perf

# Log feature
cargo build -p lp-perf --features log
cargo test  -p lp-perf --features log

# Syscall feature (only host-target build check; will fail to link
# without RV32 target since the asm is rv32-specific — acceptable.
# The intent is to verify the file at least compiles syntactically
# under the cfg gate via cargo check):
cargo check -p lp-perf --features syscall

# Mutual exclusion
cargo check -p lp-perf --features "syscall log" 2>&1 \
  | grep -q "lp-perf: enable at most one"
```

The last command should succeed (find the compile_error message).
If `lp-riscv-emu-shared` doesn't yet have `SYSCALL_PERF_EVENT`
(phase 2 not done), the `--features syscall` `cargo check` will
fail with an unresolved-import error. That's expected; note in the
PR/commit that phase 2 is required for syscall builds to compile.

## Out of scope for this phase

- Engine emission sites (phase 3).
- lpvm-native emission (phase 3).
- fw-emu / fw-esp32 wiring (phase 3).
- Anything in `lp-riscv-emu` (phases 4-5).
- Mode system (phase 6).
- CLI changes (phase 7).
