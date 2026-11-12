# Phase 7: Update State Extraction for Sync API

## Goal

Update `get_changes()` to extract actual state from runtimes instead of returning empty placeholders. Update `state_ver` tracking when state changes.

## Dependencies

- Phase 6

## Implementation

### 1. Update get_changes() to Extract Texture State

**File**: `lp-engine/src/project/runtime.rs`

```rust
impl ProjectRuntime {
    pub fn get_changes(
        &self,
        since_frame: FrameId,
        detail_specifier: &ApiNodeSpecifier,
    ) -> Result<ProjectResponse, Error> {
        // ... existing code ...
        
        // Collect changes and details
        for (handle, entry) in &self.nodes {
            // ... existing change detection ...
            
            // Add detail if requested
            if detail_handles.contains(handle) {
                let state = match entry.kind {
                    NodeKind::Texture => {
                        // Get actual texture state from runtime
                        if let Some(runtime) = &entry.runtime {
                            // Try to downcast to TextureRuntime
                            // We can't safely downcast Box<dyn NodeRuntime>
                            // Need a method on NodeRuntime to get state, or
                            // store state separately, or use Any trait
                            
                            // Option: Add get_state() method to NodeRuntime trait
                            // Option: Store state in NodeEntry separately
                            // Option: Use Any trait for downcasting
                            
                            // For now, use Any trait:
                            use core::any::Any;
                            if let Some(tex_runtime) = runtime.as_any().downcast_ref::<TextureRuntime>() {
                                tex_runtime.get_state().into()
                            } else {
                                // Fallback to empty state
                                NodeState::Texture(TextureState {
                                    texture_data: Vec::new(),
                                })
                            }
                        } else {
                            NodeState::Texture(TextureState {
                                texture_data: Vec::new(),
                            })
                        }
                    }
                    // ... other kinds ...
                };
                
                // ... rest of detail construction ...
            }
        }
        
        Ok(ProjectResponse::GetChanges { ... })
    }
}
```

Actually, we need to add `as_any()` to `NodeRuntime` trait:

```rust
// nodes/mod.rs
pub trait NodeRuntime: Send + Sync {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;
    fn render(&mut self, ctx: &dyn RenderContext) -> Result<(), Error>;
    fn destroy(&mut self) -> Result<(), Error> { ... }
    
    // For downcasting
    fn as_any(&self) -> &dyn core::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any;
}

// In TextureRuntime:
impl NodeRuntime for TextureRuntime {
    // ... existing methods ...
    
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}
```

### 2. Update state_ver When State Changes

**File**: `lp-engine/src/project/runtime.rs`

```rust
impl ProjectRuntime {
    fn ensure_texture_rendered(&mut self, handle: TextureHandle) -> Result<(), Error> {
        // ... existing code ...
        
        // After (potential) shader rendering, update state_ver
        if let Some(entry) = self.nodes.get_mut(&node_handle) {
            entry.state_ver = self.frame_id;
        }
        
        Ok(())
    }
    
    pub fn render(&mut self) -> Result<(), Error> {
        // ... render fixtures ...
        
        // After fixture renders, output state_ver is updated by RenderContext
        // (when get_output() is called)
        
        Ok(())
    }
}
```

**File**: `lp-engine/src/project/runtime.rs` (RenderContextImpl)

```rust
impl<'a> RenderContext for RenderContextImpl<'a> {
    fn get_output(&mut self, handle: OutputHandle, universe: u32, start_ch: u32, ch_count: u32) -> Result<&mut [u8], Error> {
        // Get output runtime and buffer
        // ... implementation ...
        
        // Update output state_ver to current frame
        let node_handle = handle.as_node_handle();
        if let Some(entry) = self.nodes.get_mut(&node_handle) {
            entry.state_ver = self.frame_id;
        }
        
        // Return buffer slice
        Ok(buffer)
    }
}
```

### 3. Extract State for Other Node Types

**File**: `lp-engine/src/project/runtime.rs`

```rust
// In get_changes(), for other node types:
NodeKind::Shader => {
    // Shader runtime not implemented yet
    NodeState::Shader(ShaderState {
        glsl_code: String::new(),
        error: None,
    })
}
NodeKind::Output => {
    // Output runtime not implemented yet
    NodeState::Output(OutputState {
        channel_data: Vec::new(),
    })
}
NodeKind::Fixture => {
    // Fixture runtime state extraction
    // FixtureState has lamp_colors - we'd need to extract from runtime
    // For now, return empty
    NodeState::Fixture(FixtureState {
        lamp_colors: Vec::new(),
    })
}
```

## Success Criteria

- All code compiles
- `get_changes()` extracts actual texture state from `TextureRuntime`
- `state_ver` is updated when texture is rendered
- `state_ver` is updated when output is accessed
- Tests pass

## Notes

- Need to add `as_any()` methods to `NodeRuntime` trait for downcasting
- State extraction for shader/output/fixture is stubbed (runtimes not fully implemented)
- `state_ver` tracking ensures sync API knows when state has changed
- Texture state extraction copies texture data (may be large - consider optimization later)
