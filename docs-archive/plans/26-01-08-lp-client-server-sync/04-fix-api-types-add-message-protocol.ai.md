# Phase 4: Fix API Types and Add Message Protocol

## Goal

Fix `NodeSpecifier` to use `NodeHandle`, and create message protocol infrastructure.

## Tasks

1. Fix `NodeSpecifier` in `lp-shared/src/project/api.rs`:
   - Change `ByHandles(Vec<i32>)` to `ByHandles(Vec<NodeHandle>)`
   - Update any code that uses `NodeSpecifier`

2. Create `Message` enum in `lp-shared/src/message.rs`:
   ```rust
   pub enum Message {
       Request {
           id: u64,
           request: ServerRequest,
       },
       Response {
           id: u64,
           response: ServerResponse,
       },
       Log {
           level: LogLevel,
           message: String,
       },
   }
   ```
   - Add `Serialize` and `Deserialize` derives
   - Add `LogLevel` enum if needed

3. Create `ClientTransport` trait in `lp-shared/src/transport.rs`:
   ```rust
   pub trait ClientTransport {
       fn send_message(&mut self, message: &Message) -> Result<(), Error>;
       fn receive_message(&mut self) -> Result<Option<Message>, Error>;
   }
   ```
   - Returns `Option<Message>` for non-blocking receive (None if no message available)

4. Update `lp-shared/src/lib.rs`:
   - Export `message` and `transport` modules

5. Add serialization support:
   - Ensure `Message`, `ServerRequest`, `ServerResponse` are serializable
   - Use JSON serialization (compression can be added later)

## Success Criteria

- `NodeSpecifier` uses `NodeHandle` instead of `i32`
- `Message` enum exists with Request/Response/Log variants
- `ClientTransport` trait is defined
- All types are serializable
- All code compiles without warnings
