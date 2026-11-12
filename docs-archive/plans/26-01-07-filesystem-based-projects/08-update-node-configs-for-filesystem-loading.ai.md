# Phase 8: Update Node Configs for Filesystem Loading (Shader GLSL Files)

## Goal

Update node loading to read from filesystem, especially handling shader GLSL files.

## Tasks

1. Update `ProjectLoader` in `lp-core/src/project/loader.rs`:
   - `load_node(fs: &dyn Filesystem, node_path: &str) -> Result<(String, NodeConfig), Error>`:
     - Read `node_path/node.json`
     - Parse node config based on directory suffix (`.shader`, `.texture`, etc.)
     - For shaders: read `main.glsl` and inject into config
     - Return node ID (full path) and config
   - `load_all_nodes(fs: &dyn Filesystem) -> Result<HashMap<String, NodeConfig>, Error>`:
     - Discover all nodes
     - Load each node
     - Return map of node ID to config

2. Update shader node config:
   - `ShaderNode::Single` should have `glsl: String` field
   - When loading from filesystem, read `main.glsl` and populate this field
   - When saving to filesystem, write `glsl` to `main.glsl` and remove from `node.json`

3. Update `LpApp::load_project()`:
   - Use `ProjectLoader::load_all_nodes()` to load nodes
   - Initialize runtime with loaded nodes

4. Add validation:
   - Validate directory suffix matches node type from `node.json`
   - Validate node IDs are valid paths

5. Add tests:
   - Test loading shader node with `main.glsl`
   - Test loading other node types
   - Test nested directories

## Success Criteria

- Nodes can be loaded from filesystem
- Shader GLSL files are read from `main.glsl`
- Node configs are correctly parsed
- Validation works
- Tests pass
- Code compiles without warnings

