# lpvm-native overview

This document describes the design of `lpvm-native`, a lightweight backend that compiles LPIR directly
to native machine code. It replaces Cranelift's general-purpose compiler infrastructure with a
purpose-built pipeline tuned for LPIR's constraints: structured control flow, scalarized operations,
moderate per-function size (still tiny next to a general-purpose compiler), and a fixed set of
target ISAs.

The document is intentionally ahead of the implementation. It captures the intended shape of the
system so that incremental work stays on track. It will be updated as the design is validated.

## Motivation

Cranelift works. It compiles LPIR to correct RV32 machine code on ESP32-C6. The problem is resource
consumption:

- **Peak compile RAM**: ~50–100 KB. regalloc2's interference graph and union-find structures
  dominate. On a 500 KB device, this limits shader complexity.
- **Binary size**: ~150–230 KB of ROM for the backend. At 76% flash utilization, this constrains
  what else can ship in firmware.
- **Compile time**: ~5–10 ms per function. Acceptable today, but limits headroom for larger shaders
  or tighter frame budgets.

We already forked regalloc2 (`ChunkedVec`), disabled the optimizer and verifier, and applied LTO.
There is no path to a 10x reduction without replacing the allocator, and at that point most of
Cranelift's value is gone.

LPIR's properties—structured control flow, non-SSA vregs, scalarized scalar types, and per-function
bodies that are small compared to whole translation units—enable algorithms that are dramatically
simpler than what a general-purpose compiler requires. A custom backend can exploit these
constraints.

## Pipeline

```
IrModule (from lps-frontend or lpir parser)
  │
  ├─ per function:
  │    │
  │    ▼
  │  lower.rs ── LPIR Op sequence → VInst sequence
  │    │
  │    ▼
  │  regalloc  ── assign physical registers, insert spill/reload
  │    │
  │    ▼
  │  isa emit ── VInst → machine code bytes + relocations
  │
  ▼
output ── package code into ELF object or JIT buffer
  │
  ▼
link (if ELF) ── resolve builtins, produce executable
```

Each stage is a pure function from its input to its output. No shared mutable state between stages.
The pipeline is single-pass per function in the common case (lowering, allocation, and emission each
walk the instruction sequence once).

## Input: LPIR

`lpvm-native` consumes `IrModule` / `IrFunction` from the `lpir` crate. It does not parse GLSL or
interact with Naga. The GLSL→LPIR path is handled by `lps-frontend` and is shared across all
backends.

Relevant LPIR properties (see `docs/design/lpir/`):

- **Types**: `i32`, `f32`, `ptr`. No vectors, no `i64` as a data type.
- **Vregs**: non-SSA, may be reassigned, dense indices, typed at first definition.
- **Control flow**: structured (`if`/`else`, `loop`, `break`, `continue`, `br_if_not`, `switch`,
  `return`). No CFG, no basic blocks.
- **Calls**: single `call` opcode for both local functions and imports. VM context `v0:ptr` is
  implicit.
- **Float mode**: the IR is mode-agnostic. Whether `fadd` means IEEE f32 or Q16.16 fixed-point is
  determined by the backend at emission time.
- **Program size** (measured on real GLSL via `lp-cli shader-lpir <file.glsl> --stats`; same parse +
  lower path as the JIT; use `--skip-validate` if the printer is needed while validation is still
  catching up on a given pattern): **Per function** spans a wide range. Tiny helpers and filetest snippets
  are often **single-digit to a few dozen ops** with comparable vreg counts. A non-trivial composite
  shader (`lp-shader/lps-filetests/filetests/debug/rainbow.glsl`) is **~550 LPIR ops** across **15**
  functions; the largest function body (`rainbow_main`) is **~154 ops** with **~157 vregs**
  declared in that function (vreg index space is dense; peak *live* values at any program point is
  lower than total vregs, but allocators must still handle large vreg pools and spills). Treat
  **100+ ops / 100+ vregs per function** as normal for scene-style code, not an edge case.

## Float modes: Q32 and F32

LPIR float operations (`fadd`, `fmul`, etc.) are emitted differently depending on the numeric mode.
This choice is a compile-time parameter, not an IR property.

### Q32 (Q16.16 fixed-point)

The primary mode for ESP32-C6, which lacks an FPU.

Most Q32 operations lower to **builtin calls**: `fadd` → call `__lp_lpir_fadd_q32`, `fmul` → call
`__lp_lpir_fmul_q32`, etc. These builtins are precompiled RV32 functions linked from firmware.

