# Questions for Initial Firmware Plan

## Overview
This plan will create the initial firmware framework for LightPlayer with three components:
- `lp-core`: Shared application model (no_std, has alloc)
- `fw-esp32`: ESP32 firmware (no_std, esp-hal)
- `fw-host`: Host firmware simulator (std)

## Answers Summary

1. **JSON Serialization**: `serde` + `serde_json` with `alloc` feature (supports no_std)
2. **LED Driver**: Custom RMT implementation (from reference code)
3. **Filesystem**: `esp-storage` for flash access + `littlefs2` for filesystem + adapter layer
4. **Project Config**: Updated structure with `$type` discriminator, `nodes` structure, u32 IDs as strings in JSON
5. **Commands**: `$type` discriminator, `UpdateProject`, `GetProject`, `Log` commands
6. **Host Simulation**: `egui` for visualization
7. **Shader Compilation**: No caching, on-demand compilation, `ProjectRuntime` for status tracking
8. **Texture Formats**: OpenGL-style (`RGB8`, `RGBA8`, `R8`)
9. **Mapping**: Normalized coordinates, runtime calculation, basic averaging (kernel precomputation later)
10. **Error Handling**: `Result<T, E>` with custom error enums, `Display` for serialization
11. **Serial Protocol**: Extensible structure, simple `\n` termination for now
12. **File Storage**: Single `project.json` file, GLSL embedded in JSON

## Questions

### 1. JSON Serialization Library
**Question**: What JSON serialization library should we use for `lp-core` (no_std with alloc)?

**Options**:
- `serde` + `serde_json` (requires alloc, widely used)
- `serde_core` (no_std serde, but need to check JSON support)
- `miniserde` (no_std, smaller but less features)

**Context**: We need to serialize/deserialize the project config JSON and command messages. The workspace already uses `serde` with `alloc` feature.

**Suggested**: Use `serde` + `serde_json` with `alloc` feature since it's already in the workspace and provides the best compatibility.

---

### 2. LED Driver Library
**Question**: What library should we use for WS2812B LED output on ESP32?

**Options**:
- `esp-hal-smartled` (community crate, integrates with esp-hal)
- `smart-leds` + custom RMT driver (like in lpmini2024)
- Direct RMT implementation

**Context**: The reference implementation (`lpmini2024/apps/fw-esp32c3`) uses `smart-leds` with a custom RMT driver. ESP32-C6 also has RMT peripheral.

**Suggested**: Start with `esp-hal-smartled` if available for ESP32-C6, otherwise use `smart-leds` + custom RMT driver similar to reference.

---

### 3. Filesystem Integration
**Question**: How should we integrate LittleFS with esp-hal for shader storage?

**Options**:
- Use `littlefs2` crate directly with esp-hal's Flash
- Use `esp-storage` as a bridge layer
- Create custom block device trait implementation

**Context**: We discussed using `littlefs2` earlier. Need to determine the integration approach with esp-hal's flash API.

**Suggested**: Start with `littlefs2` and create a block device trait implementation that wraps esp-hal's Flash. This keeps the abstraction clean.

---

### 4. Project Config JSON Structure
**Question**: What is the exact JSON structure for the project config file?

**Current understanding**:
```json
{
  "outputs": {
    "string": { "gpio_pin": number, "count": number, "chip": "ws2812b" }
  },
  "textures": {
    "string": { "width": number, "height": number, "type": "R8G8B8A8" }
  },
  "shaders": {
    "string": { "glsl": "string", "output_shades": number }
  },
  "fixtures": {
    "string": { "output_id": "string", "mapping": [{ "pixel": number, "point": [x, y], "radius": number }] }
  }
}
```

**Questions**:
- Should `output_shades` be in shaders or fixtures?
- What texture formats should we support initially? (R8G8B8A8, R8G8B8, etc.)
- Should `point` be `[x, y]` or `{x, y}`?
- Do we need any other fields (e.g., scene config, timing)?

**Suggested**: Confirm the exact structure, but start with minimal fields and extend as needed.

---

### 5. Command Protocol Format
**Question**: What is the exact format for the JSON command messages?

**Current understanding**:
- Commands are `\n` terminated JSON messages
- Commands needed:
  - `load_project`: Load project config from filesystem
  - `store_project`: Save project config to filesystem  
  - `log`: Device log message `{ "level": "info|warn|error", "message": "string" }`

