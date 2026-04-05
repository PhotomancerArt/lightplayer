# M4: `lpvm-rv32`

## Goal

Refactor `lp-riscv-emu` to support the Module/Memory/Instance separation, then
build the `lpvm-rv32` backend wrapper. This is the hardest backend milestone
because the emulator's internal architecture must change.

## Context for Agents

### Current `Riscv32Emulator` architecture

`Riscv32Emulator` in `lp-riscv-emu` owns everything in one struct:

- **Registers**: `regs: [i32; 32]`, `pc: u32`
- **Memory**: `Memory` struct containing `code: Vec<u8>` + `ram: Vec<u8>` with
  fixed address mappings (code at `0x0`, RAM at `0x80000000`)
- **Execution state**: `instruction_count`, `traps`, `log_level`, `log_buffer`,
  `serial_host`, `time_mode`
- With `std`: `start_time`, `alloc_tracer`

Construction: `Riscv32Emulator::new(code: Vec<u8>, ram: Vec<u8>)` — takes
ownership of both code and RAM, builds `Memory::with_default_addresses(code, ram)`.

### Why this doesn't fit Module/Memory/Instance

- **No code sharing**: each emulator instance owns its own copy of code bytes.
  You can't create two instances from the same compiled module without cloning.
- **Memory and code are fused**: `Memory` struct contains both read-only code
  and mutable RAM in one object. There's no way to have shared code with
  per-instance RAM.
- **No reset**: to re-run, you create a new emulator from scratch. There's no
  way to reset execution state while keeping the same code.

### Current emulator consumers

The emulator is used beyond LPVM:

- `fw-tests` — firmware integration tests
- `lp-riscv-elf` — ELF loading tests
- `lp-riscv-emu-guest-test-app` — guest test binary
- `lp-cli` — memory profiling
- `lp-client` — emulator transports
- `lpir-cranelift` (riscv32-emu feature) — `glsl_q32_call_emulated`

All existing consumers must continue to work after the refactor.

### How the RV32 backend works in filetests today

`LpirRv32Executable` in **`lps-filetests`** (path may still be `lp-glsl-filetests`):

1. GLSL → **`lps-naga`** → `IrModule` + module metadata (e.g. `LpvmModuleMeta` /
   transitional `GlslModuleMeta`)
2. `object_bytes_from_ir(&ir, &options)` — compile LPIR to RV32 object file
   via Cranelift (targeting riscv32)
3. `link_object_with_builtins(&object_bytes)` — link with builtins → `ElfLoadInfo`
4. Per call: `glsl_q32_call_emulated(&load, &ir, &meta, &options, name, &args)`
   — creates a fresh emulator, loads ELF, runs function

Note: steps 2-3 use Cranelift to compile to an object file (not JIT). This is
distinct from `lpvm-cranelift` which does JIT compilation. `lpvm-rv32` handles
the "compile to object → link → emulate" path.

## Part 1: Refactor `lp-riscv-emu`

### Target architecture

Separate the emulator into three conceptual layers:

1. **Code image (module-like)**: Read-only compiled code bytes + trap map +
   entry point metadata. Can be shared across instances. Does not own mutable
   state.

2. **Memory (per-instance)**: Mutable RAM. The emulator's `Memory` struct
   already distinguishes code vs RAM internally — formalize this split. Code
   is referenced (borrowed or `Arc`), RAM is owned per-instance.

3. **Execution state (per-instance)**: Registers, PC, instruction count, traps,
   logging, serial, timing. Bound to one memory + one code image.

### Key design decisions

**Code access pattern**: `fetch_instruction` currently reads from either code
or RAM (to support JIT-in-RAM). This must continue to work. Options:

- Code image is mapped at a fixed address in the instance's memory view
  (read-only region) — same as today, but the backing storage is shared.
- The execution loop checks code image first, then RAM, using address ranges.

