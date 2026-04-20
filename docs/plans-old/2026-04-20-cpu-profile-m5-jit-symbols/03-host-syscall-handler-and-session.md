# Phase 3: Host syscall handler + ProfileSession integration

> Read `00-notes.md` and `00-design.md` for shared context.
> Depends on phase 1 (`JitSymbolEntry` ABI struct) and phase 2
> (`JitSymbols` overlay).

## Scope of phase

Wire the host syscall handler for `SYSCALL_JIT_MAP_LOAD`, give
`ProfileSession` ownership of a `JitSymbols` overlay, and persist
`dynamic_symbols` into `meta.json` at session finish. Stub
`SYSCALL_JIT_MAP_UNLOAD` as a logged-error.

### In scope

- `ProfileSession` gains a `jit_symbols: JitSymbols` field.
- New method `ProfileSession::on_jit_map_load(&mut self, base, entries:
  &[(offset, size, String)], cycle: u64)`.
- `ProfileSession::finish_with_symbolizer` re-reads `meta.json` from
  disk, splices in `dynamic_symbols`, writes it back. (Order: do this
  **before** the report.txt build, so the report's writer doesn't see a
  stale meta.)
- New branch in `run_loops.rs::handle_syscall` for `SYSCALL_JIT_MAP_LOAD`
  (real impl) + dispatcher helper that mirrors
  `dispatch_profile_alloc_syscall`.
- New branch for `SYSCALL_JIT_MAP_UNLOAD`: `log::error!(...)`, set
  `a0 = 0`, `Continue`.
- Unit test for the syscall handler: synthetic guest memory + syscall
  args → expected `JitSymbols` state after dispatch.
- Unit test for `meta.json` rewrite at finish.

### Out of scope

- Guest emission (phase 4).
- CLI / alloc-trace symbolizer changes (phases 5 and 6).
- Wiring `JitSymbols` into the panic backtrace path (deferred — see
  `docs/future-work/2026-04-20-jit-symbols-in-panic-backtrace.md`).

## Code Organization Reminders

- Keep the new helper functions next to the existing
  `dispatch_profile_alloc_syscall` / `handle_perf_event_syscall`
  patterns in `run_loops.rs`. Match their style.
- Tests at the **top** of any new test module.
- Helper utility functions (e.g. for reading the entries array out of
  guest memory) at the **bottom** of their file.
- Don't pull syscall-handling logic into `jit_symbols.rs`. Keep the
  overlay pure data; the host wiring lives in `mod.rs` /
  `run_loops.rs`.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** rewrite parts of `meta.json` you weren't told to rewrite.
  Specifically: read it back, splice in exactly one new top-level field
  (`dynamic_symbols`), serialize back. Don't reformat unrelated fields.
- Do **not** change the existing `meta.json` write at `ProfileSession::new`
  beyond what's described below.
- Do **not** suppress warnings.
- Do **not** weaken or skip existing tests in `profile/mod.rs` or
  `run_loops.rs`-adjacent test modules.
- If anything is ambiguous, stop and report.
- Report back: files changed, validation output, any deviations.

## Implementation Details

### 1. `ProfileSession` owns `JitSymbols`

In `lp-riscv/lp-riscv-emu/src/profile/mod.rs`:

- Add `jit_symbols: JitSymbols` to `ProfileSession`.
- Initialize it as `JitSymbols::new()` in `ProfileSession::new`.
- Add a public method:

  ```rust
  /// Called by the host syscall handler for `SYSCALL_JIT_MAP_LOAD`.
  /// `entries` is `(offset_within_module, size_bytes, name)`.
  pub fn on_jit_map_load(
      &mut self,
      base: u32,
      entries: &[(u32, u32, String)],
      cycle: u64,
  ) {
      self.jit_symbols.add_module(base, entries, cycle);
  }
  ```

- In `finish_with_symbolizer`, **before** the report.txt build, splice
  `dynamic_symbols` into `meta.json`:

  ```rust
  // At the top of finish_with_symbolizer, before the collectors loop.
  self.write_dynamic_symbols_to_meta()?;
  ```

  Helper at the bottom of `impl ProfileSession`:

  ```rust
  fn write_dynamic_symbols_to_meta(&self) -> std::io::Result<()> {
      let meta_path = self.trace_dir.join("meta.json");
      let bytes = std::fs::read(&meta_path)?;
      let mut value: serde_json::Value = serde_json::from_slice(&bytes)
          .map_err(|e| std::io::Error::new(
              std::io::ErrorKind::InvalidData,
              format!("meta.json parse: {e}"),
          ))?;
      if let serde_json::Value::Object(map) = &mut value {
          map.insert(
              "dynamic_symbols".to_string(),
              self.jit_symbols.to_json(),
          );
      }
      let file = std::fs::File::create(&meta_path)?;
      serde_json::to_writer_pretty(std::io::BufWriter::new(file), &value)?;
      Ok(())
  }
  ```

### 2. Host syscall handler

In `lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs`:

Add `SYSCALL_JIT_MAP_LOAD` and `SYSCALL_JIT_MAP_UNLOAD` to the
`use lp_riscv_emu_shared::{...}` block (alongside the existing
`SYSCALL_*` imports).

Add a dispatcher helper near `dispatch_profile_alloc_syscall`,
gated `#[cfg(feature = "std")]`:

