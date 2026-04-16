# Phase 9: Update Workspace and Dependencies

## Scope of Phase

Update the workspace `Cargo.toml` to include `lp-riscv-emu-guest` in the workspace members, and
verify that all build scripts and references are updated correctly.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Update Workspace Cargo.toml

Update the root `Cargo.toml` to add `lp-riscv-emu-guest` to the workspace members:

```toml
[workspace]
members = [
    # ... existing members ...
    "lp-shader/lp-riscv-emu-guest", # Add after lp-riscv-tools
    # ... rest of members ...
]
```

Add it in the `lps` section, after `lp-shader/lp-riscv-tools`.

**Note**: `lp-riscv-emu-guest` should NOT be added to `default-members` since it's `no_std` and only
builds for RISC-V target, similar to `lps-builtins-emu-app`.

### 2. Verify Build Scripts

Check if any build scripts reference `lps-builtins-emu-app` and need updating:

- `lp-shader/lps-compiler/build.rs` - This references `lps-builtins-emu-app`
  executable. This
  should still work since `lps-builtins-emu-app` still exists, just as a thin wrapper.

No changes needed here - `lps-builtins-emu-app` still produces the same binary output.

### 3. Verify Justfile/scripts

Check `justfile` and `scripts/build-builtins.sh` to see if they need updates:

- They should still work since they build `lps-builtins-emu-app` which still exists
- The build process should be the same

No changes needed unless there are specific references to source files that moved.

## Validate

Run from workspace root:

```bash
# Check that lp-riscv-emu-guest compiles
cargo check --package lp-riscv-emu-guest --target riscv32imac-unknown-none-elf

# Check that lps-builtins-emu-app still compiles
cargo check --package lps-builtins-emu-app --target riscv32imac-unknown-none-elf

# Check that workspace still works
cargo check --workspace
```

All should compile successfully.
