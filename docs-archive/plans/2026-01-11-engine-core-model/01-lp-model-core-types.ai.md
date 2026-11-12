# Phase 1: lp-model - Core Types (Minimal)

## Goal

Create basic types needed for everything else. Keep it minimal - just enough to compile and be used by later phases.

## Implementation

### 1. Path and FrameId

**File**: `lp-model/src/path.rs`
```rust
use alloc::string::String;

/// Light Player path - absolute paths from project root
/// 
/// Currently supports absolute paths only. Designed to support relative paths
/// later when nodes become nestable.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LpPath(pub String);

impl LpPath {
    pub fn new(path: String) -> Self {
        Self(path)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for LpPath {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for LpPath {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}
```

**File**: `lp-model/src/project/frame_id.rs`
```rust
/// Frame identifier - increments each render frame
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FrameId(pub i64);

impl FrameId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }
    
    pub fn as_i64(self) -> i64 {
        self.0
    }
    
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for FrameId {
    fn default() -> Self {
        Self(0)
    }
}
```

**File**: `lp-model/src/project/mod.rs`
```rust
pub mod config;
pub mod frame_id;

pub use config::ProjectConfig;
pub use frame_id::FrameId;
```

### 2. Node Types

**File**: `lp-model/src/nodes/kind.rs`
```rust
/// Node kind - matches directory suffixes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Texture,
    Shader,
    Output,
    Fixture,
}
```

**File**: `lp-model/src/nodes/handle.rs`
```rust
/// Runtime node identifier - sequential generation
/// 
/// Handles can change on reload (not stable). Paths are for loading/resolving;
/// handles are for runtime references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeHandle(pub i32);

impl NodeHandle {
    pub fn new(id: i32) -> Self {
        Self(id)
    }
    
    pub fn as_i32(self) -> i32 {
        self.0
    }
}
```

**File**: `lp-model/src/nodes/specifier.rs`
```rust
use alloc::string::String;

/// Node specifier - currently just a path string
/// 
/// May support other specifier types in the future (e.g., expressions, handles).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NodeSpecifier(pub String);

impl NodeSpecifier {
    pub fn new(spec: String) -> Self {
        Self(spec)
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for NodeSpecifier {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeSpecifier {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}
```

**File**: `lp-model/src/nodes/mod.rs`
```rust
pub mod kind;
pub mod handle;
pub mod specifier;

// Node type modules
pub mod texture;
pub mod shader;
pub mod output;
pub mod fixture;

pub use kind::NodeKind;
pub use handle::NodeHandle;
pub use specifier::NodeSpecifier;

/// Node config trait - all node configs implement this
pub trait NodeConfig {
    fn kind(&self) -> NodeKind;
}
```

### 3. Project Config

**File**: `lp-model/src/project/config.rs`
```rust
use alloc::string::String;

/// Project configuration - minimal, no nodes field
/// 
/// Nodes are discovered from filesystem, not stored in config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    pub uid: String,
    pub name: String,
}
```

### 4. Node Configs (Minimal)

**File**: `lp-model/src/nodes/texture/config.rs`
```rust
use crate::nodes::{NodeConfig, NodeKind};

/// Texture node configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureConfig {
    /// Memory texture - simple buffer
    Memory {
        width: u32,
        height: u32,
        // format: todo!(), // Will add format later
    },
}

impl NodeConfig for TextureConfig {
    fn kind(&self) -> NodeKind {
        NodeKind::Texture
    }
}
```

**File**: `lp-model/src/nodes/texture/mod.rs`
```rust
pub mod config;
pub mod state;

pub use config::TextureConfig;
pub use state::TextureState;
```

**File**: `lp-model/src/nodes/shader/config.rs`
```rust
use crate::nodes::{NodeConfig, NodeKind, NodeSpecifier};

/// Shader node configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderConfig {
    /// Path to GLSL file (relative to node directory)
    pub glsl_path: String,
    /// Texture to render to (specifier)
    pub texture_spec: NodeSpecifier,
    /// Render order - lower numbers render first (default 0)
    pub render_order: i32,
}

impl Default for ShaderConfig {
    fn default() -> Self {
        Self {
            glsl_path: "main.glsl".to_string(),
            texture_spec: NodeSpecifier::from(""),
            render_order: 0,
        }
    }
}

impl NodeConfig for ShaderConfig {
    fn kind(&self) -> NodeKind {
        NodeKind::Shader
    }
}
```

