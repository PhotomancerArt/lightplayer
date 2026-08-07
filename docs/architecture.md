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
efficient fixed-point algorithms. Float mode selection is a backend parameter — the IR itself is
mode-agnostic, so the same lowered program can be emitted for either mode.

## Core and Application Layers

The engine internals live in `lp-core/` (`lpc-*` crates), and app-facing orchestration —
servers, clients, transports, the Studio — lives in `lp-app/` (`lpa-*` crates). See
[`lp-core/README.md`](../lp-core/README.md) and [`lp-app/README.md`](../lp-app/README.md)
for the full crate indexes; the load-bearing pieces:

- **`lpc-model`** - Shared core vocabulary: ids, paths, frame ids, node kinds, `LpValue` /
  `LpType`, slots, and the authored project/node definitions.

- **`lpc-engine`** - The runtime for one loaded project: node trees, resolver caches, buses,
  shader/runtime value conversion, and frame execution.

- **`lpc-wire`** - The engine/client wire contract: messages, tree deltas, project requests,
  transport errors, and partial state serialization.

- **`lpc-view`** - Client-side view/cache of one engine, built incrementally from `lpc-wire`
  updates — the basis for realtime visualization and control.

- **`lpa-server`** - Embeddable server layer that hosts engines, manages projects, and serves
  the `lpc-wire` API over app-provided transports. Tick-based, so it runs in both async and
  synchronous (bare-metal) environments.

- **`lpa-client`** - Client-side transport/API layer for talking to a LightPlayer server or
  firmware target (WebSocket, serial, local).

- **`lpa-studio-core` / `lpa-studio-web`** - The headless Studio application core (controllers,
  views, typed actions) and the Dioxus browser shell that renders it.

## Platform Implementations

### CLI (`lp-cli/`)

The command-line interface provides developer tools and a local server. It is designed to run from
the source checkout and may assume repository-relative assets such as checked-in board manifests;
it is not currently packaged as a user-facing deployable CLI:

- **Dev Server** - Runs `lp-server` with a local filesystem, WebSocket transport, and debug UI
- **File Watching** - Monitors project files and syncs changes to the server
- **Project Management** - Creates, initializes, and manages LightPlayer projects
- **Debug UI** - Visual interface for inspecting node states, outputs, and project structure
- **Hardware Manifests** - Interactive CRUD and validation for board profiles under
  `lp-core/lpc-hardware/boards`
- **Hardware Calibration** - Host-driven GPIO square-wave calibration that maps board-visible
  silkscreen labels to internal HAL GPIO addresses

The CLI uses `lp-client` with WebSocket transport for local development, and can also connect to
remote servers.

### Firmware ESP32 (`lp-fw/`)

Bare-metal firmware for three ESP32-family chips — `fw-esp32c6` (RISC-V), `fw-esp32s3`
(Xtensa LX7), and `fw-esp32v3` (classic ESP32, Xtensa LX6) — sharing a chip-generic layer
(`fw-esp32-common`). See [`lp-fw/README.md`](../lp-fw/README.md) for the full crate table.

- **Bare-metal Operation** - Runs the LightPlayer server in a `no_std` environment using
  `esp-hal`
- **Serial Transport** - USB serial (UART0 on the classic ESP32) for client connections
- **LED Output** - The multi-channel WS281x RMT driver (`lp-ws281x`), shared by all three
  chips; output channels are addressed through board-manifest endpoints
- **JIT Compilation** - Compiles GLSL shaders to the chip's own instruction set (RISC-V or
  Xtensa) at runtime via `lpvm-native`
- **Fixed-point Math** - Uses Q32 fixed-point arithmetic for shader execution (no
  floating-point unit required)

The firmware uses `fw-core` abstractions for serial I/O and transport, with chip-specific
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

## GLSL Compiler (`lp-shader/`)

The GLSL compiler transforms GLSL shaders into executable code for embedded and desktop targets.
See [`lp-shader/README.md`](../lp-shader/README.md) for the full crate index and commands, and
[`docs/design/lpir/`](design/lpir/) for the IR specification.

### Pipeline

```
GLSL source (#version 450 core)
  │
  ▼
lps-frontend           Naga glsl-in → IrModule
  │
  ▼
LPIR                    flat, scalarized, mode-agnostic IR
  │
  ├──► lpvm-native        → native machine code (RV32 + Xtensa, default on-device JIT)
  ├──► lpvm-cranelift     → native machine code (Cranelift, reference backend)
  ├──► lpvm-wasm          → .wasm (browser preview, wasm.q32 filetests)
  └──► lpir::interp       → in-process interpreter (testing)
```

**Naga** (`glsl-in`) parses GLSL 4.50 and type-checks it. **`lps-frontend`** walks Naga's
expression arena and lowers to **LPIR** — a flat, scalarized IR with structured control flow and
virtual registers. Lowering is mode-agnostic: Q32 vs float is a backend decision.

**LPIR** is LightPlayer's own intermediate representation. It acts as an anti-corruption layer so
the compiler core is written entirely in LightPlayer's terms, independent of Cranelift. Cranelift
only appears in **`lpvm-cranelift`**, the backend adapter. This gives decoupled testing (the
in-crate interpreter runs any LPIR program without Cranelift), multiple backends from one lowering,
and stable compiler internals across Cranelift version bumps.

### Backends

