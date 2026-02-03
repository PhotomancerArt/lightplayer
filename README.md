# LightPlayer

LightPlayer is a tool for controlling visual effects on 32-bit RISC-V microcontrollers
(such as esp32c6) and various linux and desktop platforms.

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

# Architecture

LightPlayer follows a client-server architecture designed for headless operation on embedded devices and desktop platforms. The system is built around a portable core that can run on various platforms, with platform-specific implementations for different deployment scenarios.

## Fixed-Point Math for Performance

LightPlayer uses **Q32 (Q16.16) fixed-point arithmetic** for shader execution on embedded devices. This provides significant performance benefits:

- **No Floating-Point Unit Required** - Many embedded microcontrollers (like ESP32-C6) lack hardware floating-point units. Fixed-point math uses only integer operations, which are fast and power-efficient.

- **Deterministic Performance** - Fixed-point operations have predictable execution times, making them ideal for real-time applications like LED control where frame timing is critical.

- **Precision** - The Q16.16 format provides 16 integer bits and 16 fractional bits (stored in a 32-bit integer), giving a range of -32768.0 to +32767.9999847412109375 with precision of approximately 0.00001526. This is sufficient for most visual effects while maintaining performance.

- **Code Size** - Fixed-point operations compile to fewer instructions than software floating-point emulation, reducing code size and improving cache efficiency.

The GLSL compiler automatically transforms floating-point operations in shaders to fixed-point equivalents, and provides optimized builtin functions (sin, cos, sqrt, etc.) implemented using efficient fixed-point algorithms.

## Core Application Architecture

The core application (`lp-core/`) provides the foundation for all LightPlayer implementations:

- **`lp-engine`** - The rendering engine executes GLSL shaders and manages the node graph (textures, shaders, fixtures, outputs). It handles frame-based rendering, texture sampling, shader compilation, and output data generation.

- **`lp-server`** - The server manages projects, handles client connections, and processes filesystem changes. It uses a tick-based API that works in both async and synchronous environments.

- **`lp-model`** - Defines the data models and message protocol for client-server communication. Includes project configurations, node definitions, and API types.

- **`lp-client`** - Async client library for communicating with `lp-server`. Provides transport abstraction (WebSocket, serial, local) and handles filesystem synchronization and project management operations.

- **`lp-engine-client`** - Higher-level client library that maintains a synchronized project view (`ClientProjectView`) with the server state. Handles incremental updates, node watching, and data retrieval for realtime visualization and control.

- **`lp-shared`** - Shared utilities including filesystem abstractions (`LpFs` trait), output providers, time providers, and transport traits. These abstractions enable platform-specific implementations while keeping core logic portable.

## Platform Implementations

### CLI (`lp-cli/`)

The command-line interface provides development tools and a local server:

- **Dev Server** - Runs `lp-server` with a local filesystem, WebSocket transport, and debug UI
- **File Watching** - Monitors project files and syncs changes to the server
- **Project Management** - Creates, initializes, and manages LightPlayer projects
- **Debug UI** - Visual interface for inspecting node states, outputs, and project structure

The CLI uses `lp-client` with WebSocket transport for local development, and can also connect to remote servers.

### Firmware ESP32 (`lp-fw/fw-esp32/`)

Bare-metal firmware for ESP32-C6 microcontrollers:

- **Bare-metal Operation** - Runs `lp-server` in a `no_std` environment using `esp-hal`
- **Serial Transport** - USB serial communication for client connections
- **LED Output** - Direct GPIO/RMT control for addressable LED strips (WS2812, etc.)
- **JIT Compilation** - Compiles GLSL shaders to RISC-V code using Cranelift JIT at runtime
- **Fixed-point Math** - Uses Q32 fixed-point arithmetic for shader execution (no floating-point unit)

The firmware uses `fw-core` abstractions for serial I/O and transport, with ESP32-specific implementations for hardware access.

### Firmware Emulator (`lp-fw/fw-emu/`)

Firmware implementation that runs in the RISC-V32 emulator for testing:

- **Host Testing** - Allows testing firmware logic without hardware
- **Emulator Integration** - Runs in `lp-riscv-emu` with simulated time and syscalls
- **Serial Emulation** - Emulates serial I/O through emulator syscalls
- **Integration Tests** - Enables comprehensive testing of the full firmware stack

### Firmware Core (`lp-fw/fw-core/`)

Shared firmware abstractions:

- **Serial I/O** - Abstract serial communication interface
- **Transport** - Serial-based transport implementation for client-server communication
- **Logging** - Platform-specific logging infrastructure (emulator syscalls, ESP32 `esp_println`)

## GLSL Compiler (`lp-glsl/`)

The GLSL compiler transforms GLSL shaders into executable RISC-V code:

