# LightPlayer

LightPlayer is a work-in-progress application for controlling visual effects on the esp32c6
microcontroller using GLSL shaders.

GLSL shaders are used to define the visual effects, which are just-in-time (JIT) compiled to native
RISC-V code on the target device.

The architecture is client-server, designed for headless operation on unattended devices,
controlled from a web UI, native app, or by api.

# Quick Start

To run the demo project:

```bash
# Clone the repo
git clone https://github.com/Yona-Appletree/lp2025.git
cd lp2025

# Initialize your development environment
scripts/dev-init.sh

# Run the demo
just demo

# Run other examples
just demo -- <example-name>
```

# On-device demo (ESP32-C6)

To flash firmware, push the `examples/basic` project over USB serial, and run it on real hardware:

```bash
just demo-esp32c6-host
```

You need an ESP32-C6 board connected by USB, the RISC-V target installed (the recipe runs `install-rv32-target`), and [`espflash`](https://github.com/esp-rs/espflash) on your `PATH` for flashing.

**Wiring (GPIO is hardcoded today):** connect a WS2812-class addressable strip (or other device driven the same way) to **GPIO 18** as the data line—**not** the `pin` field in `examples/basic/src/strip.output/node.json`, which the firmware does not use yet. The firmware initializes a buffer for **256** LEDs; the basic demo mapping uses **241** pixels, which fits in that limit.

For an empty flash and firmware only (no project push), use `just demo-esp32c6-standalone`.

# Development

To get started with development:

1. **Initialize the development environment:**

   ```bash
   scripts/dev-init.sh
   ```

   This will:
   - Check for required tools (Rust, Cargo, rustup, just)
   - Verify Rust version meets minimum requirements (1.90.0+)
   - Install the RISC-V target (`riscv32imac-unknown-none-elf`) if needed
   - Set up git hooks (pre-commit hook runs `just check`)

2. **Required tools:**
   - Rust toolchain (1.90.0 or later) - [Install Rust](https://rustup.rs/)
   - `just` - Task runner: `cargo install just` or via package manager

3. **Common development commands:**
   - `just fci` - Fix, check, build, and test the whole project. Do this before you submit a PR.
   - `just fci-app` - Fix, check, build, and test the application.
   - `just fci-glsl` - Fix, check, build, and test the GLSL compiler.

See `just --list` for all available commands.

# Repository Structure

## CLI (`lp-cli/`)

Command-line interface for creating projects, running dev server, and managing LightPlayer
projects. Includes debug UI and file watching capabilities.

## Firmware (`lp-fw/`)

- **`fw-core`** Core firmware abstractions (serial I/O, transport, logging infrastructure)
- **`fw-emu`** Firmware that runs in the RISC-V32 emulator for testing without hardware
- **`fw-tests`** Integration tests for firmware (emulator-based testing and USB serial tests)
- **`fw-esp32`** ESP32 firmware

### Running Firmware Tests

USB serial integration tests verify firmware behavior with real hardware:

```bash
# Run USB serial tests (requires connected ESP32)
cargo test --package fw-tests --features test_usb -- --ignored

# Run with debug output
DEBUG=1 cargo test --package fw-tests --features test_usb -- --ignored
```

See [`lp-fw/fw-tests/README.md`](lp-fw/fw-tests/README.md) for more details.

## Application Core (`lp-core/`)

- **`lp-engine`** Core rendering engine that executes shaders and manages nodes (fixtures,
  textures, outputs)
- **`lp-engine-client`** Client for `lp-engine`, handling state sync and local project view
- **`lp-server`** Server that manages projects and handles client connections
- **`lp-client`** Async client library for communicating with `lp-server`. Manages filesystem sync
  and project management.
- **`lp-model`** Data models and API definitions for projects, nodes, and server communication
- **`lp-shared`** Shared utilities for filesystem, logging, time, and transport

## GLSL Compiler (`lp-shader/`)

Full layout and commands: [`lp-shader/README.md`](lp-shader/README.md).

- **`lps-frontend`** GLSL → LPIR (via naga)
- **`lpir`** LightPlayer IR definitions
- **`lpvm-native`** LPIR → custom RV32 machine code (default on-device JIT)
- **`lpvm-cranelift`** LPIR → Cranelift → RISC-V machine code (reference backend)
- **`lpvm-wasm`** LPIR → WASM (browser / `wasm.q32` filetests)
- **`lps-q32`** Fixed-point Q16.16 types: `Q32` scalar, `Vec2Q32`–`Vec4Q32`, `Mat2Q32`–`Mat4Q32`,
  component-wise math helpers, constant encoding for compiler
- **`lps-shared`** Shared type and function-signature shapes for tests / exec helpers
- **`lps-diagnostics`** Error codes, spans, `GlslError`
- **`lpvm`** Runtime values and literal parsing (uses `glsl` parser fork where needed)
- **`lps-builtin-ids`** Generated enum of builtin function IDs
- **`lps-builtins`** Rust functions used by the generated code: fixed-point math, glsl builtins,
  lygia-inspired library of native glsl functions
- **`lps-builtins-emu-app`** RISC-V guest for running tests linked against builtins
- **`lps-builtins-gen-app`** Code generator for builtin function boilerplate
- **`lps-filetests`** Collection of tests for GLSL spec compliance and correctness
- **`lps-filetests-gen-app`** Generator for repetitive filetests (vector, matrices)
- **`lps-filetests-app`** Filetest runner binary
- **`lpfn-impl-macro`** Macros for builtin function implementations

## RISC-V Tooling (`lp-riscv/`)

- **`lp-riscv-emu`** RISC-V 32-bit emulator used for testing and development
- **`lp-riscv-emu-shared`** Shared types between emulator host and guest
- **`lp-riscv-emu-guest`** Guest-side runtime for emulated environment
- **`lp-riscv-emu-guest-test-app`** Test application for emulator guest
- **`lp-riscv-inst`** RISC-V instruction encoding/decoding utilities (no_std)
- **`lp-riscv-elf`** ELF file loading and linking utilities (std required)

## Other Directories

- **`examples/`** Example LightPlayer projects
- **`docs/`** Documentation, plans, and design notes
- **`scripts/`** Build scripts and development utilities

# Acknowledgments

LightPlayer would not be possible without the amazing work of these projects:

- **[Cranelift](https://cranelift.dev/)** - Fast, secure compiler
  backend ([forked](https://github.com/Yona-Appletree/lp-cranelift) to support 32-bit RISC-V and
  `no_std`)
- **[Naga](https://github.com/gfx-rs/wgpu/tree/main/naga)** - Shader IR and **`glsl-in`** GLSL
  frontend (used by `lps-frontend`)
- **[pp-rs](https://github.com/light-player/pp-rs)** - GLSL preprocessor fork, patched in
  **`[patch.crates-io]`** in the workspace `Cargo.toml` so naga `glsl-in` works on **`no_std`**
  targets
- **[glsl-parser](https://git.sr.ht/~hadronized/glsl)** - GLSL parser (
  [forked](https://github.com/light-player/glsl-parser) for spans)
- **[Lygia](https://github.com/patriciogonzalezvivo/lygia)** - Shader library (source for lpfn
  built-in functions)
- **[DirectXShaderCompiler](https://github.com/microsoft/DirectXShaderCompiler)** - HLSL compiler (
  compiler architecture inspiration)
- **[esp-hal](https://github.com/esp-rs/esp-hal)** - Pure Rust ESP32 bare metal HAL (used for ESP32
  firmware)
- **[GLSL Specification](https://github.com/KhronosGroup/GLSL)** - GLSL language reference
- **[RISC-V Instruction Set Manual](https://github.com/msyksphinz-self/riscv-isadoc)** - RISC-V
  architecture documentation
- **[RISC-V ELF psABI Specification](https://github.com/riscv-non-isa/riscv-elf-psabi-doc)** -
  RISC-V ABI documentation

... and many more not listed. Thank you to everyone in the open source community for your work.

Special thanks to @SeanConnell for his support and guidance throughout the development of
the project.
