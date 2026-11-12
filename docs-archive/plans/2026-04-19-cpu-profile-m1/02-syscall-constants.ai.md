# Phase 2 — Syscall constants

Add `SYSCALL_PERF_EVENT` and reserve `SYSCALL_JIT_MAP_LOAD` /
`SYSCALL_JIT_MAP_UNLOAD` in `lp-riscv-emu-shared`. This is a single-
file change; phases 4 and 5 use these constants but neither file
depends on the other.

This phase can run in parallel with phases 1 and 4 (disjoint files).

## Subagent assignment

Trivial — do not delegate to a subagent. Make the edit directly.

## Files to update

```
lp-riscv/lp-riscv-emu-shared/src/syscall.rs   # UPDATE: + 3 const defs
```

## Edit

Append the three constants after the existing `SYSCALL_ALLOC_TRACE`
block (which ends at line 32 today, before the `ALLOC_TRACE_*` event-
type constants). Place them immediately after `SYSCALL_ALLOC_TRACE`
and before `ALLOC_TRACE_ALLOC` so syscall numbers are listed
contiguously:

```rust
/// Emit a perf event from guest to host.
/// ABI: a0=name_ptr, a1=name_len, a2=kind (0=Begin, 1=End, 2=Instant).
/// a3 reserved for a future `arg: u32` payload.
pub const SYSCALL_PERF_EVENT: i32 = 10;

/// Reserved for m5 JIT-symbol overlay (load).
/// Not yet implemented; reserving the number to avoid collision in m2-m4.
pub const SYSCALL_JIT_MAP_LOAD: i32 = 11;

/// Reserved for m5 JIT-symbol overlay (unload).
/// Not yet implemented; reserving the number to avoid collision in m2-m4.
pub const SYSCALL_JIT_MAP_UNLOAD: i32 = 12;
```

Type is `i32` to match the existing convention (every existing
`SYSCALL_*` constant in the file is `i32`).

## Validation

```bash
cargo build -p lp-riscv-emu-shared
cargo test  -p lp-riscv-emu-shared      # no tests in this crate today; should pass
cargo check --workspace                 # nothing else should break
```

No new tests required — bare constant addition.

## Out of scope for this phase

- Host syscall handler (phase 5).
- Guest-side helper functions (none for m1; m5 adds JIT helpers).
- `lp-perf`'s `sinks/syscall.rs` consumer (phase 1; depends on this
  but doesn't require this phase to be merged first — both can land
  independently and only the cross-build needs both).
