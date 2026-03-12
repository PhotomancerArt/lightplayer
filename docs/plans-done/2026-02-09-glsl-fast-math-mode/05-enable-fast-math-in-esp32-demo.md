# Phase 5: Enable Fast Math in ESP32 Demo Project

## Scope of phase

Add `glsl_opts: { "fast_math": true }` to the rainbow.shader node.json in the esp32 demo project so the demo uses fast math.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update rainbow.shader node.json in demo_project.rs

**File**: `lp-fw/fw-esp32/src/demo_project.rs`

Current JSON:

```json
{
  "glsl_path": "main.glsl",
  "texture_spec": "/src/main.texture",
  "render_order": 0
}
```

Update to:

```json
{
  "glsl_path": "main.glsl",
  "texture_spec": "/src/main.texture",
  "render_order": 0,
  "glsl_opts": {
    "fast_math": true
  }
}
```

In the raw string, add the new field. Escape as needed for Rust string literals.

### 2. Verify ESP32 build

The esp32 firmware loads projects from the memory filesystem. The demo project is written by `write_basic_project`. When the firmware runs, it loads the project and compiles shaders. The ShaderConfig will be deserialized from the node.json we updated. Ensure the JSON is valid and Serde will deserialize it correctly.

## Validate

```bash
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf
```

Or use the project's build command from the justfile: `just build-rv32` or equivalent.
