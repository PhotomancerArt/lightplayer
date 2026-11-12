# Design: Dev UI for lp-cli Client

## Overview

Add a debug UI to the lp-cli dev command that displays project node state, similar to the old debug app. The UI will:

- Display all nodes with checkboxes for detail tracking
- Show "all detail" checkbox to track all nodes
- Render node-specific detail panels (textures, shaders, fixtures, outputs)
- Sync project state from the server using `LpClient`/`AsyncLpClient`
- Use egui/eframe for the UI framework

This enables debugging project state from the client perspective, demonstrating that the client-server sync works correctly.

## File Structure

```
lp-app/apps/lp-cli/src/
├── commands/
│   └── dev/
│       ├── args.rs                    # MODIFY: Add --headless flag
│       ├── handler.rs                 # MODIFY: Spawn UI when not headless
│       ├── async_client.rs           # MODIFY: Add project_sync method
│       └── ...
└── debug_ui/                          # NEW: Debug UI module
    ├── mod.rs                         # NEW: Module entry point
    ├── ui.rs                          # NEW: Main UI app state and egui App impl
    └── panels.rs                      # NEW: Panel rendering functions

lp-app/crates/lp-model/src/nodes/
├── texture/
│   └── state.rs                       # MODIFY: Add width, height, format to TextureState
└── fixture/
    └── state.rs                       # MODIFY: Add MappingCell and mapping_cells to FixtureState

lp-app/crates/lp-engine/src/project/
└── runtime.rs                         # MODIFY: Extract width, height, format for TextureState, mapping_cells for FixtureState
```

## Type Tree

### lp-app/apps/lp-cli/src/commands/dev/args.rs

- `pub struct DevArgs` - **MODIFY**: Add field:
  ```rust
  pub struct DevArgs {
      // ... existing fields ...
      pub headless: bool,  // NEW: Flag to disable UI
  }
  ```

### lp-app/apps/lp-cli/src/commands/dev/async_client.rs

- `pub struct AsyncLpClient` - **MODIFY**: Add method:
  ```rust
  impl AsyncLpClient {
      // ... existing methods ...
      
      /// Sync project view with server
      ///
      /// Sends GetChanges request and updates the ClientProjectView.
      /// Returns when sync completes or timeout occurs.
      pub async fn project_sync(
          &mut self,
          handle: ProjectHandle,
          view: &mut ClientProjectView,
      ) -> Result<()> {
          // Implementation: call project_get_changes, wait for response,
          // call view.apply_changes() with response
      }
  }
  ```

### lp-app/apps/lp-cli/src/debug_ui/ui.rs

- `pub struct DebugUiState` - **NEW**: UI application state:
  ```rust
  pub struct DebugUiState {
      project_view: Arc<Mutex<ClientProjectView>>,
      project_handle: ProjectHandle,
      async_client: AsyncLpClient,
      tracked_nodes: BTreeSet<NodeHandle>,
      all_detail: bool,
      sync_in_progress: bool,
      glsl_cache: BTreeMap<NodeHandle, String>,
  }
  ```

- `impl eframe::App for DebugUiState` - **NEW**: egui App implementation:
  ```rust
  impl eframe::App for DebugUiState {
      fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
          // Handle sync (if not in progress, start new sync)
          // Render UI panels
      }
  }
  ```

### lp-app/apps/lp-cli/src/debug_ui/panels.rs

- `pub fn render_nodes_panel()` - **NEW**: Render node list with checkboxes
- `pub fn render_texture_panel()` - **NEW**: Render texture node detail
- `pub fn render_shader_panel()` - **NEW**: Render shader node detail  
- `pub fn render_fixture_panel()` - **NEW**: Render fixture node detail
- `pub fn render_output_panel()` - **NEW**: Render output node detail
- `pub fn texture_data_to_color_image()` - **NEW**: Convert texture bytes to egui ColorImage

### lp-app/crates/lp-model/src/nodes/texture/state.rs

- `pub struct TextureState` - **MODIFY**: Add fields:
  ```rust
  pub struct TextureState {
      pub texture_data: Vec<u8>,  // Existing
      pub width: u32,              // NEW: Texture width
      pub height: u32,             // NEW: Texture height
      pub format: String,          // NEW: Texture format (RGB8, RGBA8, R8)
  }
  ```

### lp-app/crates/lp-model/src/nodes/fixture/state.rs

- `pub struct MappingCell` - **NEW**: Mapping cell structure:
  ```rust
  pub struct MappingCell {
      pub channel: u32,
      pub center: [f32; 2],  // Post-transform, in texture space [0,1]
      pub radius: f32,
  }
  ```

- `pub struct FixtureState` - **MODIFY**: Add field:
  ```rust
  pub struct FixtureState {
      pub lamp_colors: Vec<u8>,           // Existing
      pub mapping_cells: Vec<MappingCell>, // NEW: Post-transform mapping
  }
  ```

### lp-app/crates/lp-engine/src/project/runtime.rs

- `fn get_changes()` - **MODIFY**: Update state extraction:
  - TextureState: Extract width, height, format from runtime texture
  - FixtureState: Extract mapping_cells after applying transform (convert from fixture space to texture space)

