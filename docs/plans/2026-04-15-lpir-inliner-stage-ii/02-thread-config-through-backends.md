# Phase 2 — Thread `CompilerConfig` through backends

## Scope of phase

Add **`pub config: lpir::CompilerConfig`** to:

- **`lpvm_native::NativeCompileOptions`** (`native_options.rs`)
- **`lpvm_cranelift::CompileOptions`** (`compile_options.rs`)
- **`lpvm_wasm::WasmOptions`** (`options.rs`)

Update **`Default`** impls to set **`config: CompilerConfig::default()`**. Replace **`Copy`** with **`Clone`** (and **`PartialEq`/`Eq`** as needed) wherever **`CompilerConfig`** prevents **`Copy`**.

Update **every** construction site: **`..Default::default()`**, field updates, and any code that assumed **`Copy`** (e.g. pass-by-value patterns may become **`.clone()`**).

**Passes:** thread **`options.config`** into **`compile_module` / `compile`** paths so **future** passes (inliner) can read it. For M1, if no pass consumes **`InlineConfig`** yet, wiring is still “plumbing only” with no semantic change.

## Code Organization Reminders

- Touch only what **`grep`** / the compiler flags for **`NativeCompileOptions`**, **`CompileOptions`**, **`WasmOptions`**.
- Keep **`CompilerConfig`** ownership clear: one **`Clone`** per compile from options is fine; no need for **`Arc`** unless profiling says otherwise.

## Implementation Details

- **`lp-core/lp-engine/src/gfx/native_jit.rs`** and any **`fw-*` / tests** that build **`NativeCompileOptions`** — add **`..Default::default()`** or explicit **`config`** fields.
- **`lps-filetests/tests/rv32n_smoke.rs`** and similar — update struct literals.
- **`lpvm_native::compile.rs`**: forward **`config`** only where the roadmap expects (inline in M4); optional comment **`// M1: config available on options`** if no consumer yet.

### Tests

```bash
cargo test -p lpvm-native
cargo test -p lpvm-cranelift
cargo test -p lpvm-wasm
```

Fix any **`cargo check -p lp-engine`** / **`fw-esp32`** breakage from option type changes before phase 3.

## Validate

```bash
cargo test -p lpvm-native
cargo test -p lpvm-cranelift
cargo test -p lpvm-wasm
cargo test -p lps-frontend
cargo check -p lp-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
```

Adjust crate paths if the repo workspace layout differs.