A few Q32 operations can be **inlined** as integer arithmetic, avoiding the call overhead:

| LPIR op | Inline expansion | Why |
|---------|-----------------|-----|
| `fneg` | `sub rd, x0, rs` | Two's complement negate works on Q16.16 |
| `fabs` | `srai tmp, rs, 31` / `xor rd, rs, tmp` / `sub rd, rd, tmp` | Branchless absolute value |

The backend tracks which operations are inlineable per mode. The boundary between inline and builtin
is a tuning decision: more inlining reduces call overhead but increases code size and register
pressure.

Q32 semantics (saturation, division by zero, conversions) are defined in `docs/design/q32.md`.

### F32 (IEEE 754)

Requires hardware float support (F extension) or a software float library. ESP32-C6 does not have
the F extension, so F32 mode is host-only for now.

On hosts with float hardware, `fadd` → `fadd.s rd, rs1, rs2` (single RV32F instruction). On hosts
without, F32 operations would lower to soft-float builtin calls, structurally similar to Q32.

F32 mode is not in the initial implementation. The pipeline supports it by design: the `FloatMode`
parameter selects which emission path runs, and the VInst layer is mode-agnostic.

### Design choice: mode in the emitter, not the IR

This matches LPIR's design (see `docs/design/lpir/00-overview.md`, decision 2). Different backends
benefit from different Q32 strategies. Keeping mode selection in the emitter avoids a shared IR-level
Q32 transform that would not fit all targets.

For `lpvm-native`, the consequence is that Q32 awareness lives in `isa/rv32/emit.rs` (and the
eventual soft-float F32 path), not in lowering or register allocation.

## VInst: virtual instruction layer

VInst is the intermediate representation between LPIR and machine code. It is the central data
structure of the backend.

### Purpose

Lowering (LPIR→VInst) translates semantic operations into target-class instructions using virtual
registers. Emission (VInst→bytes) handles encoding and physical register names. Register allocation
operates on VInst sequences.

This separation means:

- Lowering does not need to know about physical registers or instruction encoding.
- The register allocator is ISA-independent; it sees VInst operand patterns but not encoding details.
- Emission is a mechanical mapping from VInst+allocation to bytes.

### Shape

VInst instructions are flat (no expression trees), typed (each vreg has a known width), and
explicitly name all source and destination vregs:

```
Add32  { dst: VReg, src1: VReg, src2: VReg }
Load32 { dst: VReg, base: VReg, offset: i32 }
Store32 { src: VReg, base: VReg, offset: i32 }
CallBuiltin { name: BuiltinId, args: [VReg], rets: [VReg] }
Ret { vals: [VReg] }
Branch { cond: VReg, target: Label }
Jump { target: Label }
Label(LabelId)
```

The enum covers the union of operations needed across all target ISAs. An ISA that does not support a
given VInst variant reports an error at emission time.

### 64-bit

The type system includes `I64` for forward compatibility. VInst has `Load64`/`Store64` variants.
Implementations may panic on 64-bit paths until needed. This avoids a type system redesign later at
the cost of unused enum arms now.

### What VInst is not

VInst is not a general-purpose compiler IR. It has no optimization passes, no dataflow analysis, no
SSA form. It is a thin scheduling layer between LPIR semantics and machine encoding. Keeping it
minimal is intentional: the fewer responsibilities VInst carries, the less that can go wrong.

## Lowering: LPIR → VInst

Lowering walks `IrFunction.body` (a `Vec<Op>`) and emits a `Vec<VInst>`. It is a single forward
pass.

### Operation mapping

Most LPIR ops map 1:1 to a VInst:

| LPIR | VInst |
|------|-------|
| `iadd v2, v0, v1` | `Add32 { dst: v2, src1: v0, src2: v1 }` |
| `load v1, 0` | `Load32 { dst: v1, base: v0, offset: 0 }` |
| `store v1, 0, v2` | `Store32 { src: v2, base: v1, offset: 0 }` |
| `return v0` | `Ret { vals: [v0] }` |

Float ops in Q32 mode expand to `CallBuiltin`:

| LPIR (Q32) | VInst |
|------------|-------|
| `fadd v2, v0, v1` | `CallBuiltin { name: FaddQ32, args: [v0, v1], rets: [v2] }` |
| `fneg v1, v0` | `Sub32 { dst: v1, src1: ZERO, src2: v0 }` (inlined) |

### Control flow

Structured control flow lowers to labels and branches:

