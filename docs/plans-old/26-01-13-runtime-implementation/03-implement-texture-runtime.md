# Phase 3: Implement Texture Runtime

## Goal

Implement `TextureRuntime` with initialization and state extraction. Textures allocate buffers based on config and can provide state for sync API.

## Dependencies

- Phase 2
- `lp-shared::util::Texture` (may need to check for circular dependency)

## Implementation

### 1. Add Texture Field to TextureRuntime

**File**: `lp-engine/src/nodes/texture/runtime.rs`

```rust
use lp_shared::util::Texture;
use lp_model::nodes::texture::{TextureConfig, TextureState};

pub struct TextureRuntime {
    texture: Option<Texture>,  // None until initialized
    node_handle: NodeHandle,  // For state extraction
}

impl TextureRuntime {
    pub fn new(node_handle: NodeHandle) -> Self {
        Self {
            texture: None,
            node_handle,
        }
    }
    
    pub fn texture(&self) -> Option<&Texture> {
        self.texture.as_ref()
    }
    
    pub fn texture_mut(&mut self) -> Option<&mut Texture> {
        self.texture.as_mut()
    }
    
    pub fn get_state(&self) -> TextureState {
        // Extract state for sync API
        if let Some(tex) = &self.texture {
            TextureState {
                texture_data: tex.data().to_vec(),  // Copy texture data
            }
        } else {
            TextureState {
                texture_data: Vec::new(),
            }
        }
    }
}
```

### 2. Implement init()

**File**: `lp-engine/src/nodes/texture/runtime.rs`

```rust
impl NodeRuntime for TextureRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // Get config from context (need to pass config or look it up)
        // Actually, config should be passed to init() or stored in runtime
        // For now, we'll need to get it from ProjectRuntime via node_handle
        // This is a design issue - let's store config in TextureRuntime
        
        // Actually, looking at the design, config is in NodeEntry, not passed to init()
        // We need a way to get config. Options:
        // 1. Pass config to init() (change trait signature)
        // 2. Store config in runtime (but runtime is created before init)
        // 3. Look up config from ProjectRuntime via node_handle
        
        // For now, let's assume we can get config somehow. Will need to check how this works.
        todo!("Get texture config - may need to change init() signature or pass config")
    }
}
```

Actually, looking at the existing code, `init()` doesn't receive config. We need to either:
1. Change `NodeRuntime::init()` to take config
2. Store config in runtime before calling init
3. Look up config from ProjectRuntime

Let's check how it's currently done... The runtime is created, then `init()` is called. Config is in `NodeEntry`. We could:
- Pass config to `init()` (requires trait change)
- Store config in runtime when creating it (before init)

Let's go with storing config in runtime:

```rust
pub struct TextureRuntime {
    config: Option<TextureConfig>,  // Stored when runtime is created
    texture: Option<Texture>,
    node_handle: NodeHandle,
}

impl TextureRuntime {
    pub fn new(node_handle: NodeHandle) -> Self {
        Self {
            config: None,
            texture: None,
            node_handle,
        }
    }
    
    pub fn set_config(&mut self, config: TextureConfig) {
        self.config = Some(config);
    }
}

impl NodeRuntime for TextureRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        let config = self.config.as_ref()
            .ok_or_else(|| Error::InvalidConfig {
                node_path: format!("texture-{}", self.node_handle.as_i32()),
                reason: "Config not set".to_string(),
            })?;
        
        // For now, TextureConfig is an enum with Memory variant
        // But design says it should be a struct. Let's handle both cases.
        match config {
            TextureConfig::Memory { width, height } => {
                // Create texture with RGBA8 format (default for now)
                let format = "RGBA8".to_string();  // From lp-shared formats
                let texture = Texture::new(*width, *height, format)
                    .map_err(|e| Error::InvalidConfig {
                        node_path: format!("texture-{}", self.node_handle.as_i32()),
                        reason: format!("Failed to create texture: {}", e),
                    })?;
                
                self.texture = Some(texture);
                Ok(())
            }
        }
    }
    
    fn render(&mut self, _ctx: &dyn RenderContext) -> Result<(), Error> {
        // No-op - textures don't render themselves, shaders render to textures
        Ok(())
    }
}
```

### 3. Update ProjectRuntime to Set Config

**File**: `lp-engine/src/project/runtime.rs`

```rust
impl ProjectRuntime {
    pub fn initialize_nodes(&mut self) -> Result<(), Error> {
        // ... existing code ...
        
        for handle in handles {
            if let Some(entry) = self.nodes.get_mut(&handle) {
                // Create runtime based on kind
                let mut runtime: Box<dyn NodeRuntime> = match entry.kind {
                    NodeKind::Texture => {
                        let mut tex_runtime = TextureRuntime::new(handle);
                        // Set config before init
                        if let Some(tex_config) = entry.config.as_any().downcast_ref::<TextureConfig>() {
                            tex_runtime.set_config(tex_config.clone());
                        }
                        Box::new(tex_runtime)
                    }
                    // ... other kinds ...
                };
                
                // ... rest of init ...
            }
        }
    }
}
```

