# Plan Notes: Dev UI for lp-cli Client

## Questions

### Q1: Where should the debug UI module live?

**Context:** The user wants the debug UI in its own module in lp-cli, and notes it's temporary.

**Suggested Answer:** Create `lp-app/apps/lp-cli/src/debug_ui/` as a module directory. This keeps it separate and makes it easy to remove later. The module should be gated behind a feature flag or just kept as a separate module that can be easily removed.

**Files:**
- `lp-app/apps/lp-cli/src/debug_ui/mod.rs` - Module entry point
- `lp-app/apps/lp-cli/src/debug_ui/ui.rs` - Main UI rendering logic
- `lp-app/apps/lp-cli/src/debug_ui/panels.rs` - Panel rendering functions (textures, shaders, fixtures, nodes)

**Answer:** Confirmed - use `lp-app/apps/lp-cli/src/debug_ui/` directory structure.

### Q2: How should we integrate with AsyncLpClient?

**Context:** The dev command uses `AsyncLpClient` which wraps `LpClient`. We need to sync project state and get node details.

**Suggested Answer:** 
- Add a `ClientProjectView` instance to track project state (from `lp-engine-client`)
- Add async methods to `AsyncLpClient` for syncing project state:
  - `project_sync(handle: ProjectHandle, view: &mut ClientProjectView)` - Syncs view with server
- The UI will periodically call sync to update state
- Use `project_get_changes` with the view's `detail_specifier()` to request detail for tracked nodes

**Considerations:**
- Need to store `ClientProjectView` somewhere accessible to the UI
- Sync frequency: every frame or on a timer?
- How to handle the project handle - store it in the UI state?

