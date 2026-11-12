# Questions: Restore Debug UI

## Context

After the filesystem-based projects refactor, the debug UI was partially updated but is incomplete:
- Textures panel still shows placeholder message
- Shaders panel only shows IDs, not code/status
- Fixtures panel only shows IDs, not full details

The issue is that node configs (TextureNode, ShaderNode, FixtureNode) are no longer stored in ProjectConfig, and we're not keeping them in the runtimes. The debug UI needs access to these configs to display properly.

## Questions

1. **Where should node configs be stored?**
   - ✅ **ANSWERED**: Store configs in node runtimes (e.g., `TextureNodeRuntime` contains `config: TextureNode`)
   - Configs are immutable - we reload the whole node when config changes
   - This keeps configs with their runtimes and avoids reloading from disk

2. **What information should be displayed for each node type?**
   - ✅ **ANSWERED**: Use the original helper functions - they were good
   - Textures: ID, size, format, actual texture image/data (via `render_texture()`)
   - Shaders: ID, GLSL code, texture_id, compilation status/errors (via `render_shader_panel()`)
   - Fixtures: ID, output_id, texture_id, channel_order, mappings, texture preview with overlay (via `render_fixture()`)
   - Panel functions should iterate over nodes and call these helper functions

4. **How should we handle texture data display?**
   - ✅ **ANSWERED**: Always pass actual texture data from runtime
   - Get texture data via `texture_rt.texture().data()` and pass as `Some(&texture_data)` to `render_texture()`
   - This shows the actual rendered texture content

5. **For shaders, should we reload GLSL from filesystem or store it in the runtime?**
   - ✅ **ANSWERED**: Runtime should keep a copy of the config
   - Store `config: ShaderNode` in `ShaderNodeRuntime` (GLSL code is already in the config)
   - No need to reload from filesystem - use the config stored in runtime

