# Phase 4: Wire Graphics Through ProjectRuntime

## Scope

Update `ProjectRuntime` to hold `Rc<dyn LpGraphics>` and pass it to shader node construction.

## Files

```
lp-core/lp-engine/src/
├── project/runtime.rs            # UPDATE: add graphics field
```

## ProjectRuntime Changes

### Add graphics field

```rust
use alloc::rc::Rc;
use crate::gfx::LpGraphics;

pub struct ProjectRuntime {
    project: Option<Project>,
    root_scene: Option<NodeHandle>,
    events: EventQueue,
    data_pool: SceneDataPool,
    texture_pool: TexturePool,
    file_watcher: Option<FileWatcher>,
    graphics: Rc<dyn LpGraphics>,     // NEW
}
```

### Constructor changes

```rust
impl ProjectRuntime {
    /// Create a new ProjectRuntime with injected graphics backend.
    ///
    /// The graphics backend is cloned into all shader nodes as they
    /// are constructed.
    pub fn new(graphics: Rc<dyn LpGraphics>) -> Self {
        Self {
            project: None,
            root_scene: None,
            events: EventQueue::default(),
            data_pool: SceneDataPool::new(),
            texture_pool: TexturePool::new(),
            file_watcher: None,
            graphics,
        }
    }

    /// For backward compatibility or tests without graphics.
    ///
    /// Shaders won't work without a graphics backend, but other
    /// node types will function normally.
    pub fn without_graphics() -> Self {
        // This exists temporarily for tests that don't need shaders.
        // In Phase 6, we'll remove this or provide a test mock.
        panic!("without_graphics is deprecated - use a mock graphics backend")
    }
}
```

### Pass graphics to shader construction

Locate `NodeType::Shader` construction in `init_scene_from_model`:

```rust
NodeType::Shader => {
    let config = ShaderConfig::new_from_params(node, &shader_ctx);
    let glsl_code = config
        .as_ref()
        .and_then(|c| fs.read_to_string(c.glsl_path.as_deref()?));
    let mut shader_runtime = ShaderRuntime::new(handle, self.graphics.clone()); // Pass graphics
    // ... rest of initialization
}
```

Same for `handle_fs_change` when recreating shaders on file changes:

```rust
NodeType::Shader => {
    let config = ShaderConfig::new_from_params(node, &shader_ctx);
    let glsl_code = config
        .as_ref()
        .and_then(|c| fs.read_to_string(c.glsl_path.as_deref()?));
    let mut shader_runtime = ShaderRuntime::new(handle, self.graphics.clone()); // Pass graphics
    // ... compile and set runtime
}
```

## Error Handling

`ProjectRuntime::new()` signature changes, so all call sites will fail to compile until Phase 6 (firmware updates).

## Validate

```bash
cargo check -p lp-engine --lib 2>&1 | head -20
```

Expected: Errors in ProjectRuntime::new() call sites (lp-server, fw-emu, fw-esp32, tests).
