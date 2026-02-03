# LightPlayer

LightPlayer is a tool for controlling visual effects on microcontrollers (and larger computers).

GLSL shaders are used to define the visual effects, which are JIT-compiled to native RISC-V
code on the target device.

## Quick Start

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

## Development

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

The repository is organized into several main areas:

## CLI (`lp-cli/`)

- Command-line interface for creating projects, running dev server, and managing LightPlayer
  projects
- Includes debug UI and file watching capabilities

## Firmware (`lp-fw/`)

- **`fw-core`** - Core firmware abstractions (serial I/O, transport, logging infrastructure)
- **`fw-emu`** - Firmware that runs in the RISC-V32 emulator for host testing
- **`fw-esp32`** - ESP32 firmware
- **`fw-tests`** - Integration tests for firmware (emulator-based testing)

## Application Core (`lp-core/`)

- **`lp-engine`** - Core rendering engine that executes shaders and manages nodes (fixtures,
  textures, outputs)
- **`lp-server`** - Server that manages projects and handles client connections
- **`lp-client`** - Standalone client library for communicating with LpServer (transport layer,
  async client API)
- **`lp-engine-client`** - Higher-level client library for interacting with the engine (project
  view, state management)
- **`lp-model`** - Data models and API definitions for projects, nodes, and server communication
- **`lp-shared`** - Shared utilities for filesystem, logging, time, and transport

## GLSL Compiler (`lp-glsl/`)

- **`lp-glsl-compiler`** - Main GLSL compiler that parses, transforms, and codegens to RISC-V
- **`lp-glsl-builtins`** - Built-in GLSL functions (math, noise, color space conversions)
- **`lp-glsl-builtins-emu-app`** - RISC-V executable containing builtin functions for emulator
- **`lp-glsl-builtins-gen-app`** - Code generator for builtin function boilerplate
- **`lp-glsl-filetests`** - Test framework for GLSL spec compliance
- **`lp-glsl-filetests-gen-app`** - Generator for some repetative filetests (vector, matries)
- **`lp-glsl-filetests-app`** - Filetest runner binary
- **`lp-glsl-jit-util`** - Utilities for JIT compilation
- **`esp32-glsl-jit`** - ESP32 proof-of-concept JIT compiler
- **`lp-glsl-q32-metrics-app`** - Metrics tool for fixed-point math (q32)
- **`lpfx-impl-macro`** - Macros for builtin function implementations

## RISC-V Tooling (`lp-riscv/`)

- **`lp-riscv-inst`** - RISC-V instruction encoding/decoding utilities (no_std)
- **`lp-riscv-emu`** - RISC-V 32-bit emulator runtime (no_std + optional std)
- **`lp-riscv-elf`** - ELF file loading and linking utilities (std required)
- **`lp-riscv-emu-shared`** - Shared types between emulator host and guest
- **`lp-riscv-emu-guest`** - Guest-side runtime for emulated environment
- **`lp-riscv-emu-guest-test-app`** - Test application for emulator guest

## Other Directories

- **`examples/`** - Example LightPlayer projects
- **`docs/`** - Documentation, plans, and design notes
- **`scripts/`** - Build scripts and development utilities

## Acknowledgments

LightPlayer would not be possible without the amazing work of these projects:

- **[Cranelift](https://cranelift.dev/)** - Fast, secure compiler
  backend ([homepage](https://cranelift.dev/), [GitHub](https://github.com/bytecodealliance/cranelift)) - [lightplayer's RISC-V 32 fork](https://github.com/Yona-Appletree/lp-cranelift)
- **[glsl-parser](https://git.sr.ht/~hadronized/glsl)** - GLSL
  parser - [lightplayer's fork](https://github.com/Yona-Appletree/glsl-parser) (enhanced to support
  spans)
- **[Lygia](https://github.com/patriciogonzalezvivo/lygia)** - Shader library (source for lpfx
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
