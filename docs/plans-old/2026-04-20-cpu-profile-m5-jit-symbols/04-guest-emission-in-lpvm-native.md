# Phase 4: Guest emission in `lpvm-native`

> Read `00-notes.md` and `00-design.md` for shared context.
> Depends on phase 1 (`lp-perf::emit_jit_map_load`).

## Scope of phase

After a JIT module is compiled and linked into a `JitBuffer`, derive
per-function sizes from sorted offsets, materialize a
`Vec<JitSymbolEntry>` (with name strings stored in a sibling buffer
so guest pointers are stable for the duration of the call), and call
`lp_perf::emit_jit_map_load`.

### In scope

- One emission site in `lp-shader/lpvm-native/src/rt_jit/compiler.rs`
  (or `module.rs` if more natural — pick the spot that already has
  both the `JitBuffer` base address and the `BTreeMap<String, usize>`).
- A small private helper in the same module that takes
  `(buffer_base: u32, buffer_len: u32, entry_offsets: &BTreeMap<String,
  usize>)` and:
  - sorts offsets,
  - computes sizes as `next_offset - this_offset` (last function:
    `buffer_len - last_offset`),
  - writes name bytes contiguously into a single `Vec<u8>`,
  - writes `JitSymbolEntry` records pointing into that buffer,
  - calls `lp_perf::emit_jit_map_load(buffer_base, buffer_len, &entries)`.
- `lp-shader/lpvm-native/Cargo.toml`: add a `lp-perf = { path = "...",
  default-features = false }` dep if not already present. Re-export /
  pass through the appropriate feature so the call site picks the
  `syscall` sink on RV32 firmware and `noop` on host.
- A small unit test in `lpvm-native` for the size-derivation helper:
  given a synthetic `BTreeMap<String, usize>` and `buffer_len`, assert
  the produced entries match expected offsets and sizes.

### Out of scope

- Wiring `SYSCALL_JIT_MAP_UNLOAD` (deferred).
- Symbolizer changes (phases 5, 6).
- Anything in `lpvm-cranelift` — only `lpvm-native` is in scope for m5.
- Adding new features to crates other than `lpvm-native` itself
  (existing `lp-perf` features are reused).

## Code Organization Reminders

- Keep the size-derivation helper as a free function next to the call
  site, with its `#[cfg(test)]` test below it.
- Don't sprinkle JIT-symbol logic across the rt_jit submodules — keep
  it co-located.
- One concept per file; if the helper is bulky, a sibling
  `jit_symbol_emit.rs` is fine. Otherwise inline.
- Mark anything temporary with a `TODO`.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** add `#[cfg(feature = "std")]` around the JIT compilation
  path — see `.cursorrules` ("The compiler is the product").
- Do **not** suppress warnings.
- Do **not** weaken or skip existing tests.
- The emission must compile cleanly on **both** host and RV32. If you
  hit a `core::arch::asm!` issue on host, check that you're routing
  through `lp-perf` (which gates the `ecall` to `target_arch =
  "riscv32"` already) — do **not** add your own target gate.
- If anything is ambiguous, stop and report.
- Report back: files changed, validation output, any deviations.

## Implementation Details

### 1. Locate the call site

In `lp-shader/lpvm-native/src/rt_jit/compiler.rs`, find where
`compile_module_jit` (or the equivalent function that hands back the
finished `JitBuffer`) computes the final base address of the buffer
and has access to the `entry_offsets: BTreeMap<String, usize>`. That
is the emission point.

If the buffer base address isn't available there but is in
`module.rs::NativeJitModuleInner::new` (or wherever the module is
constructed), emit from there instead. The constraint is: emit
**exactly once**, **after** the buffer is at its final address, and
**before** any guest code starts running into it.

### 2. Helper

Sketch (adjust types to actual):

