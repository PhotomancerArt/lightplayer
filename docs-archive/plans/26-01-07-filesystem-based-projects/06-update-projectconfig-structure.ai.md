# Phase 6: Update ProjectConfig Structure (Remove Nodes Field)

## Goal

Remove the `nodes` field from `ProjectConfig`, keeping only top-level config.

## Tasks

1. Update `ProjectConfig` in `lp-core/src/project/config.rs`:
   - Remove `nodes: Nodes` field
   - Keep only `uid` and `name` fields
   - Remove `Nodes` struct (or keep for reference, but don't use)

2. Update `ProjectRuntime::init()`:
   - Remove nodes parameter (nodes will be loaded from filesystem separately)
   - Or keep signature but nodes will be empty initially

3. Update `LpApp::create_default_project()`:
   - Return `ProjectConfig` with only `uid` and `name`
   - Remove node creation (will be handled by loader)

4. Update all code that accesses `config.nodes`:
   - `LpApp::load_project()` - remove node initialization from config
   - Tests - update to not expect nodes in config

5. Update serialization/deserialization:
   - `ProjectConfig` should serialize/deserialize only top-level fields

## Success Criteria

- `ProjectConfig` contains only `uid` and `name`
- All code updated to not access `config.nodes`
- Code compiles without warnings
- Tests updated (may need to be adjusted for new structure)

