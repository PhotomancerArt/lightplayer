# Phase 10: Cleanup, Review, and Validation

## Scope of Phase

Final cleanup, remove any temporary code, fix warnings, and validate that everything works
correctly.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Remove Temporary Code

Grep for any temporary code, TODOs, or debug prints:

```bash
# From workspace root
grep -r "TODO\|FIXME\|XXX\|HACK" lp-glsl/crates/lp-riscv-emu-guest/
grep -r "TODO\|FIXME\|XXX\|HACK" lp-glsl/apps/lp-builtins-app/src/
```

Remove any temporary code found.

### 2. Fix Warnings

Run clippy and fix any warnings:

```bash
cargo clippy --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
cargo clippy --package lp-builtins-app --target riscv32imac-unknown-none-elf
```

Fix all warnings.

### 3. Format Code

Run rustfmt on all changed files:

```bash
cargo +nightly fmt --package lp-riscv-emu-guest
cargo +nightly fmt --package lp-builtins-app
```

### 4. Verify Build

Build `lp-builtins-app` to ensure it still produces the expected binary:

```bash
# From workspace root
scripts/build-builtins.sh
```

Or manually:

```bash
cargo build --package lp-builtins-app --target riscv32imac-unknown-none-elf --release
```

Verify the binary is produced at:
`target/riscv32imac-unknown-none-elf/release/lp-builtins-app`

### 5. Verify Integration

Check that `lp-glsl-compiler` can still use `lp-builtins-app`:

```bash
cargo build --package lp-glsl-compiler
```

This should build successfully and embed `lp-builtins-app` as before.

### 6. Check File Structure

Verify the final file structure matches the design:

```
lp-glsl/crates/lp-riscv-emu-guest/
├── Cargo.toml
├── build.rs
├── memory.ld
└── src/
    ├── lib.rs
    ├── entry.rs
    ├── panic.rs
    ├── syscall.rs
    ├── host.rs
    └── print.rs

lp-glsl/apps/lp-builtins-app/
├── Cargo.toml
└── src/
    ├── main.rs
    └── builtin_refs.rs
```

### 7. Verify Public API

Check that the public API is clean:

- `lp-riscv-emu-guest::entry` - Entry point module (functions are `#[no_mangle]`)
- `lp-riscv-emu-guest::host` - Host communication functions
- `lp-riscv-emu-guest::print` - Print macros
- `lp-riscv-emu-guest::ebreak` - Halt function
- `lp-riscv-emu-guest::print!` - Print macro
- `lp-riscv-emu-guest::println!` - Println macro

## Validate

Run the full validation suite:

```bash
# Check compilation
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
cargo check --package lp-builtins-app --target riscv32imac-unknown-none-elf
cargo check --package lp-glsl-compiler

# Check formatting
cargo +nightly fmt --check --package lp-riscv-emu-guest
cargo +nightly fmt --check --package lp-builtins-app

# Check clippy
cargo clippy --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
cargo clippy --package lp-builtins-app --target riscv32imac-unknown-none-elf

# Build lp-builtins-app
scripts/build-builtins.sh
```

All should pass successfully.
