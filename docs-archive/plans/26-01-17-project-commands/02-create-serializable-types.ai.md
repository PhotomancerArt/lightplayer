# Phase 2: Create SerializableNodeDetail and SerializableProjectResponse

## Goal

Create serializable wrapper types for `NodeDetail` and `ProjectResponse` to enable serialization of project responses over the wire.

## Tasks

1. Update `lp-model/src/project/api.rs`:
   - Create `SerializableNodeDetail` enum with variants for each node kind:
     - `Texture { path, config, state, status }`
     - `Shader { path, config, state, status }`
     - `Output { path, config, state, status }`
     - `Fixture { path, config, state, status }`
   - Add `Serialize` and `Deserialize` derives
   - Create `SerializableProjectResponse` enum:
     - `GetChanges { current_frame, node_handles, node_changes, node_details }`
   - Add conversion functions:
     - `impl NodeDetail { pub fn to_serializable(&self) -> Result<SerializableNodeDetail, Error> }`
     - `impl ProjectResponse { pub fn to_serializable(&self) -> Result<SerializableProjectResponse, Error> }`

2. Update `lp-model/src/server/api.rs`:
   - Enable `ProjectRequest` variant in `ServerResponse`:
     - `ProjectRequest { response: SerializableProjectResponse }`
   - Remove TODO comment

3. Add tests:
   - Test conversion from `NodeDetail` to `SerializableNodeDetail` for each node kind
   - Test conversion from `ProjectResponse` to `SerializableProjectResponse`
   - Test serialization/deserialization round-trip

## Success Criteria

- [ ] `SerializableNodeDetail` enum exists with all node kind variants
- [ ] `SerializableProjectResponse` enum exists
- [ ] Conversion functions work correctly (downcast `Box<dyn NodeConfig>` to concrete types)
- [ ] `ServerResponse::ProjectRequest` variant is enabled
- [ ] All tests pass
- [ ] Code compiles without warnings
