# LPVM - LightPlayer Virtual Machine

**Roadmap (milestones, three-layer `lps` / `lpir` / `lpvm` naming):**
[docs/roadmaps/2026-04-04-lpvm/overview.md](../../roadmaps/2026-04-04-lpvm/overview.md)

This document contains notes about a major refactor to separate the existing `GlslExecutable`
into `LpvmModule`, `LpvmInstance`, and `LpvmMemory` concepts.

These ideas are inspired by WASM's `Module`, `Instance`, and `Memory` concepts.

# Justification

The existing `GlslExecutable` and related `Riscv32Emulator` implementation combine complied code,
memory, and thread state into a single concept.

When introducing a VMContext concept for global variables, unifoms, fuel, and other thread-specific
state, it was hard to find a clear way to architect this idea.

The realization was made that because of `Riscv32Emulator` combining the above concepts, the
only place to put the VMContext was in the `GlslExecutable` itself, which is not in line with
the future goals for parallelism.

Additonally, this restriction is arbitray. There is no fundtamental reason why `Riscv32Emulator`
should be limited in this way, other than history.

# Implementation

LPVM will be implemented as a new set of crates, under the `lpvm/` directory. Code will be copied
from the existing crates as needed, and we will then migrate consumers to use the new crates.

# Nomenclature

Historically, we have used the "glsl" prefix for most concepts in the compiler and runtime.

This doesn't well separate compiler from runtime, and mixes frontend and backend concepts:
glsl is a language, independent of the runtime. Future frontends like WGSL would make this
confusing.

# Scope

LPVM is the runtime system for executing compiled LPIR. There are four main ways that LPIR
can be executed:

- JIT compilation to machine code using cranelift
- RV32 emulation using `riscv32-emu`
- WASM compilation using `wasmtime`
- Directly interpreted LPIR using `lpir::interp`

# Core Traits

Inspired by WASM's `Module`, `Instance`, and `Memory` concepts, the core traits for LPVM are:

- `LpvmMemory` -- represents the linear memory of the VM
- `LpvmModule` -- represents a compiled LPIR module
- `LpvmInstance` -- represents a running instance of a LPIR module

# Crates

---

# Milestones

## 2026-04-10: lpvm-native JIT On-Device Success

**Status:** First successful on-device execution of lpvm-native JIT backend on ESP32-C6.

The native JIT runtime (`rt_jit`) successfully executed on the ESP32-C6 hardware without issues. This represents a major milestone where the full GLSL-to-native-RISC-V compilation pipeline runs entirely on-device.

### Results

**Binary Size:**

- Firmware: 1,642,640 / 3,145,728 bytes (52.22% of available flash)
- This represents a significant reduction compared to previous wasmtime-based builds

**Compile Time:**

- Shader 3 (3877 bytes GLSL source): compiled in 584ms

**Runtime Performance:**

- Stable ~10 FPS execution
- Consistent frame timing over 180+ frames
- Native function calls working correctly

### Configuration

```
Target: riscv32imac-unknown-none-elf
Chip: ESP32-C6 (revision v0.1)
Clock: 40 MHz
Features: esp32c6,server
Backend: lpvm-native rt_jit
```

### Key Achievements

1. **First-try success:** No runtime issues encountered on device
2. **ROM size reduction:** Significant flash savings vs wasmtime backend
3. **Compile time:** Sub-second shader compilation on-device
4. **Stable FPS:** Consistent 10 FPS render rate

### Validation Commands

```bash
# ESP32 build with native JIT
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Flash and monitor
espflash flash --chip esp32c6 -T lp-fw/fw-esp32/partitions.csv target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32
espflash monitor --chip esp32c6
```

---

## 2026-04-10: lpvm-native vs Cranelift/Wasmtime Comparison

**Summary:** Side-by-side comparison of the new lpvm-native backend against the previous cranelift/wasmtime-based backend.

### Comparison Table

| Metric           | lpvm-native (new)        | cranelift/wasmtime (old) | Delta                 |
| ---------------- | ------------------------ | ------------------------ | --------------------- |
| **Binary Size**  | 1,642,640 bytes (52.22%) | 2,381,536 bytes (75.71%) | -738,896 bytes (-31%) |
| **Compile Time** | 584ms                    | 1000ms                   | -416ms (-42%)         |
| **Runtime FPS**  | ~10 FPS                  | ~29 FPS                  | -19 FPS (-66%)        |