**Constructor changes**: Instead of `new(code, ram)` which takes ownership of
everything, provide something like:

- `new(code_image: &CodeImage, ram_size: usize)` — borrows code, allocates RAM
- Or `new(code_image: Arc<CodeImage>, ram: Vec<u8>)` — shared code, owned RAM

**Backward compatibility**: Provide a convenience constructor or builder that
matches the old `new(code, ram)` signature for existing consumers. Internally
it creates a code image + RAM and wires them together.

### What to watch out for

- **Self-modifying code / JIT-in-RAM**: The emulator supports executing code
  from RAM (not just the code region). This is used for on-device JIT where
  compiled code lives in RAM. The refactored memory model must preserve this
  capability.
- **Memory address layout**: Code at `0x0`, RAM at `0x80000000`. This is
  hardcoded in `Memory::with_default_addresses`. The refactored version should
  keep these defaults.
- **Trap table**: Currently part of the emulator construction. It describes
  addresses where traps are installed. This is code-level metadata and belongs
  with the code image (module), not the instance.

## Part 2: Build `lpvm-rv32`

### Crate location

`lpvm/lpvm-rv32/`

### Dependencies

```toml
[dependencies]
lpvm = { path = "../lpvm", default-features = false }
lpir = { path = "../../lp-shader/lpir", default-features = false }
lp-riscv-emu = { path = "../../lp-riscv/lp-riscv-emu" }
lp-riscv-elf = { path = "../../lp-riscv/lp-riscv-elf" }
cranelift-codegen = { ..., default-features = false }  # for object compilation
cranelift-object = { ... }
cranelift-module = { ... }
lps-builtins = { ... }  # for linking (path may still be lp-glsl-builtins)
```

Note: `lpvm-rv32` needs Cranelift for compiling LPIR to RV32 object code. This
is the `cranelift-object` path, not the `cranelift-jit` path. It's a different
use of Cranelift than `lpvm-cranelift`.

### Trait implementation mapping

| LPVM trait     | RV32 implementation                            | Notes                                                  |
|----------------|------------------------------------------------|--------------------------------------------------------|
| `LpvmModule`   | Compiled RV32 object (ELF) + linked code image | LPIR → Cranelift → RV32 object → ELF link → code image |
| `LpvmInstance` | Emulator execution state + RAM                 | Wraps the refactored `Riscv32Emulator`                 |
| `LpvmMemory`   | Emulator RAM                                   | The mutable portion of the emulator's memory           |

### Compilation flow

1. Take `IrModule` (LPIR)
2. Compile to RV32 object bytes via Cranelift (targeting riscv32)
3. Link with builtins → ELF
4. Extract code image from ELF → `LpvmModule`

This logic currently lives in `lpir-cranelift` behind the `riscv32-emu` feature
(`object_bytes_from_ir`, `link_object_with_builtins`). Move it to `lpvm-rv32`.

## Unit Tests

- Compile a simple LPIR module to RV32 object
- Link and create a module
- Instantiate with memory
- Call a function via trait interface
- Verify return values
- Test multiple instances from one module (the key new capability)
- Test VMContext passing

## What NOT To Do

- Do NOT break existing emulator consumers. Provide backward-compatible APIs.
- Do NOT remove JIT-in-RAM support from the emulator.
- Do NOT update **`lps-filetests`** to use `lpvm-rv32` yet. That's M5.
- Do NOT merge `lp-riscv-emu` into `lpvm-rv32`. The emulator is a
  general-purpose tool; `lpvm-rv32` wraps it.

## Done When

- `lp-riscv-emu` refactored: code image separate from mutable RAM and
  execution state
- Existing emulator consumers still work (fw-tests, ELF loading, etc.)
- `lpvm-rv32` crate exists at `lpvm/lpvm-rv32/`
- `LpvmModule`/`LpvmInstance`/`LpvmMemory` implemented
- Unit tests pass
- Workspace builds pass, including `fw-tests`
