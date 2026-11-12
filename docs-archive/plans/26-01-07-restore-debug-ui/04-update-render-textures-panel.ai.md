# Phase 4: Update render_textures_panel() to use render_texture()

## Goal

Update `render_textures_panel()` to iterate over textures, get configs and data from runtime, and call `render_texture()`.

## Tasks

1. Update `render_textures_panel()` to:
   - Get texture IDs from runtime
   - For each texture ID:
     - Get texture runtime and config
     - Get texture data from `texture_rt.texture().data()`
     - Call `render_texture()` with texture_id, config, and `Some(&texture_data)`
2. Remove placeholder message code

## Success Criteria

- Textures panel displays actual texture images with metadata
- Uses existing `render_texture()` helper function
- Code compiles without errors