### Analysis

**Trade-offs:**

- **Size:** lpvm-native is 31% smaller, freeing significant flash space
- **Compile:** lpvm-native compiles 42% faster (sub-second vs 1 second)
- **Runtime:** cranelift/wasmtime executes **3x faster** in FPS

**Key Takeaway:**
The lpvm-native backend achieves major wins in binary size and compile time, but at a significant runtime performance cost. The ~3x FPS difference suggests cranelift's code generation produces more optimized machine code than our current lpvm-native backend.

**However:** Filetest instruction counts show only ~18% slowdown (28,742 vs 24,402 inst) for the same shader pattern. This discrepancy suggests the performance gap may be in the **runtime layer** (value marshalling, instance overhead) rather than generated code quality.

### Mitigation Strategies (TODO)

1. **Investigate codegen differences:** Compare disassembly of hot paths between backends
2. **Profile shader execution:** Identify bottlenecks in lpvm-native generated code
3. **Optimize instruction selection:** Review VInst lowering for inefficient patterns
4. **Register allocation:** Greedy allocator may be suboptimal vs cranelift's allocator
5. **Builtin call overhead:** Native builtin dispatch may have higher overhead than wasmtime's

### Key Insight: Filetest vs On-Device Discrepancy

**Observation:** Filetests show only ~18% slowdown (28,742 vs 24,402 inst), but on-device shows ~66% FPS drop (10 vs 29 FPS).

This 3x on-device gap vs 1.2x instruction-count gap suggests the issue is **not** in code generation quality, but rather in the runtime layer.

---

## 2026-04-10: Performance Investigation Results

**Root Cause Found:** The native JIT runtime layer has significant per-call overhead that cranelift's `DirectCall` avoids.

### Comparison: Cranelift vs Native JIT Call Path

| Aspect                  | Cranelift (`DirectCall`)              | Native JIT (`NativeJitInstance`)                    |
| ----------------------- | ------------------------------------- | --------------------------------------------------- |
| **Function resolution** | Once at compile time (HashMap lookup) | Every call - linear search through `ir.functions`   |
| **Metadata lookup**     | Cached in `DirectCall` struct         | Every call - linear search through `meta.functions` |
| **Argument packing**    | Stack-allocated buffer                | `Vec::with_capacity()` heap allocation per call     |
| **Return buffer**       | Caller-provided stack buffer          | `alloc::vec![0i32; n]` heap allocation per call     |
| **Validation**          | Arg count only                        | Arg count, parameter types, return types            |

### Critical Findings

**1. O(n) Linear Search Per Pixel**

`NativeJitInstance::invoke_flat()` (instance.rs:27-98):

```rust
// Called for EVERY pixel - does linear search!
let idx = self.module.inner.ir.functions
    .iter()
    .position(|f| f.name == name)  // O(n) search by string
    .ok_or_else(...)?;
```

Same pattern in `call_q32()` - searches `meta.functions` by name on every call.

**2. Multiple Allocations Per Pixel**

```rust
// Every call allocates:
let mut full: Vec<i32> = Vec::with_capacity(1 + flat.len());  // arg packing
let mut sret_buf = alloc::vec![0i32; n_buf];                  // return buffer
```

**3. Duplicate Metadata Lookups**

`call_q32()` alone does **3 separate lookups**:

- `meta.functions.iter().find(|f| f.name == name)` for validation
- `ir.functions.iter().position(|f| f.name == name)` for invoke_flat
- Inside `invoke_flat()`: another `ir.functions` and `meta.functions` search

### Why Filetests Don't Show This

Filetests measure **instruction count within the shader**, not the runtime overhead. The per-call overhead (function lookup, allocation, validation) happens **outside** the shader's instruction stream, so it's not captured in the 28,742 vs 24,402 instruction counts.

### Recommended Fixes (Priority Order)

**P1: Cache function pointer at compile time**

- Add `NativeJitModule::direct_call(name)` that returns a resolved handle
- Cache function index, entry offset, and metadata in a struct
- Per-pixel call uses cached data - zero lookup overhead

**P2: Eliminate per-call allocations**

- Use stack-allocated fixed-size buffers (max 8 args like cranelift)
- Caller provides return buffer (like `call_i32_buf`)

**P3: Reduce validation overhead**

- Move validation to compile time
- Debug builds check, release builds skip

