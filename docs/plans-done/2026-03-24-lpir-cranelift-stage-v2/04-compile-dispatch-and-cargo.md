## Scope of phase

### 1. Wire filetests + wasm to shared crates (not `lp-glsl-frontend`)

**Already done (prerequisite):** **`lp-glsl-diagnostics`**, **`lps-shared`**,
**`lpvm`**, **`lp-glsl-exec`** exist as copies; legacy **`lp-glsl-frontend`**
/ **`lp-glsl-cranelift`** were **not** refactored.

**This phase:**

- **`lp-glsl-filetests`:** depend on **`lp-glsl-exec`**, **`lpvm`**, and
  **`lp-glsl-diagnostics`** (and **`lps-shared`** if any shared signature types are
  needed at compile boundaries). Replace imports that pointed at
  **`lp_glsl_cranelift::exec::…`** with **`lp_glsl_exec`**, **`lpvm`**, etc.
- **`lp-glsl-wasm`:** **`impl GlslExecutable` for WasmExecutable** using
  **`lp_glsl_exec::GlslExecutable`**, **`lpvm::GlslValue`**, and
  **`lp_glsl_diagnostics::GlslError`** (match error mapping to what filetests expect).
- **Legacy `lp-glsl-cranelift`:** **leave as-is** for V2 (optional later: re-export
  or depend on new crates for non-filetests callers — **Stage VII**).

**Trait shape:** **`lp-glsl-exec`** intentionally omits Cranelift-only hooks (e.g.
**`DirectCallInfo`** / **`get_direct_call_info`**); legacy JIT utilities keep those
until the old crate goes away.

**Cycles:** **`lp-glsl-exec`** must not depend on wasm, **`lpir-cranelift`**, or
filetests. Impls live in **`lp-glsl-wasm`**, filetests adapters, etc.

### 2. Filetests `Cargo.toml`

- Add **`lpir-cranelift`** with needed features for **`jit`** / **`rv32`**.
- Add **`lp-glsl-exec`**, **`lpvm`**, **`lp-glsl-diagnostics`** (and
  **`lps-shared`** if needed).
- **Remove `lp-glsl-cranelift`** once filetests no longer imports it (error tests
  included — switch to **`lpir_cranelift::jit`** or shared parse errors as
  appropriate).

### 3. `compile_for_target`

- **`Wasm`** → existing wasm path (executable implements **`lp_glsl_exec`**).
- **`Jit`** → **`LpirJitExecutable`**.
- **`Rv32`** → **`LpirRv32Executable`**.
- **No `Cranelift` arm.**

## Code organization reminders

- Prefer **mechanical import rewrites**; keep **git commits** compiling when
  possible (deps + imports first, then remove **`lp-glsl-cranelift`** from
  filetests).

## Implementation details

- **`test_error`:** replace **`glsl_emu_riscv32_with_metadata`** with
  **`lpir_cranelift::jit`** (or stricter parse-only tests) so error expectations
  stay meaningful.
- **Features:** optional **`lpir-filetests`** flag if binary size matters.

## Tests

- **`cargo test -p lp-glsl-wasm`**, **`cargo test -p lp-glsl-filetests`** after
  wiring.
- Smoke: **`compile_for_target`** for **`jit.q32`**, **`wasm.q32`**, **`rv32.q32`**
  when V1 ready.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo check -p lp-glsl-exec
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo check -p lp-glsl-wasm
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo check -p lp-glsl-filetests
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo test -p lp-glsl-filetests --lib
```

`cargo +nightly fmt`.
