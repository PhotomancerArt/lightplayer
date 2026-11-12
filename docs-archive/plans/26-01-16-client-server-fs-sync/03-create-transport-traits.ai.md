# Phase 3: Create Transport Traits in lp-shared

## Goal

Create `ClientTransport` and `ServerTransport` traits in `lp-shared` for pluggable transport implementations.

## Tasks

1. Create `lp-shared/src/transport/mod.rs`:
   - Export `client` and `server` modules
   - Export `TransportError` type

2. Create `lp-shared/src/transport/client.rs`:
   - Define `ClientTransport` trait:
     ```rust
     pub trait ClientTransport {
         fn send(&mut self, msg: Message) -> Result<(), TransportError>;
         fn receive(&mut self) -> Result<Option<Message>, TransportError>;
     }
     ```
   - Messages are consumed (moved) on send
   - `receive()` returns `Option<Message>` for non-blocking (None if no message available)

3. Create `lp-shared/src/transport/server.rs`:
   - Define `ServerTransport` trait (same interface as `ClientTransport`):
     ```rust
     pub trait ServerTransport {
         fn send(&mut self, msg: Message) -> Result<(), TransportError>;
         fn receive(&mut self) -> Result<Option<Message>, TransportError>;
     }
     ```
   - Separate trait for clarity (client vs server side)

4. Create `TransportError` in `lp-shared/src/transport/mod.rs` or `lp-shared/src/error.rs`:
   - `pub enum TransportError { ... }`
   - Variants: `Serialization(String)`, `Deserialization(String)`, `ConnectionLost`, `Other(String)`
   - Implement `Display` trait

5. Update `lp-shared/src/lib.rs`:
   - Export `transport` module

6. Add `Message` type placeholder (will be defined in lp-model):
   - For now, can use a simple type or import from lp-model
   - Or define basic structure here and refine later

## Success Criteria

- `ClientTransport` trait exists in `lp-shared/src/transport/client.rs`
- `ServerTransport` trait exists in `lp-shared/src/transport/server.rs`
- `TransportError` type exists
- Traits are exported from `lp-shared`
- All code compiles without warnings