**Answer:** 
- UI should sync as soon as it gets a response (not on a timer)
- It's okay if it lags behind a bit
- Don't want more than one GetChanges request in flight at a time
- After a sync completes, immediately start the next sync
- Use a flag/state to track if a sync is in progress
- The UI reads from the view whenever it renders (at egui's render rate)

### Q3: What UI framework should we use?

**Context:** The old debug UI uses `egui` with `eframe`. The lp-cli is currently a CLI tool.

**Suggested Answer:** Use `egui` with `eframe` for the debug UI. This matches the old implementation and provides a good GUI framework. We'll need to:
- Add `egui` and `eframe` dependencies to `lp-cli/Cargo.toml`
- Create a windowed app that runs alongside or instead of the CLI loop
- The dev command could optionally spawn the UI window, or we could have a separate `lp-cli dev --ui` flag

**Alternative:** Could use a TUI library like `ratatui` but that would be a different approach from the old debug UI.

**Answer:** Use `egui` with `eframe`. For now, always open the UI. We can add `--headless` flag later if needed.

### Q4: How should node detail tracking work?

**Context:** Each node should have a checkbox to show detail, and there should be an "all detail" checkbox.

**Suggested Answer:**
- Store a set of `NodeHandle` values for nodes we want to track detail for
- When a node checkbox is checked, add its handle to the detail tracking set
- When "all detail" is checked, add all node handles to the tracking set
- When syncing, use `ClientProjectView::detail_specifier()` to generate the `ApiNodeSpecifier` for the sync request
- The view will automatically update with state for tracked nodes

**Implementation:**
- UI state: `BTreeSet<NodeHandle>` for tracked nodes
- "All detail" checkbox: when checked, track all nodes; when unchecked, clear all tracking
- Individual checkboxes: toggle individual nodes

**Answer:** Confirmed - use `BTreeSet<NodeHandle>` for tracking, with "all detail" checkbox and individual node checkboxes.

### Q5: What data do we need to display?

**Context:** The old debug UI shows textures, shaders, fixtures with their configs and states.

**Suggested Answer:** Display:
- **Node list:** All nodes with checkboxes, showing path, kind, status
- **Node detail panels:** When a node is tracked, show:
  - **Texture nodes:** Texture image, size, format, texture data
  - **Shader nodes:** GLSL code, status, errors
  - **Fixture nodes:** Mapping overlay on texture, fixture config
  - **Output nodes:** Output config, channel data

**Data sources:**
- Node list: `ClientProjectView::nodes` - gives us path, kind, status
- Node config: `ClientNodeEntry::config` - but this is a trait object, need to downcast or match on kind
- Node state: `ClientNodeEntry::state` - only available if tracked
- For texture data: `ClientProjectView::get_texture_data(handle)`
- For output data: `ClientProjectView::get_output_data(handle)`

**Answer:** Confirmed - display the listed data. We may need to add extra things to the detail to get things like fixture data. Plan to add what we need as we discover gaps.

### Q6: How do we access node configs?

**Context:** Node configs are stored as `Box<dyn NodeConfig>` which can't be easily serialized or displayed.

**Suggested Answer:** 
- Match on `NodeKind` to determine what type of config we have
- Downcast the config trait object to concrete types:
  - `NodeKind::Texture` -> `TextureConfig`
  - `NodeKind::Shader` -> `ShaderConfig`  
  - `NodeKind::Output` -> `OutputConfig`
  - `NodeKind::Fixture` -> `FixtureConfig`
- For display, we can serialize to JSON or format directly
- Note: The `ClientProjectView` already does this pattern in `apply_changes()` when creating configs

**Alternative:** We could request configs from the server via filesystem reads, but runtime state should be sufficient for debugging.

**Answer:** We don't need to show config for every node. We might not need it at all (ideal). Hopefully everything comes from the project view and state. Focus on displaying state data rather than configs.

### Q7: How should the UI be triggered?

**Context:** The dev command currently runs a client loop that waits for Ctrl+C.

**Suggested Answer:**
- Add a `--ui` flag to the dev command
- When `--ui` is present, spawn an egui window
- The window runs in a separate thread or async task
- The client loop continues to run and sync project state
- The UI reads from the `ClientProjectView` which is updated by the sync loop

**Alternative:** Always show UI when in dev mode, or have it be the default.

**Answer:** Default to show UI, `--headless` flag to not show it. The UI can probably run in the client thread for simplicity (that's the main thing it does).

### Q8: How should we handle the sync loop?

**Context:** We need to periodically sync project state from the server.

**Suggested Answer:**
- In the dev command handler, after loading the project:
  - Create a `ClientProjectView` instance
  - Spawn an async task that periodically calls `project_sync()`
  - Sync frequency: every 100ms or so (10 FPS for state updates)
  - The UI reads from the view (which should be behind a mutex/Arc for thread safety)

**Implementation:**
- Store `Arc<Mutex<ClientProjectView>>` so both sync loop and UI can access it
- Sync loop: `tokio::spawn(async move { ... })` that loops and syncs
- UI: Reads from the Arc<Mutex<ClientProjectView>>

**Answer:** The sync loop can be part of the UI's update cycle for now. In egui's update callback, check if a sync is in progress, and if not, start a new sync. This keeps things simple since the UI runs in the client thread.

### Q9: What about shader GLSL code?

**Context:** The old UI shows shader GLSL code. We need to get this from config or filesystem.

**Suggested Answer:**
- Shader config has `glsl_path` field pointing to the GLSL file
- We can read the GLSL file from the server filesystem using `AsyncLpClient::fs_read()`
- Cache the GLSL content in UI state keyed by node handle
- Update when shader config changes

**Alternative:** Could include GLSL code in node state, but that's not currently part of the state model.

**Answer:** The shader node should send GLSL code as part of node detail. We'll need to extend the node detail/state to include GLSL code for shader nodes.

### Q10: How should we display texture data?

**Context:** The old UI renders texture images using egui's Image widget.

**Suggested Answer:**
- Use `ClientProjectView::get_texture_data(handle)` to get texture bytes
- Convert texture bytes to egui `ColorImage` based on format (RGB8, RGBA8, R8)
- Create egui `TextureHandle` and display with `Image` widget
- Similar to old implementation in `debug_ui.rs::texture_data_to_color_image()`

**Note:** Need to handle different texture formats and convert to ColorImage format.

**Answer:** Confirmed - use `ClientProjectView::get_texture_data()` and convert to egui `ColorImage` for display.

## Engine and State Extensions Needed

### TextureState Extensions

**Current:** `TextureState { texture_data: Vec<u8> }`

**Needed:** Add width, height, format to display textures properly
- `width: u32` - Texture width
- `height: u32` - Texture height  
- `format: String` - Texture format (RGB8, RGBA8, R8)

**Location:** `lp-model/src/nodes/texture/state.rs`

**Engine changes:** Update texture state extraction in `lp-engine/src/project/runtime.rs` to include width, height, format from runtime texture.

### ShaderState

**Current:** `ShaderState { glsl_code: String, error: Option<String> }`

**Status:** Already has `glsl_code` - no changes needed ✓

### FixtureState Extensions

**Current:** `FixtureState { lamp_colors: Vec<u8> }`

**Needed:** To display fixture mapping overlay, we need post-transform mapping data:
- List of mapping cells/shapes/areas that represent sampling regions
- Post-transform (after transform matrix is applied)
- Each cell should have information needed to draw overlay:
  - `channel: u32` - Output channel
  - `center: [f32; 2]` - Center in texture space (normalized [0,1] or pixel coordinates)
  - `radius: f32` - Sampling radius
  - Possibly shape type (circle, etc.) if we support different shapes

**Structure:**
```rust
pub struct MappingCell {
    pub channel: u32,
    pub center: [f32; 2],  // Post-transform, in texture space
    pub radius: f32,
    // Shape type if we support different shapes later
}

pub struct FixtureState {
    pub lamp_colors: Vec<u8>,
    pub mapping_cells: Vec<MappingCell>,  // NEW: Post-transform mapping
}
```

**Engine changes:** 
- Extract mapping from `FixtureRuntime` after transform is applied
- Convert from fixture space to texture space coordinates
- Include in `FixtureState` when creating node state

### OutputState

**Current:** `OutputState { channel_data: Vec<u8> }`

**Status:** Sufficient for display ✓
