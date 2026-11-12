# Phase 1: Cargo features and `#![no_std]`

## Scope

Restructure `lpvm-cranelift` Cargo features so `std` is the default, add
`#![no_std]` to `lib.rs`, gate `std`-dependent code, and verify cross-compile.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update `Cargo.toml` features

Replace the current feature layout with:

```toml
[features]
default = ["std"]
std = [
    "cranelift-codegen/std",
    "cranelift-codegen/host-arch",
    "cranelift-frontend/std",
    "cranelift-module/std",
    "cranelift-jit/std",
    "cranelift-native",
    "lps-builtins/std",
]
cranelift-optimizer = ["cranelift-codegen/optimizer"]
cranelift-verifier = ["cranelift-codegen/verifier"]
riscv32-emu = [
    "std",
    "dep:cranelift-object",
    "dep:lp-riscv-elf",
    "dep:lp-riscv-emu",
    "cranelift-codegen/riscv32",
]
```

Update dependency lines to remove hard-coded `features = ["std"]` etc.:

```toml
cranelift-codegen = { workspace = true, default-features = false }
cranelift-frontend = { workspace = true, default-features = false }
cranelift-module = { workspace = true, default-features = false }
cranelift-jit = { workspace = true, default-features = false }
cranelift-native = { workspace = true, optional = true }
```

`cranelift-native` becomes optional (only present with `std`).

`lps-builtins` dependency needs `default-features = false` so that
`std` is forwarded only when our `std` feature is active.

### 2. Add `#![no_std]` to `lib.rs`

```rust
#![no_std]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;
```

The crate already has `extern crate alloc`. Add `#![no_std]` and the
conditional `extern crate std`.

### 3. Gate `std::error::Error` impls

In `error.rs`, wrap the three impls:

```rust
#[cfg(feature = "std")]
impl std::error::Error for CompileError {}

#[cfg(feature = "std")]
impl std::error::Error for CompilerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CompilerError::Codegen(e) => Some(e),
            _ => None,
        }
    }
}
```

In `values.rs`:

```rust
#[cfg(feature = "std")]
impl std::error::Error for CallError {}
```

### 4. Gate `process_sync`

Replace the current `process_sync.rs` with cfg-split:

```rust
#[cfg(feature = "std")]
mod imp {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static lpvm_cranelift_CODEGEN: OnceLock<Mutex<()>> = OnceLock::new();

    pub(crate) fn codegen_guard() -> MutexGuard<'static, ()> {
        lpvm_cranelift_CODEGEN
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("LPIR Cranelift codegen mutex poisoned")
    }
}

#[cfg(not(feature = "std"))]
mod imp {
    pub(crate) struct NoopGuard;
    impl Drop for NoopGuard {
        fn drop(&mut self) {}
    }
    pub(crate) fn codegen_guard() -> NoopGuard {
        NoopGuard
    }
}

pub(crate) use imp::codegen_guard;
```

### 5. Check other `std` usages

Grep for any remaining `use std::` or `std::` paths not behind `#[cfg]`.
The only known ones are in `error.rs`, `values.rs`, and `process_sync.rs`
(all handled above). If anything else surfaces, gate it.

### 6. Verify `lps-builtins` has a `std` feature

Check that `lps-builtins/Cargo.toml` has a `std` feature we can forward.
If not, add one or remove the forwarding from our `std` feature list.

## Tests

- Existing `cargo test -p lpvm-cranelift` passes (default features = std).
- Existing `cargo test -p lpvm-cranelift --features riscv32-emu` passes.

## Validate

```bash
# Cross-compile check — this is the key new validation
cargo check --target riscv32imac-unknown-none-elf -p lpvm-cranelift --no-default-features

# Host tests unchanged
cargo test -p lpvm-cranelift
cargo test -p lpvm-cranelift --features riscv32-emu
```

The cross-compile will likely fail initially due to `cranelift-native` not being
available and ISA construction in `jit_module.rs` — that's Phase 2. The goal of
this phase is to get the feature structure and `#![no_std]` gating in place. If
the cross-compile needs the ISA fix to pass, fold that minimal change into this
phase.
