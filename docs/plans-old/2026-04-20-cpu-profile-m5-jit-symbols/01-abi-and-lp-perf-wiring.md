# Phase 1: ABI + lp-perf wiring

> Read `00-notes.md` and `00-design.md` for shared context.

## Scope of phase

Define the `JitSymbolEntry` ABI struct in `lp-riscv-emu-shared` and add a
parallel `emit_jit_map_load` API to `lp-perf` that mirrors the existing
`emit_begin!` / `emit_end!` sink dispatch.

After this phase the function exists and compiles in all three sink
configurations, but **has no callers yet**. Phase 4 wires the call site.
Phase 3 wires the host receiver.

### In scope

- `lp-riscv-emu-shared`: new `JitSymbolEntry` struct.
- `lp-perf`: new `emit_jit_map_load(base, len, &[JitSymbolEntry])` entry
  point and one impl per sink (`syscall`, `log`, `noop`).
- Comment cleanup on the two reserved syscall constants.

### Out of scope

- Any caller of `emit_jit_map_load` (phase 4).
- Any host-side handler for `SYSCALL_JIT_MAP_LOAD` (phase 3).
- Any `JitSymbols` overlay logic (phase 2).
- Re-running `cargo test` for unrelated crates.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place abstract things, entry points, and tests near the **top**.
- Helper utility functions go at the **bottom**.
- Mark any temporary code with a `TODO` comment.
- No `#[allow(...)]` to silence warnings; fix the cause.

## Sub-agent Reminders

- Do **not** commit. The plan commits as a single unit at the end.
- Do **not** expand scope. Stay strictly within "In scope" above.
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix them.
- Do **not** disable, skip, or weaken any existing tests.
- If something blocks completion (ambiguous design, unexpected dep
  conflict), stop and report rather than improvising.
- Report back: what changed, what was validated, any deviations.

## Implementation Details

### 1. Add `JitSymbolEntry` to `lp-riscv-emu-shared`

Create `lp-riscv/lp-riscv-emu-shared/src/jit_symbol_entry.rs`:

```rust
//! ABI struct shared between guest (`lp-perf::sinks::syscall`) and host
//! (`lp-riscv-emu` syscall handler) for `SYSCALL_JIT_MAP_LOAD`.
//!
//! Each entry describes one JIT-emitted function: its byte offset within
//! the module's code buffer, its size in bytes, and a guest pointer to a
//! UTF-8 name string.

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct JitSymbolEntry {
    /// Byte offset within the JIT module's code buffer.
    pub offset: u32,
    /// Function size in bytes (derived from sorted-offset deltas at emit time).
    pub size: u32,
    /// Guest pointer to the UTF-8 name string.
    pub name_ptr: u32,
    /// Length of the name in bytes.
    pub name_len: u32,
}
```

Update `lp-riscv/lp-riscv-emu-shared/src/lib.rs` to add
`pub mod jit_symbol_entry;` and re-export `pub use
jit_symbol_entry::JitSymbolEntry;`. Match the existing module
declaration / re-export style in that file.

Update `lp-riscv/lp-riscv-emu-shared/src/syscall.rs` — the doc comments
on `SYSCALL_JIT_MAP_LOAD` and `SYSCALL_JIT_MAP_UNLOAD` currently say
"Not yet implemented; reserving the number to avoid collision in m2-m4."
Update `SYSCALL_JIT_MAP_LOAD`'s comment to:

```
/// JIT-symbol overlay: notify host that a JIT module has been linked.
///
/// ABI: `a0 = base_addr (u32)`, `a1 = len (u32)`, `a2 = count (u32)`,
/// `a3 = entries_ptr (u32)`. The entries array is `count` records of
/// `JitSymbolEntry` (see [`crate::JitSymbolEntry`]).
pub const SYSCALL_JIT_MAP_LOAD: i32 = 11;
```

Leave `SYSCALL_JIT_MAP_UNLOAD`'s comment alone except changing the
"m2-m4" wording to "deferred — see m5 plan / future-work doc".

