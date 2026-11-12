# Phase 3: Refactor MemoryTransport to LocalMemoryTransport (single-threaded)

## Goal

Refactor `MemoryTransport` to `LocalMemoryTransport` that is explicitly single-threaded and doesn't require `std`. Remove the conditional compilation for `Arc<Mutex>` vs `Rc<RefCell>` and use only `Rc<RefCell>`.

## Tasks

1. Update `lp-client/src/transport/memory.rs`:
   - Rename `MemoryTransport` to `LocalMemoryTransport`
   - Remove `#[cfg(feature = "std")]` and `#[cfg(not(feature = "std"))]` conditional compilation
   - Remove `Arc<Mutex>` implementation (std version)
   - Keep only `Rc<RefCell>` implementation (no_std version)
   - Remove `extern crate std;` and std imports
   - Update all references to use `Rc<RefCell>` consistently
   - Update struct documentation to note it's single-threaded
   - Update `new_pair()` to only have one implementation (no_std version)
   - Update `ClientTransport` and `ServerTransport` implementations:
     - Remove conditional compilation blocks
     - Use `borrow_mut()` instead of `lock().unwrap()`
     - Handle serialization/deserialization internally:
       - `send()`: Serialize `ClientMessage`/`ServerMessage` to JSON bytes
       - `receive()`: Deserialize JSON bytes to `ServerMessage`/`ClientMessage`
     - Return appropriate error types for serialization failures

2. Update `lp-client/src/transport/mod.rs`:
   - Update export: `pub use memory::LocalMemoryTransport;`
   - Update module documentation if needed

3. Update `lp-client/src/lib.rs`:
   - Update export: `pub use transport::LocalMemoryTransport;`

4. Verify compilation:
   - Run `cargo check` in `lp-client` to ensure it compiles
   - Note that callers will fail to compile (expected, will be fixed in phase 5)

## Success Criteria

- [ ] `MemoryTransport` renamed to `LocalMemoryTransport`
- [ ] Only `Rc<RefCell>` implementation exists (no std requirement)
- [ ] `LocalMemoryTransport` handles serialization internally
- [ ] `send()` serializes messages to JSON bytes
- [ ] `receive()` deserializes JSON bytes to messages
- [ ] Serialization errors are wrapped in `TransportError::Serialization`/`Deserialization`
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
