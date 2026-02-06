# Phase 6: Set Up build.rs and memory.ld

## Scope of Phase

Move the linker script (`memory.ld`) and build script (`build.rs`) from `lp-glsl-builtins-emu-app`
to
`lp-riscv-emu-guest` crate.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Copy memory.ld

Copy `lp-glsl-builtins-emu-app/memory.ld` to `lp-riscv-emu-guest/memory.ld`:

```bash
cp lp-glsl/lp-glsl-builtins-emu-app/memory.ld lp-glsl/lp-riscv-emu-guest/memory.ld
```

The file should remain unchanged.

### 2. Create build.rs

Create `lp-riscv-emu-guest/build.rs`:

```rust
fn main() {
    println!("cargo:rerun-if-changed=memory.ld");
    println!(
        "cargo:rustc-link-search=native={}",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    );
    println!("cargo:rustc-link-arg=-Tmemory.ld");
}
```

This is the same as the original `lp-glsl-builtins-emu-app/build.rs`.

### 3. Update Cargo.toml

Update `lp-riscv-emu-guest/Cargo.toml` to indicate this crate uses a build script:

```toml
[package]
name = "lp-riscv-emu-guest"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["lib"]

[build-dependencies]
```

The build script will automatically be used by Cargo.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
```

This should compile successfully and the linker script should be used during linking.
