# Phase 6: Filetests — `execution.rs` and `call` / `call_q32`

## Scope of phase

Replace **`dyn GlslExecutable`** in `lps-filetests/src/test_run/execution.rs` with **`LpvmInstance`** (concrete or enum wrapper).

- **`.f32` targets:** `instance.call(name, args_as_lps_value_f32)` → compare via existing float tolerance helpers.
- **`.q32` targets:** build flat **`Vec<i32>`** args (via `lpvm_abi` / metadata), **`instance.call_q32(name, &args)`**, decode return words to **`LpsValueQ32`** for comparison against expectations.

On error: **`format!("{err}\n{}", instance.debug_state().unwrap_or_default())`** or equivalent (only append when `Some`).

Map **`Self::Error`** from instances to `anyhow::Error` / `GlslError` consistently with today’s diagnostics.

## Code Organization Reminders

- Keep return-type dispatch (`LpsType` → compare shape) in `execution.rs` or a small `compare.rs`.
- Helpers at bottom.

## Implementation Details

- **`LpsValue`** (filetest parse type) → `LpsValueF32` / flat `i32` for Q32 as needed at the boundary.
- Reuse **`q32_exec_common`** only where it still reduces duplication; prefer **`call_q32`** over re-wrapping `JitModule` directly.

## Validate

```bash
cargo check -p lps-filetests
cargo test -p lps-filetests
./scripts/filetests.sh --target jit.q32  # if available; narrow filter
```