- **Parsing** - Uses a forked `glsl-parser` to parse GLSL source code with span information
- **Transformation** - Converts GLSL to a simplified intermediate representation, handles fixed-point math (Q32), and applies optimizations
- **Code Generation** - Uses Cranelift to generate RISC-V32 machine code
- **JIT Compilation** - Compiles shaders at runtime on embedded devices using Cranelift's JIT backend
- **ELF Linking** - Can also generate ELF files for static linking and testing
- **Builtin Functions** - Provides a library of GLSL builtin functions (math, noise, color space) implemented in Rust

The compiler supports multiple execution modes:
- **HostJit** - JIT compilation on the host (for testing)
- **Emulator** - JIT compilation within the RISC-V emulator
- **ELF** - Static compilation to ELF files

## RISC-V Tooling (`lp-riscv/`)

Tools for working with RISC-V code:

- **`lp-riscv-emu`** - RISC-V 32-bit emulator used for testing and development. Supports instruction-level logging, memory access tracking, and syscall emulation. Can run in `no_std` mode or with `std` for host tooling.

- **`lp-riscv-elf`** - ELF file loading and linking utilities. Handles symbol resolution, relocation, and GOT (Global Offset Table) management for linking JIT-compiled code with builtin functions.

- **`lp-riscv-inst`** - Instruction encoding/decoding utilities for RISC-V instructions. Used by the emulator and compiler tooling.

- **`lp-riscv-emu-guest`** - Guest-side runtime for code running in the emulator. Provides syscall interface, memory management, and logging facilities.

## Cranelift Fork

LightPlayer uses a [forked version of Cranelift](https://github.com/Yona-Appletree/lp-cranelift) with modifications for embedded use:

- **32-bit RISC-V Support** - Full support for RISC-V 32-bit instruction set
- **No_std Compatibility** - Can run in `no_std` environments for embedded JIT compilation
- **Custom Targets** - Support for creating custom compilation targets (host JIT, emulator, ESP32)
- **Symbol Resolution** - Custom symbol lookup for builtin functions and host functions

The fork maintains compatibility with upstream Cranelift while adding the necessary features for embedded JIT compilation.

# Planned Work

Major features planned for future releases:

- **Web UI** - Browser-based interface for creating and managing LightPlayer projects, visualizing outputs, and controlling devices remotely

- **GPU Shader Execution** - Support for executing shaders on GPU hardware (OpenGL/Vulkan) for platforms with GPU capabilities, providing significant performance improvements

- **Floating Point Support** - Native floating-point arithmetic support for shaders, enabling more complex visual effects and reducing the complexity of fixed-point math

- **Input Device Support** - Integration with input devices (sensors, MIDI controllers, network events) to enable interactive and responsive visual effects

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
- **`fw-tests`** Integration tests for firmware (emulator-based testing)
- **`fw-esp32`** ESP32 firmware

## Application Core (`lp-core/`)

- **`lp-engine`** Core rendering engine that executes shaders and manages nodes (fixtures,
  textures, outputs)
- **`lp-engine-client`** Client for `lp-engine`, handling state sync and local project view
- **`lp-server`** Server that manages projects and handles client connections
- **`lp-client`** Async client library for communicating with `lp-server`. Manages filesystem sync
  and project management.
- **`lp-model`** Data models and API definitions for projects, nodes, and server communication
- **`lp-shared`** Shared utilities for filesystem, logging, time, and transport

## GLSL Compiler (`lp-glsl/`)

- **`lp-glsl-compiler`** Main GLSL compiler that parses, transforms, and codegens to various ISAs,
  handles JIT and ELF linking
- **`lp-glsl-builtins`** Rust functions used by the generated code: fixed-point math, glsl builtins,
  lygia-inspired libary of native glsl functions
- **`lp-glsl-builtins-emu-app`** RISC-V guest for running tests linked against builtins
- **`lp-glsl-builtins-gen-app`** Code generator for builtin function boilerplate
- **`lp-glsl-filetests`** Collection of tests for GLSL spec compliance and correctnees
- **`lp-glsl-filetests-gen-app`** Generator for repetative filetests (vector, matries)
- **`lp-glsl-filetests-app`** Filetest runner binary
- **`lp-glsl-jit-util`** Utilities for JIT compilation
- **`esp32-glsl-jit`** ESP32 proof-of-concept JIT compiler
- **`lp-glsl-q32-metrics-app`** Metrics tool for fixed-point math (q32)
- **`lpfx-impl-macro`** Macros for builtin function implementations

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
- **[glsl-parser](https://git.sr.ht/~hadronized/glsl)** - GLSL
  parser - ([forked](https://github.com/Yona-Appletree/glsl-parser) to support
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