### 2. Add `emit_jit_map_load` to `lp-perf`

Update `lp-base/lp-perf/src/lib.rs`:

```rust
// Existing imports + emit_begin!/emit_end!/emit_instant! stay unchanged.

/// JIT symbol-map load notification.
///
/// On RV32 firmware with `feature = "syscall"` this triggers
/// `SYSCALL_JIT_MAP_LOAD`. On host builds (`feature = "log"` or default
/// noop), it logs or no-ops.
#[inline(always)]
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    sinks::emit_jit_map_load(base, len, entries);
}

#[cfg(feature = "syscall")]
pub use lp_riscv_emu_shared::JitSymbolEntry;

// When neither sink pulls in lp_riscv_emu_shared, we still need a
// JitSymbolEntry symbol so the public signature compiles. Define a
// local mirror behind the noop/log paths.
#[cfg(not(feature = "syscall"))]
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct JitSymbolEntry {
    pub offset: u32,
    pub size: u32,
    pub name_ptr: u32,
    pub name_len: u32,
}
```

(Both definitions are `#[repr(C)]`; only one is compiled per build.)

Update `lp-base/lp-perf/src/sinks/mod.rs` to also re-export
`emit_jit_map_load` from the selected sink. Mirror the existing
pattern:

```rust
#[cfg(all(feature = "syscall", not(feature = "log")))]
pub use syscall::{emit, emit_jit_map_load};

#[cfg(all(feature = "log", not(feature = "syscall")))]
pub use log_sink::{emit, emit_jit_map_load};

#[cfg(not(any(feature = "syscall", feature = "log")))]
pub use noop::{emit, emit_jit_map_load};
```

(Existing `compile_error!` stays.)

Update `lp-base/lp-perf/src/sinks/syscall.rs` — add this function next
to the existing `emit`:

```rust
use crate::JitSymbolEntry;

#[cfg(target_arch = "riscv32")]
#[inline(always)]
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    use lp_riscv_emu_shared::SYSCALL_JIT_MAP_LOAD;
    let count = entries.len() as i32;
    let entries_ptr = entries.as_ptr() as i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("x17") SYSCALL_JIT_MAP_LOAD,
            in("x10") base as i32,
            in("x11") len as i32,
            in("x12") count,
            in("x13") entries_ptr,
            options(nostack, preserves_flags),
        );
    }
}

#[cfg(not(target_arch = "riscv32"))]
#[inline(always)]
pub fn emit_jit_map_load(_base: u32, _len: u32, _entries: &[JitSymbolEntry]) {}
```

Update `lp-base/lp-perf/src/sinks/log_sink.rs` — add a sibling
`emit_jit_map_load` that logs at `debug!` level:

```rust
use crate::JitSymbolEntry;

#[inline(always)]
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    log::debug!(
        "lp-perf: jit_map_load base=0x{base:08x} len={len} count={}",
        entries.len()
    );
}
```

Update `lp-base/lp-perf/src/sinks/noop.rs`:

```rust
use crate::JitSymbolEntry;

#[inline(always)]
pub fn emit_jit_map_load(_base: u32, _len: u32, _entries: &[JitSymbolEntry]) {}
```

### 3. Tests

Add a small unit test at the bottom of
`lp-riscv/lp-riscv-emu-shared/src/jit_symbol_entry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn layout_is_four_u32s() {
        assert_eq!(size_of::<JitSymbolEntry>(), 16);
        assert_eq!(align_of::<JitSymbolEntry>(), 4);
    }
}
```

No tests needed for the lp-perf entry point (it's pure dispatch).

## Validate

Run all three from the workspace root:

```bash
# Default (noop) sink builds
cargo build -p lp-perf

# Syscall sink builds (host target — syscall body is a no-op off RV32)
cargo build -p lp-perf --features syscall

# Log sink builds
cargo build -p lp-perf --features log

# ABI struct test
cargo test -p lp-riscv-emu-shared

# Confirm we haven't broken the workspace host build
cargo build -p lp-server
```

All five must succeed cleanly with no warnings.
