# Phase 4: Runtime — JIT memory, caches, OOM

## Scope of phase

Make **on-device JIT** reliable: correct **executable memory**, **allocator** behavior, **I-cache
** / **coherency** if required on ESP32-C6, and **bounded** compile (reuse existing **`max_errors`
**, **`MemoryStrategy`** where applicable). Resolve any **first-run** failures seen when *
*`fw-tests`** execute real **`jit()`** on **`fw-emu`**.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **`lpvm-cranelift` `jit_memory` / Cranelift JIT finalize**
    - Audit allocation, alignment, and **permission** model for **RISC-V** embedded.
    - Add **platform hooks** (e.g. **cache flush**) if Cranelift or the HAL requires them for **RX**
      regions — document source (ESP-IDF, chip RM).

2. **`ShaderRuntime`**
    - Ensure **error paths** still set **`compilation_error`** / client-visible *
      *`NodeStatus::Error`** on OOM or platform failure.

3. **Logging**
    - Keep **`log`** at appropriate levels; avoid **`std`**-only logging in the hot compile path.

## Tests to write

- **`fw-tests`** are the primary integration tests for **`fw-emu`**.
- If a **unit** test can assert **JIT buffer** permissions or **flush** without hardware, add it;
  otherwise document **manual** ESP32 verification.

## Validate

```bash
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
```

Iterate until both pass. If failures are **emulator-specific**, fix **`fw-emu`** / RISC-V emu path
first; then re-check **`fw-esp32`** **`cargo check`**.
