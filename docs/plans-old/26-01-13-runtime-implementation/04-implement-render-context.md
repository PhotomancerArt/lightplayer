# Phase 4: Implement Render Context and Lazy Texture Rendering

## Goal

Implement `RenderContextImpl` with texture and output access, including lazy texture rendering that triggers shaders when textures are accessed.

## Dependencies

- Phase 3

## Implementation

### 1. Implement RenderContextImpl

**File**: `lp-engine/src/project/runtime.rs`

```rust
pub struct RenderContextImpl<'a> {
    runtime: &'a mut ProjectRuntime,
    frame_id: FrameId,
}

impl<'a> RenderContext for RenderContextImpl<'a> {
    fn get_texture(&mut self, handle: TextureHandle) -> Result<&Texture, Error> {
        // Ensure texture is rendered (lazy rendering)
        self.runtime.ensure_texture_rendered(handle)?;
        
        // Get texture runtime
        let node_handle = handle.as_node_handle();
        let entry = self.runtime.nodes.get_mut(&node_handle)
            .ok_or_else(|| Error::NotFound {
                path: format!("texture-{}", node_handle.as_i32()),
            })?;
        
        // Get texture from runtime
        if let Some(tex_runtime) = entry.runtime.as_mut()
            .and_then(|r| r.as_any_mut().downcast_mut::<TextureRuntime>()) {
            tex_runtime.texture()
                .ok_or_else(|| Error::Other {
                    message: "Texture not initialized".to_string(),
                })
        } else {
            Err(Error::Other {
                message: "Texture runtime not found".to_string(),
            })
        }
    }
    
    fn get_output(&mut self, handle: OutputHandle, universe: u32, start_ch: u32, ch_count: u32) -> Result<&mut [u8], Error> {
        // Get output runtime
        let node_handle = handle.as_node_handle();
        let entry = self.runtime.nodes.get_mut(&node_handle)
            .ok_or_else(|| Error::NotFound {
                path: format!("output-{}", node_handle.as_i32()),
            })?;
        
        // Get output buffer from runtime
        // Note: OutputRuntime not implemented yet, so this will be a todo!()
        todo!("Get output buffer from OutputRuntime")
    }
}
```

### 2. Implement ensure_texture_rendered()

**File**: `lp-engine/src/project/runtime.rs`

```rust
impl ProjectRuntime {
    /// Ensure texture is rendered for current frame (lazy rendering)
    fn ensure_texture_rendered(&mut self, handle: TextureHandle) -> Result<(), Error> {
        let node_handle = handle.as_node_handle();
        let entry = self.nodes.get_mut(&node_handle)
            .ok_or_else(|| Error::NotFound {
                path: format!("texture-{}", node_handle.as_i32()),
            })?;
        
        // Check if texture needs rendering
        if entry.state_ver >= self.frame_id {
            // Already rendered this frame
            return Ok(());
        }
        
        // Find shaders targeting this texture
        let texture_path = &entry.path;
        let mut shader_handles = Vec::new();
        
        for (shader_handle, shader_entry) in &self.nodes {
            if shader_entry.kind != NodeKind::Shader {
                continue;
            }
            
            // Check if shader targets this texture
            // Need to get ShaderConfig and check texture_spec
            // For now, assume we can check this somehow
            // todo!("Check shader config texture_spec matches texture_path")
            
            // Get shader config
            if let Some(shader_config) = shader_entry.config.as_any().downcast_ref::<ShaderConfig>() {
                if shader_config.texture_spec.as_str() == texture_path.as_str() {
                    shader_handles.push(*shader_handle);
                }
            }
        }
        
        // Sort shaders by render_order
        shader_handles.sort_by_key(|h| {
            let entry = &self.nodes[h];
            // Extract render_order from config
            // todo!("Extract render_order from ShaderConfig")
            0  // Placeholder
        });
        
        // Run shaders
        for shader_handle in shader_handles {
            if let Some(shader_entry) = self.nodes.get_mut(&shader_handle) {
                if let Some(shader_runtime) = &mut shader_entry.runtime {
                    // Create render context for shader
                    let mut ctx = RenderContextImpl {
                        runtime: self,  // Can't do this - self is already borrowed
                        frame_id: self.frame_id,
                    };
                    
                    // This won't work - we need a different approach
                    // Shader rendering will be implemented later
                    todo!("Run shader render() - requires shader runtime implementation")
                }
            }
        }
        
        // Update texture state_ver
        if let Some(entry) = self.nodes.get_mut(&node_handle) {
            entry.state_ver = self.frame_id;
        }
        
        Ok(())
    }
}
```

