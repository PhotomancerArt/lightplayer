# Design: Restore Debug UI

## Overview

Store node configs in their respective runtimes and update debug UI panel functions to use the existing helper functions.

## File Structure

```
lp-core/src/nodes/
├── texture/
│   └── runtime.rs              # MODIFY: Add config: TextureNode field
├── shader/
│   └── runtime.rs              # MODIFY: Add config: ShaderNode field
├── fixture/
│   └── runtime.rs              # MODIFY: Add config: FixtureNode field
└── output/
    └── runtime.rs              # MODIFY: Add config: OutputNode field

lp-core/src/project/
└── runtime.rs                  # MODIFY: Update init() to pass configs to runtimes

fw-host/src/
└── debug_ui.rs                 # MODIFY: Update panel functions to use helper functions
```

## Code Structure

### New Fields

- `TextureNodeRuntime.config: TextureNode` - Store texture config in runtime
- `ShaderNodeRuntime.config: ShaderNode` - Store shader config (includes GLSL code) in runtime
- `FixtureNodeRuntime.config: FixtureNode` - Store fixture config in runtime
- `OutputNodeRuntime.config: OutputNode` - Store output config in runtime

### New Methods

- `TextureNodeRuntime::config() -> &TextureNode` - Get config reference
- `ShaderNodeRuntime::config() -> &ShaderNode` - Get config reference
- `FixtureNodeRuntime::config() -> &FixtureNode` - Get config reference
- `OutputNodeRuntime::config() -> &OutputNode` - Get config reference

### Modified Methods

**Node Runtime `init()` methods:**
- `TextureNodeRuntime::init()`: Store `config` parameter in `self.config`
- `ShaderNodeRuntime::init()`: Store `config` parameter in `self.config`
- `FixtureNodeRuntime::init()`: Store `config` parameter in `self.config`
- `OutputNodeRuntime::init()`: Store `config` parameter in `self.config`

**Debug UI Panel Functions:**
- `render_textures_panel()`: 
  - Iterate over texture IDs from runtime
  - Get texture runtime and config
  - Get texture data from runtime
  - Call `render_texture()` with config and data
- `render_shaders_panel()`:
  - Iterate over shader IDs from runtime
  - Get shader runtime and config
  - Call `render_shader_panel()` with config and runtime
- `render_fixtures_panel()`:
  - Iterate over fixture IDs from runtime
  - Get fixture runtime and config
  - Call `render_fixture()` with config and runtime

### Removed

- `#[allow(dead_code)]` from `render_texture()`, `render_shader_panel()`, `render_fixture()` - these will now be used

## Key Design Decisions

1. **Configs stored in runtimes**: Configs are immutable and stored alongside their runtimes, avoiding filesystem reloads
2. **Reuse existing helper functions**: The original `render_texture()`, `render_shader_panel()`, and `render_fixture()` functions are good and should be used
3. **Always use actual texture data**: Pass texture data from runtime to show actual rendered content
4. **GLSL code in config**: GLSL code is already in ShaderNode config, so storing config in runtime gives access to it

