# Phase 7: Implement panel rendering functions

## Goal

Implement panel rendering functions for displaying nodes, textures, shaders, fixtures, and outputs.

## Tasks

1. Update `lp-app/apps/lp-cli/src/debug_ui/panels.rs`:
   - Implement `render_nodes_panel()`:
     - Display "All detail" checkbox
     - Display list of all nodes with checkboxes
     - Show node path, kind, status
     - Update `tracked_nodes` based on checkbox state
     - Update `all_detail` based on checkbox state
   
   - Implement `render_texture_panel()`:
     - Get texture data from view
     - Get width, height, format from state
     - Convert texture data to egui ColorImage
     - Display texture image
     - Display metadata (size, format)
   
   - Implement `render_shader_panel()`:
     - Get GLSL code from state
     - Display GLSL code in monospace text editor
     - Display status and errors
   
   - Implement `render_fixture_panel()`:
     - Get texture data from referenced texture node
     - Get mapping cells from fixture state
     - Display texture with mapping overlay
     - Draw circles/overlays for each mapping cell
   
   - Implement `render_output_panel()`:
     - Get channel data from state
     - Display output config and channel data
   
   - Implement `texture_data_to_color_image()`:
     - Convert texture bytes to egui ColorImage
     - Handle RGB8, RGBA8, R8 formats
     - Return ColorImage for display

2. Update `lp-app/apps/lp-cli/src/debug_ui/ui.rs`:
   - Call panel rendering functions in `update()` method
   - Set up UI layout (side panel for nodes, main panel for details)
   - Handle node selection and detail display

3. Add helper functions as needed:
   - Functions to extract data from `ClientProjectView`
   - Functions to convert between formats

## Success Criteria

- All panel rendering functions exist and work
- Nodes panel shows all nodes with checkboxes
- Texture panel displays images correctly
- Shader panel displays GLSL code
- Fixture panel shows texture with mapping overlay
- Output panel displays channel data
- Code compiles without warnings

## Implementation Notes

- Use egui widgets: `Checkbox`, `Label`, `Image`, `TextEdit`
- Reference old debug_ui.rs for rendering patterns
- Handle missing data gracefully (show "No data" messages)
- Texture conversion should handle all supported formats
