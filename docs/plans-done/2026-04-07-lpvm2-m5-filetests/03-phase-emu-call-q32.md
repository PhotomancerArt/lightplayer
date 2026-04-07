# Phase 3: `EmuInstance` — `call_q32` + `debug_state`

## Scope of phase

Implement on `EmuInstance` in `lp-shader/lpvm-emu/src/instance.rs`:

- **`call_q32`:** Same semantics as Phase 2: exact flat `i32` ABI, reuse emulator’s existing Q32 invoke path (no `f32` for float lanes).
- **`debug_state`:** Port logic from current **`format_emulator_state()`** on the old filetest RV32 executable / `GlslExecutable` — registers, PC, last trap, etc., as `Option<String>`.

## Code Organization Reminders

- Keep emulator formatting in `lpvm-emu`; avoid `lps-filetests` knowing RV32 register layout.
- Helpers at bottom of `instance.rs` or a small `debug_format.rs` if large.

## Implementation Details

- If `emu_run` / `glsl_q32_call_emulated` already takes flat `i32`, **`call_q32` should call that directly** after arity/type checks.
- **`debug_state`:** On success return `None`; on trap / error path the instance should capture enough state to format (may require storing last trap info on `EmuInstance`).

## Validate

```bash
cargo check -p lpvm-emu
cargo test -p lpvm-emu
```
