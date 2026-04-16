## Scope of phase

### 1. Wire filetests + wasm to shared crates (not `lps-frontend`)

**Already done (prerequisite):** **`lps-diagnostics`**, **`lps-shared`**,
**`lpvm`**, **`lps-exec`** exist as copies; legacy **`lps-frontend`**
/ **`lps-cranelift`** were **not** refactored.

**This phase:**

- **`lps-filetests`:** depend on **`lps-exec`**, **`lpvm`**, and
  **`lps-diagnostics`** (and **`lps-shared`** if any shared signature types are
  needed at compile boundaries). Replace imports that pointed at
  **`lps_cranelift::exec::…`** with **`lps_exec`**, **`lpvm`**, etc.
- **`lps-wasm`:** **`impl GlslExecutable` for WasmExecutable** using
  **`lps_exec::GlslExecutable`**, **`lpvm::GlslValue`**, and
  **`lps_diagnostics::GlslError`** (match error mapping to what filetests expect).
- **Legacy `lps-cranelift`:** **leave as-is** for V2 (optional later: re-export
  or depend on new crates for non-filetests callers — **Stage VII**).

**Trait shape:** **`lps-exec`** intentionally omits Cranelift-only hooks (e.g.
**`DirectCallInfo`** / **`get_direct_call_info`**); legacy JIT utilities keep those
until the old crate goes away.

**Cycles:** **`lps-exec`** must not depend on wasm, **`lpvm-cranelift`**, or
filetests. Impls live in **`lps-wasm`**, filetests adapters, etc.

### 2. Filetests `Cargo.toml`

- Add **`lpvm-cranelift`** with needed features for **`jit`** / **`rv32`**.
- Add **`lps-exec`**, **`lpvm`**, **`lps-diagnostics`** (and
  **`lps-shared`** if needed).
- **Remove `lps-cranelift`** once filetests no longer imports it (error tests
  included — switch to **`lpvm_cranelift::jit`** or shared parse errors as
  appropriate).

### 3. `compile_for_target`

- **`Wasm`** → existing wasm path (executable implements **`lps_exec`**).
- **`Jit`** → **`LpirJitExecutable`**.
- **`Rv32`** → **`LpirRv32Executable`**.
- **No `Cranelift` arm.**

## Code organization reminders

- Prefer **mechanical import rewrites**; keep **git commits** compiling when
  possible (deps + imports first, then remove **`lps-cranelift`** from
  filetests).

## Implementation details

- **`test_error`:** replace **`glsl_emu_riscv32_with_metadata`** with
  **`lpvm_cranelift::jit`** (or stricter parse-only tests) so error expectations
  stay meaningful.
- **Features:** optional **`lpir-filetests`** flag if binary size matters.

## Tests

- **`cargo test -p lps-wasm`**, **`cargo test -p lps-filetests`** after
  wiring.
- Smoke: **`compile_for_target`** for **`jit.q32`**, **`wasm.q32`**, **`rv32.q32`**
  when V1 ready.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lps && cargo check -p lps-exec
cd /Users/yona/dev/photomancer/lp2025/lps && cargo check -p lps-wasm
cd /Users/yona/dev/photomancer/lp2025/lps && cargo check -p lps-filetests
cd /Users/yona/dev/photomancer/lp2025/lps && cargo test -p lps-filetests --lib
```

`cargo +nightly fmt`.
