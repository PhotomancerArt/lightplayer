## Phase 1: Cargo feature `emu` and `rt_emu` module structure

### Scope

Add `emu` feature to `lpvm-native` that:
- Pulls in `lpvm-cranelift` (with `riscv32-object`), `lp-riscv-elf`, `lp-riscv-emu`, `lpvm-emu`
- Enables `std` on those dependencies
- Gates the `rt_emu/` module entirely

Create `rt_emu/` directory with stub files:
- `rt_emu/mod.rs` — re-exports
- `rt_emu/engine.rs` — `NativeEmuEngine` skeleton
- `rt_emu/module.rs` — `NativeEmuModule` skeleton  
- `rt_emu/instance.rs` — `NativeEmuInstance` skeleton

### Code organization

- Core emission (`lower.rs`, `regalloc/`, `isa/`) stays in crate root, always compiled (`no_std`)
- `rt_emu/` is a separate module only compiled with `emu` feature
- No `#[cfg]` soup in core files

### Implementation details

**`Cargo.toml`:**
```toml
[features]
default = []
emu = [
    "dep:cranelift-codegen",
    "dep:lpvm-cranelift",
    "dep:lpvm-emu",
    "dep:lp-riscv-elf",
    "dep:lp-riscv-emu",
    "lpvm-cranelift/riscv32-object",
    "lpvm-cranelift/std",
    "lpvm-emu/std",
    "lp-riscv-emu/std",
]
```

**`src/lib.rs`:**
```rust
#[cfg(feature = "emu")]
pub mod rt_emu;

#[cfg(feature = "emu")]
pub use rt_emu::{NativeEmuEngine, NativeEmuInstance, NativeEmuModule};
```

### Tests

```bash
cargo check -p lpvm-native
cargo check -p lpvm-native --features emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

All must pass without warnings.
