# Phase 5: Update render_shaders_panel() to use render_shader_panel()

## Goal

Update `render_shaders_panel()` to iterate over shaders, get configs and runtimes, and call `render_shader_panel()`.

## Tasks

1. Update `render_shaders_panel()` to:
   - Get shader IDs from runtime
   - For each shader ID:
     - Get shader runtime and config
     - Call `render_shader_panel()` with shader_id, config, and `Some(&shader_runtime)`
2. Remove placeholder message code

## Success Criteria

- Shaders panel displays GLSL code and compilation status
- Uses existing `render_shader_panel()` helper function
- Code compiles without errors

