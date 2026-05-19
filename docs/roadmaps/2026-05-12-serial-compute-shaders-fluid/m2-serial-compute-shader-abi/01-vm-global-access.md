# Phase 1: VM Global Access

## Scope Of Phase

Add host-side read/write access for VMContext private globals in `lpvm` and all
backend `LpvmInstance` implementations.

In scope:

- Add `set_global` and `get_global` to `lpvm::LpvmInstance`.
- Add shared encoding/decoding helpers for globals.
- Implement the trait methods in active backends.
- Add focused unit tests for std430 global value reads/writes.

Out of scope:

- Compute shader compile API.
- Slot shape validation.
- Engine/node integration.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.
- Tests go at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-shader/lpvm/src/instance.rs`
- `lp-shader/lpvm/src/set_uniform.rs`
- `lp-shader/lpvm/src/lpvm_data_q32.rs`
- `lp-shader/lpvm/src/lib.rs`
- `lp-shader/lpvm-native/src/rt_emu/instance.rs`
- `lp-shader/lpvm-native/src/rt_jit/instance.rs`
- `lp-shader/lpvm-emu/src/instance.rs`
- `lp-shader/lpvm-cranelift/src/lpvm_instance.rs`
- `lp-shader/lpvm-wasm/src/...` discovered by search

Expected changes:

1. Add to `LpvmInstance`:

   ```rust
   fn set_global(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error>;
   fn get_global(&mut self, path: &str) -> Result<LpsValueF32, Self::Error>;
   ```

2. Add a helper module such as `lpvm/src/global_data.rs` with:

   - `encode_global_write(sig, path, value, float_mode) -> (offset, bytes)`;
   - `decode_global_read(sig, path, bytes_or_reader, float_mode) -> LpsValueF32`.

   Reuse `LpvmDataQ32` / std430 layout helpers rather than duplicating
   struct/array encoding.

3. Consider whether `set_uniform.rs` should be generalized or whether global
   helpers should mirror it. Do not over-refactor if it makes the phase larger.

4. Implement backend methods using existing VMContext byte read/write
   mechanisms:

   - `set_global` writes bytes at `sig.globals_offset() + rel_offset`.
   - `get_global` reads bytes from the same region and decodes them.

5. Important lifecycle rule: adding `get_global`/`set_global` must not cause
   global reset. Existing `call`, `call_q32`, and render calls may continue to
   reset as they currently do until Phase 2 introduces the compute-specific
   no-reset call path.

Edge cases:

- Modules with no globals should return a clear error.
- Unknown paths should return a path/type error, not panic.
- Arrays and structs must preserve std430 layout.
- Keep `no_std + alloc` compatibility in shader/VM crates.

## Validate

```bash
cargo fmt --check
cargo test -p lpvm
cargo test -p lpvm-native
cargo test -p lpvm-emu
cargo test -p lpvm-cranelift
```

If `lpvm-wasm` has tests or checks available, run the targeted crate check:

```bash
cargo check -p lpvm-wasm
```

