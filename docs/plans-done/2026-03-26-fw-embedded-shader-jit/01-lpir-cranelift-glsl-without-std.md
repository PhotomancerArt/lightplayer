# Phase 1: `lpir-cranelift` — GLSL front end without `std`

## Scope of phase

Decouple **`lps-naga`** and **`jit()`** from the **`std`** feature. **`std`** must mean **host-only** (`cranelift-native`, `extern crate std`, etc.). **`jit(source)`** (GLSL entry) must compile under **`#![no_std]` + `alloc`** when **`glsl`** (or equivalent) is enabled.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **`Cargo.toml`**
   - Add a feature (e.g. **`glsl`**) that enables **`dep:lps-naga`**.
   - Remove **`lps-naga`** from the **`std`** feature list; **`std`** keeps **`cranelift-native`** and other host-only deps.
   - Decide **`default`** features: either **`default = ["std", "glsl"]`** for host ergonomics, or **`default = ["glsl"]`** and **`std` as additive — document so **`lp-engine`** can use **`default-features = false, features = ["glsl", …]`** without **`std`**.

2. **`src/compile.rs`**
   - Gate **`pub fn jit(...)`** with **`#[cfg(feature = "glsl")]`** (not **`std`**).
   - Update the module comment that claims GLSL-in requires **`std`**.

3. **`src/lib.rs`**
   - Re-export **`jit`** under the same **`glsl`** cfg.
   - Ensure **`jit_from_ir` / `build_jit_module`** remain available without **`glsl`** for IR-only callers.

4. **`build_jit_module` / ISA**
   - Confirm **non-`std`** builds target **RISC-V32** explicitly (ESP32-C6 / `fw-emu` alignment). Fix any remaining **`std`-only** assumptions in the JIT creation path.

5. **Tests**
   - Add or adjust a test that **`cargo check -p lpir-cranelift --no-default-features --features glsl`** succeeds on **host**.
   - **`cargo check -p lpir-cranelift --no-default-features --features glsl --target riscv32imac-unknown-none-elf`** (no **`std`**).

## Tests to write

- Prefer **compile-time** checks in this phase; expand **runtime** JIT tests on RV32 in a later phase if the host cannot execute RV32 code.

## Validate

```bash
cargo +nightly fmt -p lpir-cranelift
cargo check -p lpir-cranelift
cargo check -p lpir-cranelift --no-default-features --features glsl
cargo check -p lpir-cranelift --no-default-features --features glsl --target riscv32imac-unknown-none-elf
# Host std build still works (default features)
cargo check -p lpir-cranelift --features std,glsl
cargo test -p lpir-cranelift --features std,glsl   # existing tests that need std + jit(glsl)
```

Fix new warnings introduced in this phase.