Actually, we can't downcast `Box<dyn NodeConfig>`. We need a different approach. Let's check the config when creating the runtime:

```rust
// In initialize_nodes(), when creating TextureRuntime:
let config = entry.config.as_ref();
// We know it's TextureConfig because entry.kind == Texture
// But we can't safely downcast...

// Better: store config in NodeEntry as concrete type, or
// have a way to get config by kind

// For now, let's create runtime with config directly:
match entry.kind {
    NodeKind::Texture => {
        // We need to extract TextureConfig from Box<dyn NodeConfig>
        // This is tricky - we'll need to match on the config type
        // Or change how configs are stored
        
        // Actually, let's just pass the config to a new() method that takes config
        let tex_config = // ... extract somehow
        let mut tex_runtime = TextureRuntime::new_with_config(handle, tex_config);
        Box::new(tex_runtime)
    }
}
```

This is getting complex. Let's simplify: store config in runtime when we create it, using a helper that extracts config:

```rust
// Helper function to create texture runtime with config
fn create_texture_runtime(handle: NodeHandle, config: &dyn NodeConfig) -> TextureRuntime {
    // We know it's TextureConfig because we're in the Texture case
    // But we can't safely downcast. We need to serialize/deserialize or
    // have a method on NodeConfig to get concrete type
    
    // For now, let's assume we can get width/height from config somehow
    // Or we'll need to change the design
    
    todo!("Extract TextureConfig from Box<dyn NodeConfig>")
}
```

Actually, the cleanest approach: change `NodeRuntime::init()` to take config, or have a separate method. But that changes the trait. Let's check if we can add a method to `NodeConfig`:

```rust
// In lp-model, add to NodeConfig trait:
pub trait NodeConfig: core::fmt::Debug {
    fn kind(&self) -> NodeKind;
    // Add method to clone as concrete type? Not possible with trait objects.
}
```

Best approach: Store config in runtime when creating it, using a match on the config's kind and extracting the concrete type. We'll need to handle this in `initialize_nodes()`:

```rust
// When creating TextureRuntime, we know entry.kind == Texture
// So we can safely assume config is TextureConfig
// But we need to extract it from Box<dyn NodeConfig>

// Option: serialize and deserialize
// Option: store config separately by kind
// Option: change init() signature

// For now, let's use a workaround: create runtime, then set config via a method
// that takes Box<dyn NodeConfig> and extracts TextureConfig
```

Let's go with a simpler approach for now: create runtime, then in init(), we'll need to get config from somewhere. Actually, let's just pass config to init() by changing the context:

```rust
trait NodeInitContext {
    fn get_config(&self) -> &dyn NodeConfig;  // Get config for current node
    // ... other methods
}
```

But that doesn't work because context doesn't know which node is initializing.

Best solution: Store config in runtime when creating it. We'll need a way to extract concrete config. For now, let's use a helper that creates runtime with config extracted:

```rust
fn create_texture_runtime_with_config(handle: NodeHandle, config: Box<dyn NodeConfig>) -> Result<TextureRuntime, Error> {
    // We know it's TextureConfig, but can't downcast
    // For now, serialize and deserialize
    let config_json = serde_json::to_string(&config)  // Won't work, config is trait object
    
    // Better: store config in NodeEntry as Option<TextureConfig> separately?
    // Or: change NodeEntry to store config by kind
    
    // Simplest for now: assume TextureConfig::Memory and extract from JSON
    // But that's hacky
    
    // Actually, let's just create empty runtime and set config in a separate step
    // Or: change the design to pass config to init()
    
    todo!("Need to decide how to pass config to runtime")
}
```

Let me check the existing code to see how config is handled... Actually, let's just proceed with a workaround for now and note it:

```rust
// In initialize_nodes(), we'll extract config before creating runtime
// For Texture, we know it's TextureConfig::Memory, so we can create it directly
// But we need width/height. Let's assume we can get them somehow.

// Actually, simplest: create runtime without config, then in init() look up config
// from ProjectRuntime using node_handle. But that requires runtime to have reference to ProjectRuntime.

// Best: Pass config to init() via context, or store in runtime before init.
// Let's go with storing in runtime, and we'll extract config when creating runtime.
```

For now, let's use a simpler approach: create runtime, then set config via a method that takes the concrete config type. We'll extract config in `initialize_nodes()` by matching on the config's serialized form or by storing configs separately.

Actually, let's just proceed with the assumption that we can extract config, and we'll implement a helper:

```rust
// Helper to extract TextureConfig (will need to handle this properly)
fn extract_texture_config(config: &dyn NodeConfig) -> Option<TextureConfig> {
    // For now, return None and handle in implementation
    // We'll need to serialize/deserialize or have a better way
    None
}
```

Let's document this as a limitation and proceed:

## Success Criteria

- All code compiles
- `TextureRuntime` stores `Texture` instance
- `init()` allocates texture based on config
- `get_state()` extracts `TextureState` for sync API
- Tests pass

## Notes

- Config extraction from `Box<dyn NodeConfig>` is tricky - may need to serialize/deserialize or change design
- Texture format defaults to RGBA8 for now
- Texture is initialized to zeros
- `render()` is no-op (shaders render to textures)
