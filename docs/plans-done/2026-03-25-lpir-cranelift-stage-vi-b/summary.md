# Stage VI-B summary — lp-engine on `lpvm-cranelift`

## Shipped

- **`invoke_i32_args_returns_buf` / `aarch64_invoke_multi_ret_buf`:** zero-heap return path for
  multi-return JIT calls (mirrors existing `invoke_i32_args_returns` / AArch64 asm).
- **`DirectCall::call_i32_buf`:** writes return words into `&mut [i32]`; *
  *`unsafe impl Send + Sync for DirectCall`** (same contract as raw function pointers to finalized
  code).
- **`unsafe impl Send + Sync for JitModule`** (Stage VI-B design).
- **`lp-engine`:** `lps-cranelift`, `cranelift-codegen`, and `lps-jit-util` removed; *
  *`lpvm-cranelift`** added with `default-features = false`. Features **`cranelift-optimizer`** / *
  *`cranelift-verifier`** / **`std`** forward to **`lpvm-cranelift`** ( **`lp-server`** already
  forwarded via **`lp-engine`**).
- **`ShaderRuntime`:** stores **`Option<JitModule>`** + **`Option<DirectCall>`**; compiles with *
  *`lpvm_cranelift::jit`** and **`CompileOptions`** (Q32, mapped **`GlslOpts` → `Q32Options`**, *
  *`max_errors: Some(20)`**, **`MemoryStrategy::Default`** to match prior host *
  *`memory_optimized == false`**); render uses **`call_i32_buf`** and stack **`[i32; 4]`**; removed
  **`dyn GlslExecutable`** / **`call_vec`** fallback.
- **Tests:** **`direct_call_i32_buf_matches_call_i32`** in **`lpvm-cranelift`**.

## Validation

- `cargo test -p lp-engine`, `cargo test -p lpvm-cranelift`,
  `cargo test -p lpvm-cranelift --features riscv32-emu`
- `cargo clippy -p lp-engine -p lp-server -p lpvm-cranelift --all-features -- -D warnings`
- `cargo check --target riscv32imac-unknown-none-elf -p lpvm-cranelift --no-default-features`
- `cargo build -p fw-emu --target riscv32imac-unknown-none-elf`

## Follow-ups

- **`Q32Options`** / **`max_errors`:** still mostly forward-compat in **`lpvm-cranelift`** emitter (
  VI-A notes).
- **`MemoryStrategy::LowMemory`:** not enabled from **`lp-engine`** host path (matches old *
  *`GlslOptions::default_memory_optimized()`** with **`std`**); revisit for embedded **`lp-engine`**
  without **`std`** if that becomes a supported configuration.
