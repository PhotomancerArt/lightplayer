# Phase 7: lp-engine - Basic Rendering (Minimal)

## Goal

Implement basic frame rendering. `tick()` increments frame ID, `render()` iterates fixtures and calls their `render()` method. Stub actual texture/output work with `todo!()`.

## Dependencies

- `lp-engine` Phase 6

## Implementation

### Update ProjectRuntime

**File**: Update `lp-engine/src/project/runtime.rs`
```rust
impl ProjectRuntime {
    // ... existing methods ...
    
    /// Advance to next frame
    pub fn tick(&mut self) {
        self.frame_id = self.frame_id.next();
    }
    
    /// Render current frame
    pub fn render(&mut self) -> Result<(), Error> {
        // Render all fixtures
        let fixture_handles: Vec<NodeHandle> = self.nodes
            .iter()
            .filter(|(_, entry)| {
                entry.kind == NodeKind::Fixture && 
                entry.runtime.is_some() &&
                matches!(entry.status, NodeStatus::Ok)
            })
            .map(|(handle, _)| *handle)
            .collect();
        
        for handle in fixture_handles {
            if let Some(entry) = self.nodes.get_mut(&handle) {
                if let Some(runtime) = &mut entry.runtime {
                    // Create render context (stub for now)
                    let mut ctx = RenderContextImpl {
                        // todo!("Add texture/output access to context")
                    };
                    
                    // Render fixture
                    if let Err(e) = runtime.render(&mut ctx) {
                        entry.status = NodeStatus::Error(format!("Render error: {}", e));
                    }
                }
            }
        }
        
        // todo!("Flush outputs with state_ver == frame_id")
        
        Ok(())
    }
}

/// Stub render context implementation
struct RenderContextImpl {
    // Will add fields later
}

impl RenderContext for RenderContextImpl {
    // Methods use default todo!() implementations for now
}
```

**File**: Update `lp-engine/src/runtime/contexts.rs` to make `RenderContext` methods have default implementations:

```rust
/// Context for rendering
pub trait RenderContext {
    /// Get texture data (triggers lazy rendering if needed)
    fn get_texture(&mut self, handle: TextureHandle) -> Result<&[u8], Error> {
        todo!("Texture rendering not implemented yet")
    }
    
    /// Get output buffer slice
    fn get_output(&mut self, handle: OutputHandle, universe: u32, start_ch: u32, ch_count: u32) -> Result<&mut [u8], Error> {
        todo!("Output access not implemented yet")
    }
}
```

## Success Criteria

- All code compiles
- Can call `tick()` to advance frame ID
- Can call `render()` to render fixtures
- Fixtures with errors get `Error` status
- No crashes even if texture/output access is stubbed

## Tests

- Create project with fixture node
- Initialize nodes
- Call `tick()` and verify frame_id increments
- Call `render()` and verify no crashes
- Verify fixture status remains `Ok` (or changes to `Error` if render fails)

## Notes

- Texture lazy rendering is stubbed - will be implemented later
- Output flushing is stubbed - will be implemented later
- Render context is minimal - will be expanded as we add texture/output access