**Questions**:
- Should commands have a `type` field? e.g., `{ "type": "load_project", ... }`
- Should responses be JSON? What format?
- Do we need request/response IDs for async handling?
- Should `log` be a command or a separate message type?

**Suggested**: Use a `type` field for commands. Responses can be simple JSON with `{ "ok": true }` or `{ "error": "message" }`. Logs are one-way messages.

---

### 6. Host Simulation Library
**Question**: What library should we use for LED visualization in the host firmware?

**Options**:
- `winit` + `pixels` (window + pixel buffer)
- `egui` (immediate mode GUI, easier but heavier)
- Simple terminal output (colored circles/characters)
- `minifb` (minimal framebuffer)

**Context**: We need to visualize LEDs as circles or similar in a window. Should be simple and lightweight.

**Suggested**: Start with `minifb` or `winit` + `pixels` for simple window + pixel rendering. Terminal output could work for initial testing but window is better for development.

---

### 7. Shader Compilation Integration
**Question**: How should we integrate the existing `lp-glsl-compiler` compiler into the firmware?

**Context**: `lp-glsl-compiler` already exists and can compile GLSL to machine code. We need to:
- Store GLSL source in filesystem
- Load and compile on demand
- Execute compiled shaders

**Questions**:
- Should we cache compiled shaders in filesystem?
- How do we handle shader errors (compile-time vs runtime)?
- Should shaders be compiled at project load time or on-demand?

**Suggested**: Start with on-demand compilation. Cache compiled shaders in filesystem later. Use existing `lp-glsl-compiler` compiler as-is.

---

### 8. Texture Format Support
**Question**: What texture formats should we support initially?

**Options**:
- R8G8B8A8 (32-bit RGBA)
- R8G8B8 (24-bit RGB)
- R8 (8-bit grayscale)
- Others?

**Context**: Textures are used as shader inputs. Need to determine storage format and how to pass to shaders.

**Suggested**: Start with R8G8B8A8 (most common) and R8G8B8. Add others as needed.

---

### 9. Mapping Implementation Detail
**Question**: How detailed should the initial fixture mapping implementation be?

**Current understanding**:
- Mapping: `Array<{ pixel, point, radius }>`
- Maps shader output pixels to LED positions

**Questions**:
- Should `point` be in normalized coordinates (0-1) or absolute?
- How do we handle `radius`? Is it a sampling radius for anti-aliasing?
- Do we need to support multiple fixtures per output?
- Should mapping be precomputed or calculated at runtime?

**Suggested**: Start simple - `point` in normalized coordinates, `radius` for simple sampling. Precompute mapping at project load time.

---

### 10. Error Handling Strategy
**Question**: How should we handle errors across the three components?

**Context**: Need consistent error handling between:
- `lp-core` (no_std, alloc)
- `fw-esp32` (no_std, esp-hal)
- `fw-host` (std)

**Questions**:
- Use `Result<T, E>` with custom error types?
- Use `anyhow` for host, custom errors for embedded?
- How do we serialize errors for JSON responses?

**Suggested**: Use `Result<T, E>` with custom error enums in `lp-core`. Implement `Display` for serialization. Use `anyhow` only in `fw-host` if needed.

---

### 11. Serial Communication Protocol
**Question**: What serial port settings and protocol details do we need?

**Context**: ESP32 uses serial port, host uses stdio.

**Questions**:
- Baud rate? (115200 is common)
- Do we need flow control?
- How do we handle partial messages?
- Should we use a framing protocol (e.g., length prefix) or rely on `\n`?

**Suggested**: Start with 115200 baud, `\n` termination. Add length prefix if we encounter issues with partial messages.

---

### 12. Project File Storage
**Question**: How should project files be stored in the filesystem?

**Context**: Need to store:
- Project config JSON
- Shader GLSL source files
- Possibly compiled shaders

**Questions**:
- Single project.json file or separate files per resource?
- File naming convention?
- Directory structure?

**Suggested**: Start with flat structure:
- `project.json` - main config
- `shaders/<name>.glsl` - shader sources
- `shaders/<name>.bin` - compiled shaders (optional cache)

