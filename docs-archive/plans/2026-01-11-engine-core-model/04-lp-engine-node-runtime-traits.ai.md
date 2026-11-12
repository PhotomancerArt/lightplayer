# Phase 4: lp-engine - Node Runtime Traits (Minimal)

## Goal

Define runtime interfaces - `NodeRuntime` trait and context traits. Keep methods stubbed with `todo!()` for now.

## Dependencies

- `lp-model` (all phases)
- `lp-engine` Phase 3

## Implementation

### 1. Node Runtime Trait

**File**: `lp-engine/src/nodes/mod.rs`
```rust
use crate::error::Error;
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use lp_model::NodeConfig;

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

### 2. Runtime Contexts

**File**: `lp-engine/src/runtime/contexts.rs`
```rust
use crate::error::Error;
use lp_model::{LpPath, NodeSpecifier};
use lp_shared::fs::LpFs;

/// Handles for resolved nodes (opaque types for now)
pub struct TextureHandle(u32);
pub struct OutputHandle(u32);

impl TextureHandle {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl OutputHandle {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

/// Context for node initialization
pub trait NodeInitContext {
    /// Resolve an output node specifier to a handle
    fn resolve_output(&self, spec: &NodeSpecifier) -> Result<OutputHandle, Error> {
        todo!("Node resolution not implemented yet")
    }
    
    /// Resolve a texture node specifier to a handle
    fn resolve_texture(&self, spec: &NodeSpecifier) -> Result<TextureHandle, Error> {
        todo!("Node resolution not implemented yet")
    }
    
    /// Get filesystem for this node
    fn get_node_fs(&self) -> &dyn LpFs {
        todo!("Filesystem access not implemented yet")
    }
}

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

### 3. Module Structure

**File**: `lp-engine/src/runtime/mod.rs`
```rust
pub mod contexts;

pub use contexts::{NodeInitContext, RenderContext, TextureHandle, OutputHandle};
```

**File**: Update `lp-engine/src/lib.rs`
```rust
#![no_std]

extern crate alloc;

pub mod error;
pub mod nodes;
pub mod project;
pub mod runtime;

pub use error::Error;
pub use project::ProjectRuntime;
pub use nodes::{NodeRuntime, NodeConfig};
pub use runtime::{NodeInitContext, RenderContext};
```

## Success Criteria

- All code compiles
- Can define `Box<dyn NodeRuntime>` trait objects
- Context traits are defined (methods can be `todo!()`)

## Notes

- Context methods use `todo!()` - will be implemented when we add initialization and rendering
- `TextureHandle` and `OutputHandle` are opaque types for now
- Traits are `Send + Sync` for thread safety (may need to adjust later)