## Process Flow

### UI Sync Flow

```
DebugUiState::update()
    |
    +-- Check sync_in_progress
    |   |
    |   +-- If false: Start sync
    |       |
    |       +-- async_client.project_sync(handle, view)
    |           |
    |           +-- project_get_changes(since_frame, detail_specifier)
    |           |
    |           +-- Wait for response
    |           |
    |           +-- view.apply_changes(response)
    |           |
    |           +-- Set sync_in_progress = false
    |
    +-- Render UI panels
        |
        +-- Read from project_view (Arc<Mutex<...>>)
        |
        +-- Display nodes, checkboxes, detail panels
```

### Node Detail Tracking Flow

```
User clicks checkbox
    |
    +-- If "all detail" checkbox:
    |   |
    |   +-- If checked: Add all node handles to tracked_nodes
    |   |
    |   +-- If unchecked: Clear tracked_nodes
    |
    +-- If individual node checkbox:
        |
        +-- Toggle handle in tracked_nodes
        |
        +-- Update view.detail_tracking
            |
            +-- Next sync uses updated detail_specifier()
```

## Design Decisions

### 1. UI Framework
**Decision**: Use `egui` with `eframe` to match the old debug UI implementation.
- Consistent with existing debug UI
- Good GUI framework for Rust
- Easy to integrate with async code

### 2. Sync Strategy
**Decision**: Sync as soon as previous sync completes, no more than one GetChanges in flight at a time.
- Prevents request queue buildup
- Simple to implement
- Acceptable latency for debug UI

### 3. State Storage
**Decision**: Use `Arc<Mutex<ClientProjectView>>` for shared access between sync and UI.
- Allows both sync loop and UI to access view
- Thread-safe access
- UI reads from view whenever it renders

### 4. Detail Tracking
**Decision**: Store tracked nodes in UI state (`BTreeSet<NodeHandle>`), sync with `ClientProjectView::detail_tracking`.
- UI controls which nodes to track
- View manages detail specifier generation
- "All detail" checkbox controls all nodes at once

### 5. Texture State Extensions
**Decision**: Add width, height, format to `TextureState` for display.
- Needed to convert texture data to egui ColorImage
- Format needed to handle different pixel layouts
- Can be extracted from runtime texture

### 6. Fixture Mapping Data
**Decision**: Add post-transform mapping cells to `FixtureState`.
- Needed to draw mapping overlay on texture
- Post-transform so UI shows actual sampling regions
- List of cells/shapes for each channel

### 7. Shader GLSL Code
**Decision**: Include GLSL code in `ShaderState` (already present).
- Already in state, no changes needed
- Can be displayed directly from state

## Implementation Notes

### Texture Format Handling

When converting texture data to egui `ColorImage`:
- RGB8: 3 bytes per pixel, convert to RGBA (alpha = 255)
- RGBA8: 4 bytes per pixel, use directly
- R8: 1 byte per pixel, convert to grayscale RGBA

### Mapping Transform

For fixture mapping:
1. Get mapping points from runtime (in fixture space)
2. Apply transform matrix to convert to texture space
3. Normalize coordinates to [0, 1] range
4. Store in `MappingCell` with channel, center, radius

### Sync Loop Integration

The sync loop runs in the UI's update cycle:
- Check `sync_in_progress` flag
- If false and view needs update, start async sync
- Set flag to true, spawn async task
- When sync completes, set flag to false
- UI reads from view on each frame

### Node Detail Display

For each node type:
- **Texture**: Show image, size, format, texture data
- **Shader**: Show GLSL code, status, errors (from state)
- **Fixture**: Show texture with mapping overlay, fixture config (if needed)
- **Output**: Show output config, channel data

## Error Handling

- Sync errors: Log and continue, UI shows last known state
- Missing node data: Show "No data available" message
- Texture conversion errors: Show error message, skip image display
- Transport errors: Log and exit UI

## Testing Strategy

### Manual Testing

1. Start dev command with UI
2. Verify nodes appear in list
3. Check individual node checkboxes
4. Verify detail panels show when tracked
5. Check "all detail" checkbox
6. Verify sync happens automatically

### Integration Tests

- Test `AsyncLpClient::project_sync()` method
- Test `ClientProjectView` detail tracking
- Test texture state extraction with width/height/format
- Test fixture state extraction with mapping cells

## Success Criteria

- [ ] UI displays all nodes with checkboxes
- [ ] Individual node checkboxes toggle detail tracking
- [ ] "All detail" checkbox controls all nodes
- [ ] Texture nodes display image with correct format
- [ ] Shader nodes display GLSL code from state
- [ ] Fixture nodes display texture with mapping overlay
- [ ] Output nodes display channel data
- [ ] Sync happens automatically when not in progress
- [ ] No more than one GetChanges request in flight
- [ ] Code compiles without warnings
- [ ] All tests pass

## Notes

- UI is temporary and will be removed later
- Default behavior: show UI (can disable with `--headless`)
- UI runs in client thread for simplicity
- May need to extend state further as we discover gaps
- Mapping cell structure may evolve as we support more shape types
- Texture format may need to be enum instead of String in the future
