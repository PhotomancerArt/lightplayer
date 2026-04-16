## Phase 5: Remove riscv32-emu from lpvm-cranelift

Clean up the old feature and prepare for migration.

### Code Organization

**File: `lp-shader/lpvm-cranelift/Cargo.toml`**

Remove:
```toml
# Remove these lines:
riscv32-emu = [
    "std",
    "glsl",
    "dep:cranelift-object",
    "dep:lp-riscv-elf",
    "dep:lp-riscv-emu",
    "cranelift-codegen/riscv32",
]

# Remove these deps:
cranelift-object = { workspace = true, optional = true }
lp-riscv-elf = { path = "../../lp-riscv/lp-riscv-elf", optional = true }
lp-riscv-emu = { path = "../../lp-riscv/lp-riscv-emu", optional = true, features = ["std"] }
```

Keep `cranelift-object` if it's still needed for something else - but it's only used for RV32 emulation.

**File: `lp-shader/lpvm-cranelift/src/lib.rs`**

Remove:
```rust
// Remove these lines:
#[cfg(feature = "riscv32-emu")]
mod emu_run;
#[cfg(feature = "riscv32-emu")]
mod object_link;
#[cfg(feature = "riscv32-emu")]
mod object_module;

// Remove these exports:
#[cfg(feature = "riscv32-emu")]
pub use compile::{object_bytes_from_ir, run_lpir_function_i32};
#[cfg(feature = "riscv32-emu")]
pub use emu_run::glsl_q32_call_emulated;
#[cfg(feature = "riscv32-emu")]
pub use object_link::link_object_with_builtins;
```

**File: `lp-shader/lpvm-cranelift/src/compile.rs`**

Remove:
```rust
// Remove this function:
#[cfg(feature = "riscv32-emu")]
pub fn object_bytes_from_ir(...) { ... }
```

**Delete file: `lp-shader/lpvm-cranelift/src/emu_run.rs`**

This file moves to `lpvm-emu` (or is reimplemented there).

**Delete file: `lp-shader/lpvm-cranelift/src/object_link.rs`**

This moves to `lpvm-emu/src/compile.rs` as internal function.

**Delete file: `lp-shader/lpvm-cranelift/src/object_module.rs`**

This moves to `lpvm-emu` as internal compilation step.

**File: `lp-shader/lpvm-cranelift/build.rs`**

If this has riscv32-emu specific build logic (embedding builtins ELF), remove it or move to `lpvm-emu`.

### Validate

```bash
# Verify lpvm-cranelift still compiles without riscv32-emu
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift  # JIT tests should still pass

# Verify no references to riscv32-emu remain
grep -r "riscv32-emu" lp-shader/lpvm-cranelift/
grep -r "emu_run" lp-shader/lpvm-cranelift/src/
```

### Note

This will break `lps-filetests` temporarily. We fix that in the next phase by having it depend on `lpvm-emu` instead.
