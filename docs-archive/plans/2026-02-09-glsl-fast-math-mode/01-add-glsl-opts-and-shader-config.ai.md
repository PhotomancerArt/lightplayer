# Phase 1: Add GlslOpts and Extend ShaderConfig

## Scope of phase

Create `GlslOpts` struct in lp-model and add optional `glsl_opts` field to `ShaderConfig`. Enables per-shader-node GLSL compilation options (fast_math and future options).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create GlslOpts in lp-model

**New file**: `lp-core/lp-model/src/glsl_opts.rs`

```rust
use serde::{Deserialize, Serialize};

/// GLSL compilation options (per-shader-node)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlslOpts {
    /// Use inline iadd/isub for add/sub (wrapping) instead of saturating builtins
    #[serde(default)]
    pub fast_math: bool,
}

impl Default for GlslOpts {
    fn default() -> Self {
        Self { fast_math: false }
    }
}
```

Add `pub mod glsl_opts;` to `lp-model/src/lib.rs`. Re-export if appropriate: `pub use glsl_opts::GlslOpts;` (or leave as module-only if ShaderConfig is the main consumer).

### 2. Add glsl_opts to ShaderConfig

**File**: `lp-core/lp-model/src/nodes/shader/config.rs`

- Add `use crate::glsl_opts::GlslOpts;` (or `use crate::GlslOpts` if re-exported)
- Add field: `#[serde(default)] pub glsl_opts: Option<GlslOpts>`
- Update `Default` impl: `glsl_opts: None`
**Chosen**: `pub glsl_opts: GlslOpts` with `#[serde(default)]` on the field - when JSON omits it, Serde uses `GlslOpts::default()`. Simpler for consumers, no Option branching.

### 3. Update ShaderConfig Default

```rust
impl Default for ShaderConfig {
    fn default() -> Self {
        Self {
            glsl_path: "main.glsl".as_path_buf(),
            texture_spec: NodeSpecifier::from(""),
            render_order: 0,
            glsl_opts: GlslOpts::default(),
        }
    }
}
```

### 4. Tests

- Test `GlslOpts::default()` has `fast_math: false`
- Test `ShaderConfig` deserializes from JSON without `glsl_opts` (backward compat)
- Test `ShaderConfig` deserializes with `"glsl_opts": {"fast_math": true}`

## Validate

```bash
cargo build -p lp-model
cargo test -p lp-model
```
