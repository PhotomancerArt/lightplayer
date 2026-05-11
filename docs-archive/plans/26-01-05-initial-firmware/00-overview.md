# Initial Firmware Plan Overview

## Goal

Create the initial firmware framework for LightPlayer with three components:
- `lp-core`: Shared application model (no_std, has alloc)
- `fw-esp32`: ESP32 firmware (no_std, esp-hal)
- `fw-host`: Host firmware simulator (std)

The main goal is to establish the basic framework for a lightplayer firmware and pave the way for a simple scene running a shader on real hardware and LEDs, along with the host firmware to establish the abstraction (minimize device-specific code) and allow testing on host to work out issues in project setup, etc.

## Key Requirements

- Send and receive `\n` terminated JSON control messages via serial/stdio
- Store shaders on the filesystem
- Compile shaders and run them
- Map shader output to LEDs and send the data to LED hardware
- Simple "project" config file that maps to JSON with:
  - `outputs`: Map<string, { type: "gpio_strip", chip: "ws2812", gpio_pin, count }>
  - `textures`: Map<string, { type: "Memory", size: [width, height], format: "RGB8"|"RGBA8"|"R8" }>
  - `shaders`: Map<string, { type: "Single", glsl: string, texture_id: u32 }>
  - `fixtures`: Map<string, { type: "circle-list", output_id: string, channel_order: "rgb", mapping: Array<{ channel, center: [x, y], radius }> }>

## Initial Commands

- `UpdateProject`: Load & store the project
- `GetProject`: Retrieve the current project
- `Log`: Log messages from device { level, message }

## Technical Decisions

- **JSON**: `serde` + `serde_json` with `alloc` feature (no_std compatible)
- **LED Driver**: Custom RMT implementation
- **Filesystem**: `esp-storage` + `littlefs2` + adapter layer
- **Host Visualization**: `egui`
- **Texture Formats**: OpenGL-style (`RGB8`, `RGBA8`, `R8`)
- **Error Handling**: `Result<T, E>` with custom error enums, `Display` for serialization
- **Communication**: Extensible transport trait (JSON send/receive), simple `\n` termination for now
- **Storage**: Single `project.json` file, GLSL embedded in JSON

## Architecture

The architecture follows a layered approach:
1. **lp-core**: Platform-agnostic core with data structures, traits, and utilities
2. **fw-esp32**: ESP32-specific implementations of traits (filesystem, transport, LED output)
3. **fw-host**: Host-specific implementations for development/testing

This allows maximum code reuse and easy testing on host before deploying to hardware.