**File**: `lp-model/src/nodes/shader/mod.rs`
```rust
pub mod config;
pub mod state;

pub use config::ShaderConfig;
pub use state::ShaderState;
```

**File**: `lp-model/src/nodes/output/config.rs`
```rust
use crate::nodes::{NodeConfig, NodeKind};

/// Output node configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputConfig {
    /// GPIO strip output
    GpioStrip {
        pin: u32,
        // channel_count: todo!(), // Will add later
    },
}

impl NodeConfig for OutputConfig {
    fn kind(&self) -> NodeKind {
        NodeKind::Output
    }
}
```

**File**: `lp-model/src/nodes/output/mod.rs`
```rust
pub mod config;
pub mod state;

pub use config::OutputConfig;
pub use state::OutputState;
```

**File**: `lp-model/src/nodes/fixture/config.rs`
```rust
use crate::nodes::{NodeConfig, NodeKind, NodeSpecifier};

/// Fixture node configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureConfig {
    /// Output node specifier
    pub output_spec: NodeSpecifier,
    /// Texture node specifier
    pub texture_spec: NodeSpecifier,
    /// Mapping configuration (simplified for now)
    pub mapping: String, // todo!() - will be structured type later
    /// Lamp type (color order, etc.)
    pub lamp_type: String, // todo!() - will be enum later
    /// Transform matrix (4x4)
    pub transform: [[f32; 4]; 4], // todo!() - will be proper matrix type later
}

impl NodeConfig for FixtureConfig {
    fn kind(&self) -> NodeKind {
        NodeKind::Fixture
    }
}
```

**File**: `lp-model/src/nodes/fixture/mod.rs`
```rust
pub mod config;
pub mod state;

pub use config::FixtureConfig;
pub use state::FixtureState;
```

### 5. Node States (Minimal)

**File**: `lp-model/src/nodes/texture/state.rs`
```rust
use alloc::vec::Vec;

/// Texture node state - runtime values
#[derive(Debug, Clone)]
pub struct TextureState {
    /// Texture pixel data (RGBA, width * height * 4)
    pub texture_data: Vec<u8>,
}
```

**File**: `lp-model/src/nodes/shader/state.rs`
```rust
use alloc::string::String;
use alloc::vec::Vec;

/// Shader node state - runtime values
#[derive(Debug, Clone)]
pub struct ShaderState {
    /// Actual GLSL code loaded from file
    pub glsl_code: String,
    /// Compilation/runtime errors
    pub error: Option<String>,
}
```

**File**: `lp-model/src/nodes/output/state.rs`
```rust
use alloc::vec::Vec;

/// Output node state - runtime values
#[derive(Debug, Clone)]
pub struct OutputState {
    /// Channel data buffer
    pub channel_data: Vec<u8>,
}
```

**File**: `lp-model/src/nodes/fixture/state.rs`
```rust
use alloc::vec::Vec;

/// Fixture node state - runtime values
#[derive(Debug, Clone)]
pub struct FixtureState {
    /// Lamp color values (RGB per lamp)
    pub lamp_colors: Vec<u8>,
}
```

### 6. Update lib.rs

**File**: `lp-model/src/lib.rs`
```rust
#![no_std]

extern crate alloc;

pub mod nodes;
pub mod path;
pub mod project;

pub use path::LpPath;
pub use project::{FrameId, ProjectConfig};
pub use nodes::{NodeKind, NodeHandle, NodeSpecifier, NodeConfig};
```

## Success Criteria

- All code compiles
- Can create instances of all types
- Basic type tests pass
- `NodeConfig` trait implemented for all config types

## Tests

Add basic tests in each module:
- `LpPath` creation and conversion
- `FrameId` creation and `next()`
- `NodeHandle` creation
- `NodeSpecifier` creation
- `ProjectConfig` creation
- Each `NodeConfig` implements `kind()` correctly
