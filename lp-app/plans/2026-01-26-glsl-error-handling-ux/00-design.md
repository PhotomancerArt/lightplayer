# Design: GLSL Error Handling UX Improvements

## Overview

Improve the UX around GLSL error handling in `lp-cli dev`:
- Don't fail project startup on GLSL compilation errors
- Don't ignore subsequent file changes when there's a compilation error
- Display GLSL errors prominently in the debug UI
- Show visual status indicators (red/green/yellow circles) for node status
- Log status changes to console for CLI usage

## File Structure

```
lp-engine/src/
├── nodes/
│   └── shader/
│       └── runtime.rs                    # MODIFY: init() and handle_fs_change() don't fail on compilation errors
└── project/
    └── runtime.rs                        # MODIFY: Update status based on compilation results, ensure_all_nodes_initialized() doesn't fail on Error

lp-server/src/
└── project_manager.rs                    # MODIFY: Remove ensure_all_nodes_initialized() check (or make it not fail on Error)

lp-engine-client/src/
└── project/
    └── view.rs                           # MODIFY: Track previous status and log changes

lp-cli/src/
├── debug_ui/
│   ├── panels.rs                         # MODIFY: Add status indicator circles next to node names
│   └── nodes/
│       └── shader.rs                     # MODIFY: Ensure error display is prominent
```

## Types and Functions

### ShaderRuntime Changes

```rust
// nodes/shader/runtime.rs

impl NodeRuntime for ShaderRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // MODIFY: Don't return error on compilation failure
        // - Load GLSL source (can still fail with IO error)
        // - Try to compile
        // - If compilation fails, store error but return Ok()
        // - Caller will check compilation_error and update status
    }

    fn handle_fs_change(
        &mut self,
        change: &FsChange,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        // MODIFY: Don't return error on compilation failure
        // - Try to reload and recompile
        // - If compilation fails, store error but return Ok()
        // - Caller will check compilation_error and update status
    }
}

impl ShaderRuntime {
    // NEW: Helper to check if shader has compilation error
    pub fn has_compilation_error(&self) -> bool {
        self.compilation_error.is_some()
    }

    // NEW: Get compilation error message
    pub fn compilation_error(&self) -> Option<&str> {
        self.compilation_error.as_deref()
    }
}
```

### ProjectRuntime Changes

```rust
// project/runtime.rs

impl ProjectRuntime {
    pub fn init_nodes(&mut self) -> Result<(), Error> {
        // MODIFY: After calling runtime.init(), check if shader has compilation error
        // - If shader runtime and has compilation error, set status to Error
        // - Otherwise, set status to Ok (or InitError if init actually failed)
    }

    pub fn handle_fs_changes(&mut self, changes: &[FsChange]) -> Result<(), Error> {
        // MODIFY: After calling runtime.handle_fs_change(), check if shader has compilation error
        // - If shader runtime and has compilation error, update status to Error
        // - If shader runtime and no compilation error, update status to Ok
        // - Generate StatusChanged change if status changed
    }

    pub fn ensure_all_nodes_initialized(&self) -> Result<(), Error> {
        // MODIFY: Don't fail on Error status - only fail on InitError or Created
        // - Error status means node initialized but has runtime error (e.g., GLSL compilation)
        // - This is acceptable - project can run with nodes in error state
    }

    pub fn get_changes(
        &mut self,
        since_frame: Option<FrameId>,
        detail_specifier: ApiNodeSpecifier,
    ) -> ProjectResponse {
        // MODIFY: Before extracting node details, ensure status is current
        // - For shader nodes, check compilation_error in runtime
        // - If status doesn't match compilation_error state, update status and generate StatusChanged
    }
}
```

### ClientProjectView Changes

```rust
// lp-engine-client/src/project/view.rs

impl ClientProjectView {
    pub fn apply_changes(
        &mut self,
        response: &lp_model::project::api::ProjectResponse,
    ) -> Result<(), String> {
        // MODIFY: Track previous status for each node
        // - When applying StatusChanged change, compare with previous status
        // - If transition from Ok -> Error or Error -> Ok, log to console
        // - Format: "[{path}] Status changed: {old} -> {new}"
        // - Include error message if transitioning to Error
    }
}
```

### UI Changes

```rust
// lp-cli/src/debug_ui/panels.rs

pub fn render_all_nodes_panel(...) -> bool {
    // MODIFY: Add status indicator circle next to node name/checkbox
    // - Green circle for Ok
    // - Red circle for Error or InitError
    // - Yellow circle for Warn
    // - Gray circle for Created
    // - Use egui::painter to draw circle, or egui::widgets::Label with colored text
}

// lp-cli/src/debug_ui/nodes/shader.rs

pub fn render_shader_panel(...) {
    // MODIFY: Ensure error display is prominent
    // - Already shows error in red, but could make it more visible
    // - Maybe add a warning icon or make the error text larger/bolder
}
```

## Implementation Notes

### Status Update Logic

1. **In `init_nodes()`**: After calling `runtime.init()`, check if it's a shader runtime and if it has a compilation error. Update status accordingly.

2. **In `handle_fs_changes()`**: After calling `runtime.handle_fs_change()`, check if it's a shader runtime and update status based on compilation error state. Generate `StatusChanged` change if status changed.

3. **In `get_changes()`**: Before extracting node details, sync status with runtime state. This ensures status is always current when clients request it.

### Status Change Detection

In `ClientProjectView::apply_changes()`, maintain a map of previous statuses. When applying `StatusChanged`, compare with previous status and log transitions.

### Visual Indicators

Use `egui::painter` to draw circles, or use `egui::widgets::Label` with colored text. The circle should be small (8-10 pixels) and positioned next to the node name.

## Success Criteria

- Projects start successfully even if shader nodes have GLSL compilation errors
- File changes are processed even when there's a compilation error
- GLSL errors are prominently displayed in the debug UI
- Status indicators (colored circles) are visible next to each node name
- Status changes are logged to console with clear messages
- All code compiles without errors
- All tests pass
