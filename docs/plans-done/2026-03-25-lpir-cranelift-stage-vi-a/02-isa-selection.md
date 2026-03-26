# Phase 2: ISA selection

## Scope

Replace hard-wired `cranelift_native::builder()` with a cfg-split helper so
the crate builds and JITs on both `std` (auto-detect) and `no_std` (explicit
`riscv32imac` triple).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Extract `build_isa` helper in `jit_module.rs`

Currently `build_jit_module` calls `cranelift_native::builder()` directly.
Extract to a helper with cfg split:

```rust
use cranelift_codegen::isa::OwnedTargetIsa;
use cranelift_codegen::settings;

#[cfg(feature = "std")]
fn build_isa(flags: settings::Flags) -> Result<OwnedTargetIsa, CompilerError> {
    cranelift_native::builder()
        .map_err(|m| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "native ISA detection: {m}"
            )))
        })?
        .finish(flags)
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!("ISA: {e}")))
        })
}

#[cfg(not(feature = "std"))]
fn build_isa(flags: settings::Flags) -> Result<OwnedTargetIsa, CompilerError> {
    use cranelift_codegen::isa;
    use target_lexicon::Triple;

    let triple: Triple = "riscv32imac-unknown-none-elf"
        .parse()
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "parse triple: {e}"
            )))
        })?;
    isa::lookup(triple)
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!(
                "ISA lookup: {e}"
            )))
        })?
        .finish(flags)
        .map_err(|e| {
            CompilerError::Codegen(CompileError::cranelift(alloc::format!("ISA: {e}")))
        })
}
```

Then in `build_jit_module`, replace the `cranelift_native::builder()` block with
`let isa = build_isa(flags)?;`.

### 2. Ensure `cranelift-codegen/riscv32` is available without `std`

The `riscv32-emu` feature already enables `cranelift-codegen/riscv32`. For the
`no_std` path we also need the riscv32 backend. Options:

- **A)** Always enable `cranelift-codegen/riscv32` (small binary size cost on
  host builds).
- **B)** Add a separate feature like `riscv32-isa` that `riscv32-emu` and the
  `no_std` path both use.

Go with **A** for simplicity — the riscv32 backend codegen tables are small and
host builds already have `host-arch` which dominates. Add
`"cranelift-codegen/riscv32"` to the base dependency (not behind a feature).

### 3. JIT memory provider

Check whether `cranelift-jit` without `std` uses a different default memory
provider or requires one to be set. If it requires an explicit provider
(e.g. `AllocJitMemoryProvider` or similar), wire it in the `no_std` path
of `build_jit_module`. This may require:

```rust
#[cfg(not(feature = "std"))]
{
    // cranelift-jit may need a custom memory provider without mmap
    jit_builder.memory_provider(/* ... */);
}
```

Check the old crate's `backend/target/builder.rs` for how it set up
`AllocJitMemoryProvider` on `no_std` and replicate.

### 4. Object module ISA (if applicable)

`object_module.rs` (behind `riscv32-emu`) likely has its own ISA construction.
Since `riscv32-emu` implies `std`, this path is unaffected. Verify it still
compiles.

## Tests

```rust
#[test]
fn jit_compiles_with_default_features() {
    // Existing tests cover this — no new test needed
}
```

No new test for the `no_std` ISA path in this phase (it's validated by
`cargo check` cross-compile). Functional testing happens in VI-B.

## Validate

```bash
# Cross-compile should now pass (Phase 1 + Phase 2 together)
cargo check --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features

# Host tests unchanged
cargo test -p lpir-cranelift
cargo test -p lpir-cranelift --features riscv32-emu
```
