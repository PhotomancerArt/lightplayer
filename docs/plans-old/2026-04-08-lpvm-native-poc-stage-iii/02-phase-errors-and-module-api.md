## Phase 2: Errors for `rt_emu`

### Scope

Extend `NativeError` with variants used by the emulation runtime:
- `Link(lpvm_cranelift::CompilerError)` — linking failure
- `Call(lpvm::CallError)` — execution/call failure  
- `Alloc(String)` — vmctx allocation failure

All new variants are behind `#[cfg(feature = "emu")]`.

Remove `NotYetImplemented` and `NotLinked` variants (no longer needed with explicit runtime module).

### Code organization

- Keep `error.rs` at crate root (shared across all runtimes)
- `From` impls for `CompilerError` and `CallError` also behind `emu` feature

### Implementation details

```rust
#[derive(Debug)]
pub enum NativeError {
    // ... existing variants (Lower, EmptyModule, etc.)
    #[cfg(feature = "emu")]
    Link(lpvm_cranelift::CompilerError),
    #[cfg(feature = "emu")]
    Call(lpvm::CallError),
    #[cfg(feature = "emu")]
    Alloc(String),
}
```

### Tests

```bash
cargo check -p lpvm-native
cargo check -p lpvm-native --features emu
```
