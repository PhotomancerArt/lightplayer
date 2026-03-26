# Stage VI-A summary — lpir-cranelift embedded readiness

## Done

- **`#![no_std]` + `alloc`:** `#[macro_use] extern crate alloc`; optional `extern crate std` when feature `std` is on.
- **Cargo features:** `default = ["std"]`; `std` enables Cranelift `std`/`host-arch`, JIT `std`, optional `cranelift-native`, `lp-glsl-builtins/std`, and **`lp-glsl-naga`** (see below). Opt-in `cranelift-optimizer`, `cranelift-verifier`. `riscv32-emu` implies `std`.
- **Base Cranelift deps:** `default-features = false` with `riscv32` on codegen and `core` on frontend/module/jit; `cranelift-native` optional.
- **ISA:** With `std`, `cranelift_native::builder()`; without `std`, explicit `riscv32imac-unknown-none-elf` via `isa::lookup`. JIT sets `is_pic = false`.
- **no_std JIT memory:** `AllocJitMemoryProvider` (`jit_memory.rs`) wired when `not(feature = "std")`.
- **`process_sync`:** real mutex under `std`, no-op guard otherwise.
- **`std::error::Error`:** impls gated on `std`.
- **`CompileOptions`:** `Q32Options` / `AddSubMode` / `MulMode` / `DivMode`, `MemoryStrategy` (`Default` | `LowMemory`), `max_errors` (stored only for now).
- **`module_lower`:** `LowMemory` sorts functions by descending IR body length; after `define_function`, `LowMemory` uses `module.clear_context(&mut ctx)`, else `ctx.clear()`.
- **`libm`:** `q32_encode_f64` uses `libm::round` so float helpers compile on `no_std` targets.
- **GLSL front-end:** `lp-glsl-naga` is optional; enabled only with `std`. `jit()` is `#[cfg(feature = "std")]`. `CompilerError::Lower` is `#[cfg(feature = "std")]`. IR-only entry points (`jit_from_ir`, object/emu when enabled) work without naga.

## Validation run

- `cargo test -p lpir-cranelift`
- `cargo test -p lpir-cranelift --features riscv32-emu`
- `cargo check --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features`
- `cargo clippy -p lpir-cranelift --all-features -- -D warnings`
- `cargo clippy --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features -- -D warnings`

## Notes

- **`Module::clear_context`:** In upstream Cranelift, this calls `ctx.clear()` then restores the module default calling convention on the context signature — slightly more than a bare `ctx.clear()`; used in `LowMemory` after each `define_function`.
- **Deferred:** `Q32Options` mode variants and `max_errors` enforcement in the pipeline (VI-B / follow-up). Functional embedded engine wiring remains VI-B.
