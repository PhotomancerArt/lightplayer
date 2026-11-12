# Phase 4: Create Message Protocol in lp-model

## Goal

Create message protocol with envelope wrapper, filesystem messages, and consolidate with existing project management messages.

## Tasks

1. Create `lp-model/src/message.rs`:
   - Define `Message` enum:
     ```rust
     pub enum Message {
         Client(ClientMessage),
         Server(ServerMessage),
     }
     ```
   - Define `ClientMessage { id: u64, msg: ClientRequest }`
   - Define `ServerMessage { id: u64, msg: ServerResponse }`
   - Add `Serialize` and `Deserialize` derives
   - Export from `lp-model/src/lib.rs`

2. Create `lp-model/src/server/fs_api.rs`:
   - Define `FsRequest` enum:
     ```rust
     pub enum FsRequest {
         Read { path: String },
         Write { path: String, data: Vec<u8> },
         DeleteFile { path: String },
         DeleteDir { path: String },
         ListDir { path: String, recursive: bool },
     }
     ```
   - Define `FsResponse` enum (all include error option):
     ```rust
     pub enum FsResponse {
         Read { path: String, data: Option<Vec<u8>>, error: Option<String> },
         Write { path: String, error: Option<String> },
         DeleteFile { path: String, error: Option<String> },
         DeleteDir { path: String, error: Option<String> },
         ListDir { path: String, entries: Vec<String>, error: Option<String> },
     }
     ```
   - Add `Serialize` and `Deserialize` derives
   - Export from `lp-model/src/server/mod.rs`

3. Update `lp-model/src/server/api.rs`:
   - Add `Filesystem(FsRequest)` variant to `ServerRequest`
   - Add `Filesystem(FsResponse)` variant to `ServerResponse`
   - Import `FsRequest` and `FsResponse` from `fs_api`

4. Create `ClientRequest` enum in `lp-model/src/message.rs`:
   - For now, just filesystem requests: `Filesystem(FsRequest)`
   - Can add project management requests later

5. Update `ServerResponse` in `lp-model/src/server/api.rs`:
   - Ensure it's exported and used in `ServerMessage`

6. Remove `lp-api` crate:
   - Delete `lp-app/crates/lp-api/` directory
   - Remove from workspace `Cargo.toml`
   - Update any remaining references

7. Add serialization tests:
   - Test that `Message` serializes/deserializes correctly
   - Test that nested `Filesystem(FsRequest)` serializes correctly
   - Test that all message types round-trip through JSON

## Success Criteria

- `Message` enum exists with `Client` and `Server` variants
- `ClientMessage` and `ServerMessage` wrap requests/responses with IDs
- `FsRequest` and `FsResponse` enums exist in `fs_api.rs`
- `ServerRequest` and `ServerResponse` include `Filesystem` wrapper variants
- `lp-api` crate removed
- All messages serialize/deserialize correctly
- All code compiles without warnings