```rust
/// Reads a `JitSymbolEntry` array from guest memory and forwards to
/// [`crate::profile::ProfileSession::on_jit_map_load`].
#[cfg(feature = "std")]
fn dispatch_profile_jit_map_load(
    profile_session: &mut Option<crate::profile::ProfileSession>,
    cycle_count: u64,
    memory: &Memory,
    base: u32,
    len: u32,
    count: u32,
    entries_ptr: u32,
) {
    let Some(session) = profile_session.as_mut() else {
        return;
    };

    let _ = len; // future: validate base..base+len contains all entries

    const MAX_ENTRIES: u32 = 4096;
    if count > MAX_ENTRIES {
        log::warn!("SYSCALL_JIT_MAP_LOAD: count={count} exceeds MAX_ENTRIES; truncating");
    }
    let count = count.min(MAX_ENTRIES);

    let mut decoded: Vec<(u32, u32, String)> = Vec::with_capacity(count as usize);
    for i in 0..count {
        let entry_addr = entries_ptr.wrapping_add(i.wrapping_mul(16));
        let Ok(offset) = memory.read_word(entry_addr) else {
            log::warn!("SYSCALL_JIT_MAP_LOAD: read entry[{i}].offset failed");
            return;
        };
        let Ok(size) = memory.read_word(entry_addr.wrapping_add(4)) else {
            log::warn!("SYSCALL_JIT_MAP_LOAD: read entry[{i}].size failed");
            return;
        };
        let Ok(name_ptr) = memory.read_word(entry_addr.wrapping_add(8)) else {
            log::warn!("SYSCALL_JIT_MAP_LOAD: read entry[{i}].name_ptr failed");
            return;
        };
        let Ok(name_len) = memory.read_word(entry_addr.wrapping_add(12)) else {
            log::warn!("SYSCALL_JIT_MAP_LOAD: read entry[{i}].name_len failed");
            return;
        };

        let name = match read_jit_name(memory, name_ptr as u32, name_len as u32) {
            Some(s) => s,
            None => {
                log::warn!("SYSCALL_JIT_MAP_LOAD: read entry[{i}] name failed");
                continue;
            }
        };

        decoded.push((offset as u32, size as u32, name));
    }

    session.on_jit_map_load(base, &decoded, cycle_count);
}

#[cfg(feature = "std")]
fn read_jit_name(memory: &Memory, ptr: u32, len: u32) -> Option<String> {
    const MAX_NAME_LEN: u32 = 256;
    let len = len.min(MAX_NAME_LEN);
    let mut bytes = Vec::with_capacity(len as usize);
    for i in 0..len {
        bytes.push(memory.read_u8(ptr.wrapping_add(i)).ok()?);
    }
    String::from_utf8(bytes).ok()
}
```

Note: `Memory::read_word` returns `i32` per existing usage in
`profile/mod.rs::unwind_backtrace_inner`. Cast to `u32` as shown above.
`Memory::read_u8` returns `Result<u8, _>` per `read_memory_string` in
the same file. If signatures differ, match the actual existing API
rather than guessing.

In `handle_syscall`, add two new branches **after** the existing
`SYSCALL_ALLOC_TRACE` branch and **before** the trailing `else`:

```rust
} else if syscall_info.number == SYSCALL_JIT_MAP_LOAD {
    #[cfg(feature = "std")]
    {
        let base = syscall_info.args[0] as u32;
        let len = syscall_info.args[1] as u32;
        let count = syscall_info.args[2] as u32;
        let entries_ptr = syscall_info.args[3] as u32;
        dispatch_profile_jit_map_load(
            &mut self.profile_session,
            self.cycle_count,
            &self.memory,
            base, len, count, entries_ptr,
        );
    }
    self.regs[Gpr::A0.num() as usize] = 0;
    Ok(StepResult::Continue)
} else if syscall_info.number == SYSCALL_JIT_MAP_UNLOAD {
    log::error!(
        "SYSCALL_JIT_MAP_UNLOAD not yet implemented; symbols may be stale"
    );
    self.regs[Gpr::A0.num() as usize] = 0;
    Ok(StepResult::Continue)
}
```

### 3. Tests

#### Test for the syscall handler

Add a new test module at the bottom of
`lp-riscv/lp-riscv-emu/src/profile/mod.rs` (or a sibling test file if
you prefer — matches existing convention there). It should:

- Build a `Riscv32Emulator` (existing test pattern in this crate; see
  the existing tests in this file for how to construct one with a
  `ProfileSession`).
- Write a `JitSymbolEntry` array + name strings into guest memory.
- Construct a syscall info with `number = SYSCALL_JIT_MAP_LOAD` and
  `args = [base, len, count, entries_ptr, ...]`.
- Drive `handle_syscall` (or call `dispatch_profile_jit_map_load`
  directly if that's simpler — but prefer end-to-end).
- Assert the session's `jit_symbols.lookup(...)` resolves.

If constructing a full emulator is too heavy for a unit test, a
direct test of `dispatch_profile_jit_map_load` against a synthetic
`Memory` is acceptable — but in that case explain the trade-off in a
comment and ensure the integration phase (phase 7) really exercises
the full path.

#### Test for the `meta.json` rewrite

Test in the same module: create a `ProfileSession` in a `tempfile`
dir, push some entries into `jit_symbols` directly via the new
public method, call `finish_with_symbolizer(None)`, then read
`meta.json` back and assert `dynamic_symbols` is present with the
expected shape.

### 4. Notes

- The existing `session_creates_dir_and_meta` test in `profile/mod.rs`
  asserts presence of `meta.json` after `finish()`. Update the
  expectation if the file's structure changes (it shouldn't — we only
  add a top-level field; existing fields are preserved).
- `MAX_ENTRIES = 4096` and `MAX_NAME_LEN = 256` are defensive bounds
  (matches the spirit of `MAX_STRING_LEN = 1024` in
  `read_memory_string`). Tune if the validation pass complains about
  realistic shaders exceeding them.

## Validate

```bash
cargo test -p lp-riscv-emu
cargo build -p lp-riscv-emu
```

Both must succeed cleanly with no warnings. All pre-existing tests in
`lp-riscv-emu` must still pass.