- **`lpvm-native`** — LPIR → machine code via LightPlayer's own lightweight codegen; the
  default on-device JIT. Two ISA backends: RISC-V 32-bit (`riscv32imac`, ESP32-C6) and
  Xtensa (ESP32-S3 / classic ESP32).

- **`lpvm-cranelift`** — LPIR → Cranelift → machine code; the reference backend. Supports any
  ISA Cranelift supports; host JIT uses `cranelift-native` for development and testing.
  Optional `glsl` feature pulls in `lps-frontend` for string-to-machine-code entry points.

- **`lpvm-wasm`** — LPIR → WASM via `wasm-encoder`. Browser preview backend; produces correct
  WASM for the web demo and `wasm.q32` filetests without requiring Cranelift.

- **`lpir::interp`** — Tree-walking interpreter inside the `lpir` crate. Runs LPIR directly for
  testing without invoking any backend.

### Builtins

GLSL math builtins (`sin`, `cos`, `sqrt`, `pow`, etc.), LPFX generative functions (noise, hash,
color space), and LPIR helpers are provided as `extern "C"` functions in **`lps-builtins`**.
Both Q32 (fixed-point) and f32 (float) implementations exist. The generator app
(**`lps-builtins-gen-app`**) scans builtin sources and emits:

- `BuiltinId` enum and mappings (`lps-builtin-ids`)
- Cranelift ABI glue (`lpvm-cranelift/src/generated_builtin_abi.rs`)
- WASM import types (`lps-wasm/src/emit/builtin_wasm_import_types.rs`)
- Dead-code-prevention refs for the RV32 emu app and WASM cdylib

### Filetests

Cranelift-style file-based tests under `lps-filetests/filetests/`. Each `.glsl` file declares
expected results; the harness compiles and executes on several backends:

- **wasm.q32** — WASM via `lpvm-wasm` + Wasmtime (the host execution target)
- **rv32c.q32** — RV32 via `lpvm-cranelift` object mode + `lp-riscv-emu`
- **rv32n.q32 / rv32lpn.q32** — RV32 via the hand-built `lpvm-native` backend + `lp-riscv-emu`
- **xtn.q32** — Xtensa via `lpvm-native`'s `isa/xt` backend + `lp-xt-emu`

Run with `./scripts/filetests.sh` or `just test-filetests`.

## Emulator Substrate (`lp-emu/`)

Architecture-neutral emulator infrastructure shared by the architecture emulators
(see `docs/adr/2026-07-28-emu-core-crate-family.md`):

- **`lp-emu-core`** - Host-side emulator machinery: guest memory model, run-loop result
  contract (`StepResult`/`TrapCode`), logging levels, cycle-cost accounting, serial, time
  control, and the host-side profiler (`std` feature). No cranelift or arch-crate
  dependencies; arch specifics are injected.

- **`lp-emu-abi`** - Host↔guest protocol: syscall numbers, guest serial framing, recovery
  handshake, JIT symbol entries.

## RISC-V Tooling (`lp-riscv/`)

Tools for working with RISC-V code:

- **`lp-riscv-emu`** - RISC-V 32-bit emulator used for testing and development. Supports
  instruction-level logging, memory access tracking, and syscall emulation. Can run in `no_std` mode
  or with `std` for host tooling. Builds on the arch-neutral machinery in `lp-emu-core`.

- **`lp-riscv-elf`** - ELF file loading and linking utilities. Handles symbol resolution,
  relocation, and GOT (Global Offset Table) management for linking JIT-compiled code with builtin
  functions.

- **`lp-riscv-inst`** - Instruction encoding/decoding utilities for RISC-V instructions. Used by the
  emulator and compiler tooling.

- **`lp-riscv-emu-guest`** - Guest-side runtime for code running in the emulator. Provides syscall
  interface, memory management, and logging facilities.

## Xtensa Tooling (`lp-xt/`)

The Xtensa counterpart to `lp-riscv/`, covering the ESP32-S3 (LX7) and classic ESP32 (LX6):
instruction model, encoder/decoder, and disassembler (`lp-xt-inst`), the windowed-register
emulator with per-board memory maps and an FPU proven bit-equal to real S3 silicon
(`lp-xt-emu`), and ELF loading (`lp-xt-elf`). See [`lp-xt/README.md`](../lp-xt/README.md).

## Cranelift Fork

LightPlayer uses a [forked version of Cranelift](https://github.com/PhotomancerArt/lp-cranelift)
with modifications for embedded use:

- **32-bit RISC-V Support** - `riscv32imac` code generation (upstream Cranelift only supports
  64-bit RISC-V)
- **`no_std`** - Supports `no_std` + alloc for both object and JIT compilation, enabling the
  compiler to run on bare-metal targets
- **regalloc2 fork** - Paired with a
  [forked regalloc2](https://github.com/PhotomancerArt/lp-regalloc2) with `ChunkedVec` for OOM
  mitigation and feature-gated ION allocator

The fork maintains compatibility with upstream Cranelift while adding the necessary features for
embedded JIT compilation.

# Where Decisions Live

This page is the standing overview; the design record is elsewhere. Accepted decisions live in
[`adr/`](adr/) (one file per decision, dated), known conditions in [`debt/`](debt/) and event
registries in [`defects/`](defects/), and forward-looking direction in [`roadmaps/`](roadmaps/)
and [`future/`](future/). When this page and an ADR disagree, the ADR wins — and this page
needs a fix.
