# Phase 3: Create lp-api Protocol Messages

## Goal

Create the client/server protocol message types in `lp-api`.

## Tasks

1. **Define ClientMsg enum**:
   - Filesystem operations: `FsRead { path }`, `FsWrite { path, data }`, `FsExists { path }`, `FsListDir { path }`
   - File sync: `SyncStart { project_name }`, `SyncFile { path, data }`
   - Debug queries: `GetTextureData { texture_id }`, `GetNodeStatus { node_id, node_type }`, `GetOutputState { output_id }`
   - Log streaming: `SubscribeLogs { level }`

2. **Define ServerMsg enum**:
   - Filesystem responses: `FsReadResponse { path, data }`, `FsExistsResponse { path, exists }`, `FsListDirResponse { path, entries }`
   - File sync: `SyncFile { path, data }`, `SyncComplete`
   - Debug responses: `TextureData { texture_id, data, width, height, format }`, `NodeStatus { node_id, status }`, `OutputState { output_id, pixels }`
   - Logs: `Log { level, message }`
   - Errors: `Error { message }`

3. **Add LogLevel enum**:
   - `Info`, `Warn`, `Error`
   - Used in log messages

4. **Add serialization helpers**:
   - Basic JSON serialization (using serde)
   - Add `std` feature support

5. **Update Cargo.toml**:
   - Add `serde` and `serde_json` dependencies
   - Add `std` feature

## Success Criteria

- `ClientMsg` and `ServerMsg` enums defined
- `LogLevel` enum defined
- All types serialize/deserialize correctly
- Code compiles without warnings
- Basic tests for serialization (optional but recommended)