| LPIR | VInst sequence |
|------|---------------|
| `if v_cond { ... } else { ... }` | `Branch { cond: v_cond, target: else_label }` / then body / `Jump { target: merge }` / `Label(else_label)` / else body / `Label(merge)` |
| `loop { ... }` | `Label(loop_top)` / body / `Jump { target: loop_top }` / `Label(loop_exit)` |
| `break` | `Jump { target: loop_exit }` |
| `br_if_not v0` | `Branch { cond: v0, target: loop_exit }` (branch-on-zero) |

Labels are resolved to byte offsets during emission. Forward references use backpatching.

### VM context

`v0:ptr` (the implicit VM context) is lowered like any other vreg. The register allocator assigns it
a physical register. Callers pass it in the appropriate argument register per the shader ABI.

## Register allocation

### Interface

The allocator is behind a trait so implementations can be swapped:

```rust
trait RegAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_info: &VRegInfo) -> Allocation;
}
```

`VRegInfo` carries per-vreg type and count. `Allocation` maps each vreg to a physical register or
spill slot, and includes metadata (frame size, which callee-saved registers are used).

### Greedy allocator

The initial implementation. Round-robin through the allocatable register set (x8–x31 on RV32, 24
registers). When no register is free, spill the value with the furthest next use (simplified
Belady). ~150 lines.

Good enough for POC-sized slices and tiny helpers where **fewer than ~24 values are live at once**
(and for experiments where spilling is acceptable). It is **not** sufficient as the sole allocator
for large functions like `rainbow_main` without heavy spilling: production use assumes **linear
scan** (below) plus spills. Quality also degrades when many values are live across **builtin calls**
(each call clobbers caller-saved registers `a0`–`a7`, `t0`–`t6`).

### Linear scan allocator

The production target. Exploits LPIR's structured control flow for O(n) live interval computation:

1. Single forward pass records `first_def` and `last_use` per vreg.
2. At `LoopStart`, snapshot live vregs. At `LoopEnd`, extend their intervals to cover the loop.
3. At if/else merge points, conservatively extend intervals through both branches.

Result: `Vec<Interval>` where each interval is `(vreg, start_pc, end_pc)`. Memory cost: 4 bytes per
vreg (two `u16` values). No bitvectors, no iterative dataflow, no CFG construction.

Allocation: sort intervals by start, maintain an active set, assign registers or spill the interval
ending furthest in the future. Standard linear scan, ~400 lines.

Expected quality: ~95% of graph coloring for shader-like code (mostly straight-line with small
loops). The remaining gap is lost in Q32 builtin call overhead, which dominates shader execution
time.

### Spilling

When the allocator must spill, it assigns a stack slot and emits `Load32`/`Store32` VInsts for
reload/spill. Spill slots are frame-pointer-relative. The frame layout is finalized after allocation
(total spill slots known), then prologue/epilogue are emitted.

For the greedy allocator, spilling is emergency-only. For linear scan, spilling is heuristic-driven
(furthest-end, weighted by loop depth).

### Register classes

RV32 has one integer register file. If F32 mode is added with hardware floats, a second register
class (f0–f31) appears. The allocator interface supports multiple classes via `VRegInfo` types, but
the initial implementation handles only integer registers.

## ISA layer

### Abstraction

The ISA layer is a trait that maps VInst sequences (with physical register assignments) to machine
code bytes:

```rust
trait IsaBackend {
    fn emit_function(&self, vinsts: &[VInst], alloc: &Allocation, ctx: &EmitContext) -> CodeBlob;
}
```

`CodeBlob` contains the code bytes, relocation records, and symbol references. `EmitContext`
carries compile options (float mode, target features).

### RV32 backend (`isa/rv32/`)

The initial and primary ISA. Targets `riscv32imac-unknown-none-elf` (integer + multiply + atomics +
compressed).

**Instruction encoding** (`inst.rs`): Pure functions encoding R/I/S/B/U/J-type instructions.
Mechanical bit manipulation, no state. ~500 lines.

**Emission** (`emit.rs`): Walks VInst sequence, emits encoded instructions. Handles:

- ALU ops → R-type/I-type encodings
- Loads/stores → I-type/S-type
- Branches → B-type with label backpatching (±4 KB range)
- Jumps → J-type (±1 MB range)
- Builtin calls → save ra, move args to a0–a3, `jal`/`lui+jalr`, restore ra
- Prologue/epilogue → stack adjustment, register save/restore