The issue is that we can't mutably borrow `self` while iterating. We need a different approach. Let's collect shader handles first, then render:

```rust
impl ProjectRuntime {
    fn ensure_texture_rendered(&mut self, handle: TextureHandle) -> Result<(), Error> {
        let node_handle = handle.as_node_handle();
        
        // Check if already rendered
        if let Some(entry) = self.nodes.get(&node_handle) {
            if entry.state_ver >= self.frame_id {
                return Ok(());
            }
        }
        
        // Find shaders targeting this texture
        let texture_path = {
            let entry = self.nodes.get(&node_handle)
                .ok_or_else(|| Error::NotFound {
                    path: format!("texture-{}", node_handle.as_i32()),
                })?;
            entry.path.clone()
        };
        
        let mut shader_handles = Vec::new();
        for (shader_handle, shader_entry) in &self.nodes {
            if shader_entry.kind == NodeKind::Shader {
                // Check if shader targets this texture
                // For now, skip shader rendering (will implement in later phase)
                // Just mark texture as rendered
            }
        }
        
        // For now, just update state_ver (shader rendering comes later)
        if let Some(entry) = self.nodes.get_mut(&node_handle) {
            entry.state_ver = self.frame_id;
        }
        
        Ok(())
    }
}
```

### 3. Update ProjectRuntime::render()

**File**: `lp-engine/src/project/runtime.rs`

```rust
impl ProjectRuntime {
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
                    // Create render context
                    let mut ctx = RenderContextImpl {
                        runtime: self,  // Can't do this - self is already borrowed
                        frame_id: self.frame_id,
                    };
                    
                    // This won't work due to borrowing rules
                    // We need to restructure this
                    todo!("Fix borrowing issue in render()")
                }
            }
        }
        
        Ok(())
    }
}
```

The borrowing issue is tricky. We need to restructure. Options:
1. Use `RefCell` or `UnsafeCell` for interior mutability
2. Restructure to avoid needing mutable access to `self` in context
3. Collect all work first, then do it

Let's use a different approach: pass references to what's needed:

```rust
pub struct RenderContextImpl<'a> {
    nodes: &'a mut BTreeMap<NodeHandle, NodeEntry>,
    frame_id: FrameId,
    fs: &'a dyn LpFs,  // May not be needed for rendering
}

impl<'a> RenderContext for RenderContextImpl<'a> {
    fn get_texture(&mut self, handle: TextureHandle) -> Result<&Texture, Error> {
        // Can access self.nodes mutably
        let node_handle = handle.as_node_handle();
        // ... implementation
    }
}

// In ProjectRuntime::render():
let mut ctx = RenderContextImpl {
    nodes: &mut self.nodes,
    frame_id: self.frame_id,
    fs: self.fs.as_ref(),
};
```

This works better. Let's update the design.

## Success Criteria

- All code compiles
- `RenderContextImpl::get_texture()` triggers lazy rendering
- `RenderContextImpl::get_output()` returns output buffer slice
- Lazy rendering updates texture `state_ver`
- Tests pass

## Notes

- Borrowing rules require careful structure - pass references to `nodes` map instead of `self`
- Shader rendering is stubbed for now (will implement when shader runtime is done)
- Output runtime not implemented yet, so `get_output()` will be a todo!() for now
- Texture lazy rendering checks `state_ver` and updates it after (potential) shader rendering
