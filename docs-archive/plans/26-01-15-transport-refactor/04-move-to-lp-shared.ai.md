# Phase 4: Move LocalMemoryTransport to lp-shared

## Goal

Move `LocalMemoryTransport` from `lp-client` to `lp-shared` so it can be used by both client and server code.

## Tasks

1. Create `lp-shared/src/transport/memory.rs`:
   - Copy `LocalMemoryTransport` implementation from `lp-client/src/transport/memory.rs`
   - Update imports to use `lp_shared::transport` instead of `lp_client::transport`
   - Ensure all necessary imports are present (`lp_model`, `serde_json`, etc.)

2. Update `lp-shared/src/transport/mod.rs`:
   - Add `pub mod memory;`
   - Add `pub use memory::LocalMemoryTransport;`

3. Update `lp-shared/src/lib.rs`:
   - Add `pub use transport::LocalMemoryTransport;` (optional, for convenience)

4. Update `lp-client/src/transport/mod.rs`:
   - Remove `pub mod memory;`
   - Remove `pub use memory::LocalMemoryTransport;`

5. Update `lp-client/src/lib.rs`:
   - Change export to: `pub use lp_shared::LocalMemoryTransport;`

6. Delete `lp-client/src/transport/memory.rs`:
   - File is no longer needed in `lp-client`

7. Delete `lp-client/src/transport/mod.rs` if it's now empty:
   - If the transport module is empty, remove it entirely
   - Update `lp-client/src/lib.rs` to remove `pub mod transport;`

8. Update `lp-client/Cargo.toml`:
   - Ensure `lp-shared` dependency is present (should already be there)
   - Verify no feature gates needed for `LocalMemoryTransport`

9. Verify compilation:
   - Run `cargo check` in `lp-shared` to ensure it compiles
   - Run `cargo check` in `lp-client` to ensure it compiles
   - Note that callers will fail to compile (expected, will be fixed in phase 5)

## Success Criteria

- [ ] `LocalMemoryTransport` exists in `lp-shared/src/transport/memory.rs`
- [ ] `LocalMemoryTransport` is exported from `lp-shared/src/transport/mod.rs`
- [ ] `LocalMemoryTransport` removed from `lp-client`
- [ ] `lp-client` imports `LocalMemoryTransport` from `lp-shared`
- [ ] `lp-shared` compiles
- [ ] `lp-client` compiles (callers will fail, that's expected)
- [ ] Code compiles without warnings

## Style Notes

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Run `cargo +nightly fmt` on all changes before committing
- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