**ABI** (`abi.rs`): Defines the shader calling convention and frame layout. See "Shader ABI" below.

**Registers** (`reg.rs`): Physical register definitions, register class membership, display names.

### Extensibility

The ISA trait exists so that additional targets (e.g. AArch64, Xtensa, x86-64 host JIT) can be
added without changing lowering or register allocation. Each new ISA provides `inst.rs`, `emit.rs`,
`abi.rs`, and `reg.rs`. The VInst layer and allocator are shared.

This is a design provision, not an immediate goal. Only RV32 is implemented initially.

## Shader ABI

We control both sides of every call boundary. Builtins are our assembly. Local functions are our
generated code. There is no foreign code. This means we do not need the full RISC-V psABI with its
struct returns, varargs, exception tables, and TLS support.

The shader ABI is a minimal calling convention designed for LPIR's characteristics:

| Aspect | Convention | Rationale |
|--------|-----------|-----------|
| Arguments | a0–a7 (first 8 scalar params) | Covers all practical shader arities |
| Returns | a0–a1 | Sufficient for scalar and small-tuple returns |
| Caller-saved | a0–a7, t0–t6 | Clobbered by calls; allocator avoids these across calls |
| Callee-saved | s0–s11 | s0 reserved as frame pointer; s1–s11 available |
| Stack | Grows down, 16-byte aligned, fixed size per function | No dynamic allocation |
| ra | Saved by caller for non-leaf functions | Simplifies callee code |
| VM context | Passed in a0 (first argument) | Matches existing convention |

Multi-return beyond 2 values uses an `sret` pointer (caller-allocated buffer, address in a0 before
other args). This matches the existing Cranelift convention for compatibility.

## Output: ELF and JIT buffer

The backend supports two output modes.

### ELF object (relocatable)

A minimal relocatable ELF with:

- `.text` — code bytes
- `.symtab` — defined symbols (functions) and undefined symbols (builtins)
- `.strtab` — symbol name strings
- `.rel.text` — relocations for builtin call sites
- `.shstrtab` — section name strings

No program headers (not executable on its own). Linked by `lp-riscv-elf` to resolve builtin
addresses and produce an executable.

ELF is the initial output format. It reuses existing linking infrastructure and is debuggable with
standard tools (`readelf`, `objdump`, emulator trace).

### JIT buffer (direct)

For on-device compilation, ELF overhead is unnecessary. The JIT path:

1. At firmware boot, populate a `BuiltinTable` with addresses of all Q32 builtins.
2. At compile time, emit code directly into an executable buffer. Builtin calls use absolute
   addresses from the table—no relocations, no symbol resolution.
3. Mark the buffer executable (or use execute-in-place from PSRAM).

This eliminates ELF generation, section headers, string tables, and the linker step. The output is
position-dependent code ready to call.

JIT buffer output is the production goal for embedded. It is deferred until the pipeline is validated
through ELF + emulator.

### Choosing between them

| | ELF object | JIT buffer |
|-|-----------|------------|
| Use case | Emulator testing, host builds, cross-compilation | On-device JIT |
| Linking | `lp-riscv-elf` resolves builtins | Pre-linked via `BuiltinTable` |
| Debuggability | Standard tools | Emulator trace only |
| Overhead | Section headers, relocation records | None beyond code bytes |
| When | POC and test infrastructure | Post-POC, device integration |

## Crate structure

```
lp-shader/lpvm-native/
├── Cargo.toml
└── src/
    ├── lib.rs              # Public API, LpvmEngine impl, re-exports
    ├── error.rs            # CompileError, EmitError
    ├── types.rs            # Extended type info (IrType → backend type)
    ├── vinst.rs            # VInst enum, VReg, Label types
    ├── lower.rs            # LPIR Op → VInst lowering
    │
    ├── regalloc/
    │   ├── mod.rs          # RegAlloc trait, Allocation, VRegInfo
    │   ├── greedy.rs       # Greedy single-pass allocator
    │   └── linear_scan.rs  # Linear scan with intervals (future)
    │
    ├── isa/
    │   ├── mod.rs          # IsaBackend trait, CodeBlob, EmitContext
    │   └── rv32/
    │       ├── mod.rs      # RV32 backend entry point
    │       ├── inst.rs     # Instruction encoding (R/I/S/B/U/J)
    │       ├── reg.rs      # Physical registers, classes
    │       ├── emit.rs     # VInst → RV32 bytes, label resolution
    │       └── abi.rs      # Shader ABI: frame layout, prologue/epilogue
    │
    ├── output/
    │   ├── mod.rs          # Output trait / dispatch
    │   ├── elf.rs          # Minimal relocatable ELF emitter
    │   └── jit.rs          # Direct JIT buffer emitter (future)
    │
    ├── module.rs           # NativeModule (LpvmModule impl)
    └── instance.rs         # NativeInstance (LpvmInstance impl)
```

