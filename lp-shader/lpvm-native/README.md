# lpvm-native

A lightweight LPIR-to-RISC-V backend for LightPlayer, designed for embedded JIT compilation on resource-constrained targets like the ESP32-C6.

## Overview

`lpvm-native` (FastAlloc) compiles LightPlayer IR (LPIR) directly to RISC-V machine code without the heavy infrastructure of traditional compiler backends. It achieves **performance parity with Cranelift** while using significantly less memory and producing smaller binaries.

## Motivation

The original LightPlayer implementation used Cranelift for code generation. While Cranelift produces excellent code, its memory footprint is substantial for embedded targets:

- **Target constraints:** ESP32-C6 with 512KB RAM, 4MB flash
- **Cranelift overhead:** Complex interference graphs, heavy data structures, significant compile-time memory usage
- **Our solution:** A custom backend with a pool-based register allocator and straight-line emission pipeline

## Performance Results

Comparing `lpvm-native` against the Cranelift/wasmtime backend on ESP32-C6 @ 40MHz:

| Metric           | lpvm-native       | Cranelift/wasmtime  | Advantage              |
| ---------------- | -------------------- | ------------------- | ---------------------- |
| **Binary Size**  | ~1.64 MB (52% flash) | 2.38 MB (76% flash) | **31% smaller**        |
| **Compile Time** | ~565ms               | 1000ms              | **43% faster**         |
| **Runtime FPS**  | ~29-30 FPS           | ~29 FPS             | **Performance parity** |
| **Peak Memory**  | ~136 KB              | ~213 KB             | **36% less RAM**       |

The FastAlloc backend achieves **identical runtime performance** to Cranelift while maintaining significant advantages in binary size, compile time, and memory usage.

## Design

This backend is inspired by [Cranelift](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift) and [regalloc2](https://github.com/bytecodealliance/regalloc2), adapted for the constraints of embedded systems.

### Architecture Pipeline

```
LPIR (LightPlayer IR)
    │
    ▼
┌─────────────────┐
│  Lowering       │  lpir::Op → VInst (virtual instructions)
│  (lower.rs)     │  Region tree construction for control flow
└─────────────────┘
    │
    ▼
┌─────────────────┐
│  FastAlloc      │  VReg → PReg allocation
│  (fa_alloc/)    │  Pool-based allocator with backward walk
└─────────────────┘
    │
    ▼
┌─────────────────┐
│  Emission       │  VInst + Alloc → PInst → bytes
│  (rv32c/emit.rs) │  Direct machine code emission
└─────────────────┘
    │
    ▼
RISC-V machine code
```

### FastAlloc Register Allocator

The core innovation is a lightweight register allocator optimized for straight-line code regions:

**Key Techniques (inspired by regalloc2):**

- **Backward walk allocation:** Walks instructions in reverse, allocating registers for uses and freeing for defs
- **Pool-based register management:** LRU-spill with slot reuse instead of expensive interference graphs
- **Edit-list emission:** Records spill/reload edits during allocation, applied during code emission
- **Region tree dispatch:** Structured control flow handling without full SSA reconstruction

**Benefits over traditional allocators:**

| Technique           | FastAlloc               | Traditional (Cranelift) |
| ------------------- | ----------------------- | ----------------------- |
| Interference graph  | None (ITree eliminated) | Built and colored       |
| Spill slots         | Reused via pool         | Greedy eviction         |
| Compile-time memory | O(vregs) for pool       | O(vregs²) for graph     |
| Code quality        | Competitive             | Excellent               |

### VInst (Virtual Instructions)

The intermediate representation between LPIR and machine code:

- Compact `u16` virtual registers ([`VReg`](src/vinst.rs))
- RISC-V-oriented instruction set (IConst32, Add32, Load32, Store32, etc.)
- Symbol-based calls for deferred linking
- Source operand tracking for debug info

### Module Structure

| Module                         | Purpose                                  |
| ------------------------------ | ---------------------------------------- |
| [`fa_alloc/`](src/alloc/)   | FastAlloc register allocator             |
| [`rv32c/`](src/rv32c/)           | RISC-V instruction encoding and emission |
| [`abi/`](src/abi/)             | Calling convention and frame layout      |
| [`lower.rs`](src/lower.rs)     | LPIR → VInst lowering                    |
| [`emit.rs`](src/emit.rs)       | Emission orchestration                   |
| [`compile.rs`](src/compile.rs) | Module-level compilation                 |
| [`rt_jit/`](src/rt_jit/)       | JIT runtime for RISC-V targets           |
| [`rt_emu/`](src/rt_emu/)       | Emulation runtime for host testing       |

## Usage

### Compiling a Module

```rust
use lpvm_native::{compile_module, NativeCompileOptions};
use lpir::{LpirModule, FloatMode};
use lps_shared::LpsModuleSig;

// Compile LPIR to native code
let compiled = compile_module(
    &ir_module,
    &module_sig,
    FloatMode::Q32,           // or F32 for hardware float
    NativeCompileOptions::default(),
)?;

// Access compiled functions
for func in &compiled.functions {
    println!("{}: {} bytes", func.name, func.code.len());
}
```

### JIT Execution (on RISC-V target)

```rust
use lpvm_native::{link_jit, NativeJitEngine, NativeJitModule};

// Link compiled code into executable memory
let linked = link_jit(compiled_module, &builtins)?;

// Create JIT module and instance
let module = NativeJitEngine::new().load(linked)?;
let mut instance = module.instantiate(vmctx)?;

// Execute shader via direct call (zero per-pixel overhead)
let call_handle = module.direct_call("main")?;
instance.call_direct(&call_handle, &args, &mut ret_buf)?;
```

### Host Testing (with emulation)

Enable the `emu` feature for host-side testing with the RISC-V emulator:

```bash
cargo test -p lpvm-native --features emu
```

## Features

| Feature   | Description                                         |
| --------- | --------------------------------------------------- |
| `default` | Core `no_std` + alloc functionality                 |
| `debug`   | Debug info generation (increases binary size)       |
| `emu`     | Host emulation with `lp-riscv-emu` (requires `std`) |

## Validation

Required checks for changes to this crate:

```bash
# ESP32 build (on-device JIT)
cargo check -p fw-esp32 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    --features esp32c6,server

# Host tests with emulation
cargo test -p fw-tests --test scene_render_emu

# Allocator filetests
cargo test -p lpvm-native --test filetests
```

## Design Trade-offs

**Strengths:**

- Fast compilation (~565ms for 4KB GLSL)
- Low memory usage during compile
- Small runtime footprint
- Competitive runtime performance

**Limitations:**

- Optimized for straight-line code (shaders, not general programs)
- Simpler register allocation than graph coloring
- RV32IMAC target only

## See Also

- [`lpvm-cranelift`](../lpvm-cranelift/) - Cranelift-based backend (reference implementation)
- [`lpir`](../lpir/) - LightPlayer intermediate representation
- [Performance Reports](../../docs/design/native/perf-report/)

## License

Same as the LightPlayer project (see workspace LICENSE).
