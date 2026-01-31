# Phase 6: Add build recipe to justfile

## Scope of phase

Add a build recipe for `lp-riscv-emu-guest-test-app` to the justfile, similar to
`build-rv32-jit-test`.
Add it to the `build-rv32` dependencies so it gets built as part of the RISC-V32 build process.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update `justfile`

Add a new build recipe after `build-rv32-jit-test`:

```justfile
# riscv32: emu-guest-test-app
build-rv32-emu-guest-test-app: install-rv32-target
    cd lp-riscv/lp-riscv-emu-guest-test-app && cargo build --target {{rv32_target}} --release
```

### 2. Update `build-rv32` recipe

Add `build-rv32-emu-guest-test-app` to the dependencies:

```justfile
build-rv32: install-rv32-target build-rv32-jit-test build-fw-esp32 build-rv32-emu-guest-test-app
```

### 3. Add clippy recipe (optional but recommended)

Add clippy check for the test app:

```justfile
# riscv32: emu-guest-test-app clippy
clippy-rv32-emu-guest-test-app: install-rv32-target
    cd lp-riscv/lp-riscv-emu-guest-test-app && cargo clippy --target {{rv32_target}} --release -- --no-deps -D warnings
```

And add it to `clippy-rv32`:

```justfile
clippy-rv32: install-rv32-target clippy-rv32-jit-test clippy-fw-esp32 clippy-rv32-emu-guest-test-app
```

## Validate

Run from workspace root:

```bash
just build-rv32-emu-guest-test-app
```

Ensure:

- Binary builds successfully
- Binary is created at `target/riscv32imac-unknown-none-elf/release/lp-riscv-emu-guest-test-app`
- No warnings

Then verify it's included in full build:

```bash
just build-rv32
```

Ensure:

- `build-rv32-emu-guest-test-app` runs as part of `build-rv32`
- All builds succeed
