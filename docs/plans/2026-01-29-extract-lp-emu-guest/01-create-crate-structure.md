# Phase 1: Create lp-riscv-emu-guest Crate Structure

## Scope of Phase

Create the basic structure for the `lp-riscv-emu-guest` crate, including directory, `Cargo.toml`,
and initial module files.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create Crate Directory

Create the directory structure:

```
lp-glsl/crates/lp-riscv-emu-guest/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── entry.rs
    ├── panic.rs
    ├── syscall.rs
    ├── host.rs
    └── print.rs
```

### 2. Create Cargo.toml

Create `lp-glsl/crates/lp-riscv-emu-guest/Cargo.toml`:

```toml
[package]
name = "lp-riscv-emu-guest"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["lib"]

[dependencies]
```

- Use workspace version, edition, and license
- Set crate-type to `["lib"]` (library crate)
- No dependencies yet (will add later if needed)

### 3. Create Initial Module Files

Create stub files for each module:

**src/lib.rs**:

```rust
#![no_std]

// Modules will be added in later phases
```

**src/entry.rs**:

```rust
#![allow(unused)]
// Entry point code will be added in next phase
```

**src/panic.rs**:

```rust
#![allow(unused)]
// Panic handler will be added in later phase
```

**src/syscall.rs**:

```rust
#![allow(unused)]
// Syscall implementation will be added in later phase
```

**src/host.rs**:

```rust
#![allow(unused)]
// Host communication will be added in later phase
```

**src/print.rs**:

```rust
#![allow(unused)]
// Print macros will be added in later phase
```

### 4. Add to Workspace

Update `Cargo.toml` (workspace root) to add the new crate to `members`:

```toml
"lp-glsl/crates/lp-riscv-emu-guest",
```

Add it after `lp-glsl/crates/lp-riscv-tools` in the members list.

## Validate

Run from workspace root:

```bash
cargo check --package lp-riscv-emu-guest
```

This should compile successfully (with warnings about unused code, which is expected).