The directory structure anticipates multiple ISAs (`isa/rv32/`, `isa/aarch64/`, ...),
multiple allocators (`regalloc/greedy.rs`, `regalloc/linear_scan.rs`), and multiple output formats
(`output/elf.rs`, `output/jit.rs`). Only the RV32 + greedy + ELF combination is implemented
initially.

## Lpvm trait integration

`lpvm-native` implements the same `LpvmEngine` / `LpvmModule` / `LpvmInstance` trait family as
`lpvm-cranelift`. This means it plugs into the existing shader runtime, filetest harness, and engine
infrastructure without special-casing.

```
NativeEngine::compile(ir, meta)
    → lower all functions
    → allocate registers
    → emit machine code
    → package as ELF (or JIT buffer)
    → link with builtins (if ELF)
    → return NativeModule

NativeModule::instantiate()
    → create execution context (vmctx)
    → return NativeInstance

NativeInstance::call(name, args)
    → marshal arguments
    → invoke function via emulator (ELF) or direct call (JIT)
    → unmarshal results
```

## Prior art and references

| System | Relevance | What we take from it |
|--------|-----------|---------------------|
| QBE | Minimal compiler backend (~10K lines). RV64 port shows ~900 lines for ABI + instruction selection + emission. | Demonstrates that a small, correct backend is feasible. Reference for RV ABI structure. |
| Cranelift (our fork) | Current backend. ISLE instruction selection, regalloc2, full-featured. | Reference for correct RV32 instruction encoding. Study `abi.rs` for edge cases, don't copy the complexity. |
| Linear scan (Poletto & Sarkar, 1999) | Classic fast register allocator for JIT compilers. | Core algorithm for the production allocator. |
| LLVM MachineFunction | Multi-level IR (DAG → MachineInstr → MCInst). | Anti-reference: too many lowering stages for our needs. VInst is one level. |

## Risks and open questions

### ABI / stack / linking (high risk)

A previous attempt at a custom RV32 backend failed on these "plumbing" details. Stack frame layout,
calling convention edge cases, spill slot addressing, and ELF relocation records are individually
simple but collectively error-prone.

Mitigation: strict phased complexity. Start with no stack frame at all (leaf functions, no spills,
no calls). Add each layer only after the previous one is validated. Hard budget of 1000 lines for
ABI/stack code; if exceeded, simplify before continuing.

### Register allocation quality

The greedy allocator may produce poor code for shaders with many live values across builtin calls.
Each call clobbers a0–a7 and t0–t6, forcing caller-saved values to spill.

Open question: should the allocator have call-site awareness (prefer callee-saved registers for
values live across calls)? This is a linear-scan concern, not a greedy one.

### Branch range limits

RV32 B-type branches have ±4 KB range. For shaders exceeding this (large loop bodies, deeply nested
control flow), the emitter must invert the condition and use a J-type jump (±1 MB). This is
standard but adds a relaxation pass or two-pass emission.

Open question: emit conservatively (always use inverted-branch + jump) or optimistically (B-type
first, relax if out of range)?

### Large immediates

RV32 I-type instructions have 12-bit signed immediates. Constants outside this range need a
`lui`/`addi` sequence. Stack frame offsets and spill slot addresses may exceed 12 bits for larger
frames.

This is mechanical but pervasive—every immediate-using emission path must check the range.

### Multi-return ABI

Functions returning more than 2 scalar values use an `sret` pointer. This requires the caller to
allocate stack space and pass the address, and the callee to write results through the pointer. The
Cranelift backend already handles this; `lpvm-native` must match the convention for compatibility.

### What is not yet designed

- **Switch lowering**: jump table vs. if/else chain vs. binary search. Depends on case density.
  Deferred until control flow is implemented.
- **Memcpy emission**: inline vs. loop vs. builtin call. Depends on size thresholds.
- **Debug info**: mapping code offsets back to LPIR ops for emulator traces. Useful for development
  but not required for correctness.
- **Compressed instructions (RV32C)**: ESP32-C6 supports the C extension. Using 16-bit encodings
  where possible would reduce code size. Deferred—standard 32-bit encoding first.
