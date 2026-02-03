# Architecture

LightPlayer follows a client-server architecture designed for headless operation on embedded devices
and desktop platforms. The system is built around a portable core that can run on various platforms,
with platform-specific implementations for different deployment scenarios.

## Fixed-Point Math for Performance

LightPlayer uses **Q32 (Q16.16) fixed-point arithmetic** for shader execution on embedded devices.
This provides significant performance benefits:

- **No Floating-Point Unit Required** - Many embedded microcontrollers (like ESP32-C6) lack hardware
  floating-point units. Fixed-point math uses only integer operations, which are fast and
  power-efficient.

- **Deterministic Performance** - Fixed-point operations have predictable execution times, making
  them ideal for real-time applications like LED control where frame timing is critical.

- **Precision** - The Q16.16 format provides 16 integer bits and 16 fractional bits (stored in a
  32-bit integer), giving a range of -32768.0 to +32767.9999847412109375 with precision of
  approximately 0.00001526. This is sufficient for most visual effects while maintaining
  performance.

- **Code Size** - Fixed-point operations compile to fewer instructions than software floating-point
  emulation, reducing code size and improving cache efficiency.

The GLSL compiler automatically transforms floating-point operations in shaders to fixed-point
equivalents, and provides optimized builtin functions (sin, cos, sqrt, etc.) implemented using
efficient fixed-point algorithms.

## Core Application Architecture

The core application (`lp-core/`) provides the foundation for all LightPlayer implementations:

- **`lp-engine`** - The rendering engine executes GLSL shaders and manages the node graph (textures,
  shaders, fixtures, outputs). It handles frame-based rendering, texture sampling, shader
  compilation, and output data generation.

- **`lp-server`** - The server manages projects, handles client connections, and processes
  filesystem changes. It uses a tick-based API that works in both async and synchronous
  environments.

- **`lp-model`** - Defines the data models and message protocol for client-server communication.
  Includes project configurations, node definitions, and API types.

- **`lp-client`** - Async client library for communicating with `lp-server`. Provides transport
  abstraction (WebSocket, serial, local) and handles filesystem synchronization and project
  management operations.

- **`lp-engine-client`** - Higher-level client library that maintains a synchronized project view (
  `ClientProjectView`) with the server state. Handles incremental updates, node watching, and data
  retrieval for realtime visualization and control.

- **`lp-shared`** - Shared utilities including filesystem abstractions (`LpFs` trait), output
  providers, time providers, and transport traits. These abstractions enable platform-specific
  implementations while keeping core logic portable.

## Platform Implementations

### CLI (`lp-cli/`)

The command-line interface provides development tools and a local server:

- **Dev Server** - Runs `lp-server` with a local filesystem, WebSocket transport, and debug UI
- **File Watching** - Monitors project files and syncs changes to the server
- **Project Management** - Creates, initializes, and manages LightPlayer projects
- **Debug UI** - Visual interface for inspecting node states, outputs, and project structure

The CLI uses `lp-client` with WebSocket transport for local development, and can also connect to
remote servers.

### Firmware ESP32 (`lp-fw/fw-esp32/`)

Bare-metal firmware for ESP32-C6 microcontrollers:

- **Bare-metal Operation** - Runs `lp-server` in a `no_std` environment using `esp-hal`
- **Serial Transport** - USB serial communication for client connections
- **LED Output** - Direct GPIO/RMT control for addressable LED strips (WS2812, etc.)
- **JIT Compilation** - Compiles GLSL shaders to RISC-V code using Cranelift JIT at runtime
- **Fixed-point Math** - Uses Q32 fixed-point arithmetic for shader execution (no floating-point
  unit)

The firmware uses `fw-core` abstractions for serial I/O and transport, with ESP32-specific
implementations for hardware access.

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
- **Transformation** - Converts GLSL to a simplified intermediate representation, handles
  fixed-point math (Q32), and applies optimizations
- **Code Generation** - Uses a custom 32-bit fork of Cranelift to generate RISC-V32 machine code
- **JIT Compilation** - Compiles shaders at runtime on embedded devices using Cranelift's JIT
  backend
- **ELF Linking** - Can also generate ELF binaries for static linking and testing
- **Builtin Functions** - Provides a library of GLSL builtin functions (math, noise, color space)
  implemented in Rust, inspired by Lygia

The compiler supports multiple execution modes:

- **HostJit** - JIT compilation on the host (for testing)
- **Emulator** - JIT compilation within the RISC-V emulator
- **ELF** - Static compilation to ELF files

## RISC-V Tooling (`lp-riscv/`)

Tools for working with RISC-V code:

- **`lp-riscv-emu`** - RISC-V 32-bit emulator used for testing and development. Supports
  instruction-level logging, memory access tracking, and syscall emulation. Can run in `no_std` mode
  or with `std` for host tooling.

- **`lp-riscv-elf`** - ELF file loading and linking utilities. Handles symbol resolution,
  relocation, and GOT (Global Offset Table) management for linking JIT-compiled code with builtin
  functions.

- **`lp-riscv-inst`** - Instruction encoding/decoding utilities for RISC-V instructions. Used by the
  emulator and compiler tooling.

- **`lp-riscv-emu-guest`** - Guest-side runtime for code running in the emulator. Provides syscall
  interface, memory management, and logging facilities.

## Cranelift Fork

LightPlayer uses a [forked version of Cranelift](https://github.com/Yona-Appletree/lp-cranelift)
with modifications for embedded use:

- **32-bit RISC-V Support** - Support for RISC-V `imac` 32-bit instruction set
- **`no_std`** - Supports `no_std` for both object and JIT compilation

The fork maintains compatibility with upstream Cranelift while adding the necessary features for
embedded JIT compilation.

# Planned Work

Major features planned for future releases:

- **Web UI** - Browser-based interface for creating and managing LightPlayer projects, visualizing
  outputs, and controlling devices remotely

- **GPU Shader Execution** - Support for executing shaders on GPU hardware (OpenGL/Vulkan) for
  platforms with GPU capabilities, providing significant performance improvements

- **Floating Point Support** - Native floating-point arithmetic support for shaders, enabling more
  complex visual effects and reducing the complexity of fixed-point math

- **Input Device Support** - Integration with input devices (sensors, MIDI controllers, network
  events) to enable interactive and responsive visual effects
