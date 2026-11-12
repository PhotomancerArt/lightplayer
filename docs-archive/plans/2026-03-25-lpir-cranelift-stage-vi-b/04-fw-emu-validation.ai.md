# Phase 4: fw-emu validation

## Scope

Confirm `fw-emu` builds and runs with the new compiler path through
`lp-server` → `lp-engine` → `lpvm-cranelift`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Dependency graph

`fw-emu` → `lp-server` → `lp-engine` → `lpvm-cranelift`. No direct `fw-emu` edit
may be required if features propagate. Verify `fw-emu` / `lp-server` default
features enable `lp-engine/std` (and thus `lpvm-cranelift/std` for `jit()`).

### 2. Build

From workspace root (adjust if your workflow uses a script):

```bash
cargo build -p fw-emu --target riscv32imac-unknown-none-elf
# or host test binary if applicable
```

### 3. Run / smoke

Run the emulator binary per project docs (serial client, test project, etc.).
Confirm at least one shader compiles and pixels update.

### 4. Integration tests

Run any `fw-emu` dev-dependency tests documented in the repo (e.g. under
`lp-fw/fw-emu` or workspace `just` targets).

### 5. Memory profiling (optional this phase)

If profiling hooks exist for `fw-emu`, note baseline for a follow-up; not a
gate for VI-B completion.

## Validate

```bash
cargo build -p fw-emu --target riscv32imac-unknown-none-elf
# plus project-specific run / test commands
```
