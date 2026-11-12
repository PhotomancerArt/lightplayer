# Phase 7: Refactor Other Node Runtimes to Store State Directly

## Scope of Phase

Refactor `TextureRuntime`, `OutputRuntime`, and `ShaderRuntime` to store their respective state structs directly, following the same pattern as `FixtureRuntime`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update `TextureRuntime`

Modify `lp-engine/src/nodes/texture/runtime.rs`:

```rust
use lp_model::nodes::texture::TextureState;

pub struct TextureRuntime {
    config: Option<TextureConfig>,
    state: TextureState,  // NEW: State stored directly
    // Implementation details...
}

impl TextureRuntime {
    pub fn new() -> Self {
        Self {
            config: None,
            state: TextureState::new(FrameId::default()),
            // ... other fields
        }
    }
    
    // Update state when texture data changes
    fn update_texture(&mut self, ctx: &mut dyn RenderContext, data: Vec<u8>, width: u32, height: u32, format: String) {
        self.state.texture_data.set(ctx.frame_id(), data);
        self.state.width.set(ctx.frame_id(), width);
        self.state.height.set(ctx.frame_id(), height);
        self.state.format.set(ctx.frame_id(), format);
    }
}
```

### 2. Update `OutputRuntime`

Modify `lp-engine/src/nodes/output/runtime.rs`:

```rust
use lp_model::nodes::output::OutputState;

pub struct OutputRuntime {
    config: Option<OutputConfig>,
    state: OutputState,  // NEW: State stored directly
    // Implementation details...
}

impl OutputRuntime {
    pub fn new() -> Self {
        Self {
            config: None,
            state: OutputState::new(FrameId::default()),
            // ... other fields
        }
    }
    
    // Update state when channel data changes
    fn update_channel_data(&mut self, ctx: &mut dyn RenderContext, data: Vec<u8>) {
        self.state.channel_data.set(ctx.frame_id(), data);
    }
}
```

### 3. Update `ShaderRuntime`

Modify `lp-engine/src/nodes/shader/runtime.rs`:

```rust
use lp_model::nodes::shader::ShaderState;

pub struct ShaderRuntime {
    config: Option<ShaderConfig>,
    state: ShaderState,  // NEW: State stored directly
    // Implementation details...
}

impl ShaderRuntime {
    pub fn new() -> Self {
        Self {
            config: None,
            state: ShaderState::new(FrameId::default()),
            // ... other fields
        }
    }
    
    // Update state when errors change
    fn update_errors(&mut self, ctx: &mut dyn RenderContext, errors: Vec<String>) {
        self.state.errors.set(ctx.frame_id(), errors);
    }
}
```

### 4. Update state extraction in `ProjectRuntime`

In `lp-engine/src/project/runtime.rs`, update state extraction for all node types:

```rust
let state = match entry.kind {
    NodeKind::Texture => {
        if let Some(runtime) = &entry.runtime {
            if let Some(texture_runtime) = runtime.as_any().downcast_ref::<TextureRuntime>() {
                NodeState::Texture(texture_runtime.state.clone())
            } else {
                NodeState::Texture(TextureState::new(FrameId::default()))
            }
        } else {
            NodeState::Texture(TextureState::new(FrameId::default()))
        }
    }
    NodeKind::Output => {
        // Similar pattern...
    }
    NodeKind::Shader => {
        // Similar pattern...
    }
    NodeKind::Fixture => {
        // Already done in Phase 5
    }
};
```

### 5. Remove old getter methods

Remove methods like `get_state()`, `get_texture_data()`, etc., since state is now directly accessible. Or keep them as convenience methods.

### 6. Update all code that accesses state

Search for code that accesses state fields directly and update to use `state.field.get()` or `state.field.set()`.

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-engine
cargo test nodes::texture
cargo test nodes::output
cargo test nodes::shader
cargo check
```

Fix any warnings or errors before proceeding. All runtimes should now store state directly and update it via `StateField` methods.