**P4: Pre-validate at instantiation**

- Cache parameter/return type info so `call_q32` doesn't search metadata

---

## 2026-04-10: Performance Fix Implemented

**Status:** All P1-P4 fixes implemented. **Result: 25 FPS achieved** (up from ~10 FPS, nearly matching cranelift's 29 FPS).

### Changes Made

**1. `NativeJitDirectCall` cached handle** (`module.rs`)

- New struct stores resolved `entry_offset`, `arg_count`, `ret_count`, `is_sret`
- `NativeJitModule::direct_call(name)` resolves once at compile time
- Zero per-call lookup overhead

**2. `NativeJitInstance::call_direct()`** (`instance.rs`)

- Takes `&NativeJitDirectCall` handle + stack buffers
- Uses `[i32; 8]` stack array for arg packing (no heap alloc)
- Caller provides `&mut [i32]` return buffer (no heap alloc)
- Direct assembly call with cached entry point

**3. `native_jit.rs` updated** (`lp-engine`)

- `NativeJitShader` now stores `Option<NativeJitDirectCall>`
- `render()` uses `call_direct()` with stack-allocated `[i32; 4]` return buffer
- ~240 pixels/frame × zero allocations = major overhead reduction

### Before vs After

| Aspect          | Before                                 | After                        |
| --------------- | -------------------------------------- | ---------------------------- |
| Function lookup | O(n) linear search × 3 per pixel       | Once at compile time         |
| Metadata lookup | O(n) linear search × 2 per pixel       | Cached in handle             |
| Arg packing     | `Vec::with_capacity()` heap alloc      | `[i32; 8]` stack array       |
| Return buffer   | `alloc::vec![0i32; n]` heap alloc      | Caller-provided stack buffer |
| Validation      | Full param/return type check per pixel | Compile-time only            |
| **FPS**         | ~10 FPS                                | **25 FPS**                   |

### Bug Fix: Register Packing

**Issue:** Initial implementation had off-by-one error in `pack_regs_sret_direct()`:

```rust
// WRONG - skipped vmctx, put user args in wrong registers
(sret, words[1], words[2], ...)

// CORRECT - vmctx to a1, user args to a2-a7
(sret, words[0], words[1], words[2], ...)
```

This caused x/y/time arguments to be shifted, resulting in "lines instead of 2D blobs" visualization artifacts and random time values.

### Memory Usage Comparison

Heap: 320 KB total

| Metric | lpvm-native | cranelift/wasmtime | Delta |
|--------|-------------|-------------------|-------|
| **Peak Usage** | ~130 KB used | ~213 KB used | -39% |
| **Peak Free** | 190,142 bytes | 107,243 bytes | +77% |
| **Final Free** | 308,987 bytes (94.3%) | 304,972 bytes (93.1%) | similar |

lpvm-native uses significantly less RAM during operation - only ~130KB at peak vs ~213KB for cranelift. This leaves more memory available for user shaders and larger LED configurations.

### Final Results

| Metric           | lpvm-native (fixed)   | cranelift/wasmtime    | Delta |
| ---------------- | --------------------- | --------------------- | ----- |
| **Binary Size**  | 1,642,640 bytes (52%) | 2,381,536 bytes (76%) | -31%  |
| **Compile Time** | ~620ms                | 1000ms                | -38%  |
| **Runtime FPS**  | **25 FPS**            | 29 FPS                | -14%  |
| **Peak Memory**  | ~130 KB               | ~213 KB               | -39%  |

**Success:** lpvm-native achieves 86% of cranelift's performance with 31% smaller binary, 38% faster compile, and 39% less peak memory usage.

### Files Modified

- `lp-shader/lpvm-native/src/rt_jit/module.rs` - Added `NativeJitDirectCall`
- `lp-shader/lpvm-native/src/rt_jit/instance.rs` - Added `call_direct()`, fixed register packing
- `lp-shader/lpvm-native/src/rt_jit/mod.rs` - Exported new type
- `lp-shader/lpvm-native/src/lib.rs` - Exported new type
- `lp-core/lp-engine/src/gfx/native_jit.rs` - Updated to use fast path

### Test Configuration

Both tests run on identical hardware with the same shader:

- Target: ESP32-C6 @ 40MHz
- Shader: 3877 bytes GLSL source
- LEDs: 241 (723 bytes output buffer)
