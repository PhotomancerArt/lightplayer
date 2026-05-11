# Phase 5: lp-engine - Basic Node Runtimes (Minimal)

## Goal

Create stub implementations of all node runtimes. They should compile and implement the trait, but methods can be `todo!()`.

## Dependencies

- `lp-engine` Phase 4

## Implementation

### 1. Texture Runtime

**File**: `lp-engine/src/nodes/texture/runtime.rs`
```rust
use crate::error::Error;
use crate::nodes::NodeRuntime;
use crate::runtime::contexts::{NodeInitContext, RenderContext};

/// Texture node runtime
pub struct TextureRuntime {
    // Will add fields later
}

impl TextureRuntime {
    pub fn new() -> Self {
        Self {}
    }
}

impl NodeRuntime for TextureRuntime {
    fn init(&mut self, _ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // todo!("Texture initialization")
        Ok(())
    }
    
    fn render(&mut self, _ctx: &dyn RenderContext) -> Result<(), Error> {
        // todo!("Texture rendering")
        Ok(())
    }
}
```

**File**: `lp-engine/src/nodes/texture/mod.rs`
```rust
pub mod runtime;

pub use runtime::TextureRuntime;
```

### 2. Shader Runtime

**File**: `lp-engine/src/nodes/shader/runtime.rs`
```rust
use crate::error::Error;
use crate::nodes::NodeRuntime;
use crate::runtime::contexts::{NodeInitContext, RenderContext};

/// Shader node runtime
pub struct ShaderRuntime {
    // Will add fields later
}

impl ShaderRuntime {
    pub fn new() -> Self {
        Self {}
    }
}

impl NodeRuntime for ShaderRuntime {
    fn init(&mut self, _ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // todo!("Shader initialization - load GLSL, compile")
        Ok(())
    }
    
    fn render(&mut self, _ctx: &dyn RenderContext) -> Result<(), Error> {
        // todo!("Shader rendering - execute GLSL")
        Ok(())
    }
}
```

**File**: `lp-engine/src/nodes/shader/mod.rs`
```rust
pub mod runtime;

pub use runtime::ShaderRuntime;
```

### 3. Output Runtime

**File**: `lp-engine/src/nodes/output/runtime.rs`
```rust
use crate::error::Error;
use crate::nodes::NodeRuntime;
use crate::runtime::contexts::{NodeInitContext, RenderContext};

/// Output node runtime
pub struct OutputRuntime {
    // Will add fields later
}

impl OutputRuntime {
    pub fn new() -> Self {
        Self {}
    }
}

impl NodeRuntime for OutputRuntime {
    fn init(&mut self, _ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // todo!("Output initialization - setup GPIO, etc.")
        Ok(())
    }
    
    fn render(&mut self, _ctx: &dyn RenderContext) -> Result<(), Error> {
        // todo!("Output rendering - flush buffers")
        Ok(())
    }
}
```

**File**: `lp-engine/src/nodes/output/mod.rs`
```rust
pub mod runtime;

pub use runtime::OutputRuntime;
```

### 4. Fixture Runtime

**File**: `lp-engine/src/nodes/fixture/runtime.rs`
```rust
use crate::error::Error;
use crate::nodes::NodeRuntime;
use crate::runtime::contexts::{NodeInitContext, RenderContext};

/// Fixture node runtime
pub struct FixtureRuntime {
    // Will add fields later
}

impl FixtureRuntime {
    pub fn new() -> Self {
        Self {}
    }
}

impl NodeRuntime for FixtureRuntime {
    fn init(&mut self, _ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // todo!("Fixture initialization - resolve output/texture references")
        Ok(())
    }
    
    fn render(&mut self, _ctx: &dyn RenderContext) -> Result<(), Error> {
        // todo!("Fixture rendering - sample texture, write to output")
        Ok(())
    }
}
```

**File**: `lp-engine/src/nodes/fixture/mod.rs`
```rust
pub mod runtime;

pub use runtime::FixtureRuntime;
```

### 5. Update nodes/mod.rs

**File**: Update `lp-engine/src/nodes/mod.rs`
```rust
use crate::error::Error;
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use lp_model::NodeConfig;

pub mod texture;
pub mod shader;
pub mod output;
pub mod fixture;

pub use texture::TextureRuntime;
pub use shader::ShaderRuntime;
pub use output::OutputRuntime;
pub use fixture::FixtureRuntime;

/// Node runtime trait - all node runtimes implement this
pub trait NodeRuntime: Send + Sync {
    /// Initialize the node
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;
    
    /// Render the node
    fn render(&mut self, ctx: &dyn RenderContext) -> Result<(), Error>;
    
    /// Destroy the node (cleanup)
    fn destroy(&mut self) -> Result<(), Error> {
        todo!("Node cleanup not implemented yet")
    }
}

// Re-export NodeConfig from lp-model
pub use lp_model::NodeConfig;
```

## Success Criteria

- All code compiles
- Can create instances of all runtime types
- All runtimes implement `NodeRuntime` trait
- Can create `Box<dyn NodeRuntime>` from any runtime

## Tests

Add basic tests:
- Create each runtime type
- Call `init()` and `render()` (should succeed even if they do nothing)
- Convert to `Box<dyn NodeRuntime>`

## Notes

- All runtimes are empty structs for now - fields will be added as needed
- Methods return `Ok(())` for now - will implement actual logic later
- `todo!()` comments indicate what each method should do eventually