```rust
use ::alloc::vec::Vec;
use ::alloc::string::String;
use ::alloc::collections::BTreeMap;
use lp_perf::JitSymbolEntry;

/// Builds a `JitSymbolEntry` array (with names stored contiguously in
/// `name_buf`) and emits `SYSCALL_JIT_MAP_LOAD`.
fn emit_jit_symbols(buffer_base: u32, buffer_len: u32, entry_offsets: &BTreeMap<String, usize>) {
    if entry_offsets.is_empty() {
        return;
    }

    // Sort by offset to derive sizes via deltas.
    let mut sorted: Vec<(&str, u32)> = entry_offsets
        .iter()
        .map(|(name, off)| (name.as_str(), *off as u32))
        .collect();
    sorted.sort_by_key(|(_, off)| *off);

    // Pack names into one buffer; record (offset_in_buf, len) per name.
    let mut name_buf: Vec<u8> = Vec::new();
    let mut name_locs: Vec<(u32, u32)> = Vec::with_capacity(sorted.len());
    for (name, _) in &sorted {
        let off = name_buf.len() as u32;
        name_buf.extend_from_slice(name.as_bytes());
        name_locs.push((off, name.len() as u32));
    }
    let name_buf_base = name_buf.as_ptr() as u32;

    // Build entries with size = next_offset - this_offset (last: buffer_len - last_offset).
    let mut entries: Vec<JitSymbolEntry> = Vec::with_capacity(sorted.len());
    for (i, (_, off)) in sorted.iter().enumerate() {
        let next_off = sorted
            .get(i + 1)
            .map(|(_, n)| *n)
            .unwrap_or(buffer_len);
        let size = next_off.saturating_sub(*off);
        let (name_off, name_len) = name_locs[i];
        entries.push(JitSymbolEntry {
            offset: *off,
            size,
            name_ptr: name_buf_base.wrapping_add(name_off),
            name_len,
        });
    }

    lp_perf::emit_jit_map_load(buffer_base, buffer_len, &entries);

    // name_buf and entries drop here; this is fine because the host
    // syscall handler consumes everything synchronously inside ecall.
    let _ = name_buf;
}
```

(Adjust `BTreeMap<String, usize>` to whatever the actual entry-offsets
type is; m5 plan said `BTreeMap<String, usize>`. If the real type
differs, follow the real one.)

### 3. Call it

After buffer linking, before returning the module:

```rust
emit_jit_symbols(jit_buffer.base_addr() as u32, jit_buffer.len() as u32, &entry_offsets);
```

(`base_addr()` / `len()` may have different names — match the actual
`JitBuffer` API.)

### 4. Cargo plumbing

In `lp-shader/lpvm-native/Cargo.toml`, add (or extend) a `lp-perf`
dep:

```toml
[dependencies]
# ... existing ...
lp-perf = { path = "../../lp-base/lp-perf", default-features = false }
```

The sink is selected at the firmware level (already wired for the
existing `emit_begin!`/`emit_end!` macros). On host builds you'll get
the noop sink; on RV32 firmware with `feature = "syscall"` you'll get
the real ecall. If `lp-perf` features need explicit re-export through
`lpvm-native`, mirror whatever pattern the existing `lp-perf` users
follow (`lp-engine`, `fw-emu`, etc.) — check those crates first.

### 5. Test

Add a unit test for the size-derivation logic next to the helper:

```rust
#[cfg(test)]
mod jit_symbol_emit_tests {
    use super::*;

    #[test]
    fn sizes_are_offset_deltas() {
        let mut offsets = BTreeMap::new();
        offsets.insert("alpha".to_string(), 0);
        offsets.insert("beta".to_string(), 0x40);
        offsets.insert("gamma".to_string(), 0x60);
        // Drive only the size-derivation half of the helper. If your
        // refactor splits it cleanly, test the split function. If it
        // doesn't, factor out the (offset, size, name_len) computation
        // into a testable subroutine.
        // Expected: alpha=0x40, beta=0x20, gamma=0x40 (buf_len=0xa0).
        // ...
    }
}
```

If splitting the helper to make it testable is awkward, leave the
emission inline and write a more focused test on the pure
size-derivation function instead. Prefer the testable shape.

## Validate

```bash
# Host build - default features, noop sink for lp-perf
cargo test -p lpvm-native

# RV32 firmware build - the call site reaches the syscall sink
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# ESP32 firmware build - per .cursorrules requirement for shader pipeline changes
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

All three must succeed cleanly with no warnings.
