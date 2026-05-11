# Overview: LP Client-Server Sync

## Goal

Create an `lp-client` library that can sync with an `lp-server` instance, enabling:
- Client-server communication for project management and node state synchronization
- Frame-based incremental updates to minimize bandwidth
- Support for multiple transport mechanisms (websockets, serial) with feature gating
- Test infrastructure to verify client-server sync works correctly

## Key Changes

### Architectural Changes
- **Handle-based runtime IDs**: Node HashMaps use `NodeHandle` (i32) instead of path-based IDs
- **Frame-based versioning**: Track frame IDs per node for efficient incremental sync
- **Message protocol**: Request/response with IDs, server-sent messages (logs)
- **Transport abstraction**: Clean separation for websockets, serial, etc.

### Engine Changes
- `ProjectRuntime` tracks handles and frames
- Node runtimes store handle, path, and frame information
- New methods for querying changes and node details

### Server Changes
- Implement `GetChanges` handler
- Build responses from `ProjectRuntime` queries

### Client Changes
- New `lp-client` library with `LpClient` struct
- `RemoteProject` and `RemoteNode` state structures
- Sync logic for incremental updates

## Success Criteria

- `lp-client` can connect to `lp-server` via transport
- Client can load/unload projects
- Client can sync project state incrementally using frame IDs
- Client receives node state updates (textures, shaders, outputs)
- Tests verify sync works correctly with in-memory transport
