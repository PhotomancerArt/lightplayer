# lp-native: Custom LPIR→RV32 Backend

**Date**: 2026-04-08
**Status**: POC design
**Crate**: `lp-shader/lpvm-native`

## Problem

Cranelift is the current backend for LPIR→RV32 compilation on ESP32-C6. It
works, but its memory footprint constrains what shaders can be compiled on
device:

| Metric              | Cranelift         | Constraint                        |
| ------------------- | ----------------- | --------------------------------- |
| Peak compile RAM    | ~50–100 KB        | ESP32-C6 has 500 KB total         |
| Backend binary size | ~150–230 KB       | Flash is 3 MB, firmware is at 76% |
| Compile time        | ~5–10 ms/function | Fine for now, but limits headroom |

The bottleneck is regalloc2. Its interference graph and union-find structures
allocate tens of kilobytes on the heap during compilation. We already forked
regalloc2 with `ChunkedVec` and disabled the optimizer/verifier. There is no
path to 10x reduction without replacing the allocator entirely, which means
replacing most of Cranelift's value.

## Goal

Build a lightweight custom backend (`lpvm-native`) that compiles LPIR directly
to RV32 machine code, targeting:

| Metric              | Target             | Rationale                                 |
| ------------------- | ------------------ | ----------------------------------------- |
| Peak compile RAM    | < 5 KB             | 10x headroom for larger shaders           |
| Backend binary size | < 50 KB            | Reclaim ~100 KB flash                     |
| Compile time        | < 1 ms/function    | 5–10x faster                              |
| Code quality        | ≥ 85% of Cranelift | Acceptable; Q32 builtins dominate runtime |

Cranelift remains the reference backend for host builds and differential
testing. `lpvm-native` targets embedded only.

## Why this is tractable

LPIR was designed with a custom backend in mind. Four properties make the
problem dramatically simpler than a general-purpose compiler backend:

**Structured control flow.** LPIR has `IfStart`/`Else`/`End`,
`LoopStart`/`End`, `Break`, `Continue`—no arbitrary CFG. Live intervals can be
computed in O(n) with O(vreg_count) memory. No dominator trees, no iterative
dataflow, no bitvectors.

**Scalarized operations.** All vector ops are decomposed before LPIR. Each
opcode maps 1:1 or 1:N to RV32 instructions. No pattern matching (ISLE) needed.

**Non-SSA virtual registers.** LPIR vregs with explicit types match physical
registers naturally. No SSA deconstruction pass, no phi resolution.

**Small programs.** Typical shaders are 100–300 ops with < 60 vregs. 24
allocatable registers (x8–x31) are usually sufficient without spilling.

## Architecture

```
LPIR (IrModule)
    │
    ▼
lower.rs ─── LPIR ops → VInst (target-independent virtual instructions)
    │
    ▼
regalloc/ ── Assign physical registers to vregs
    │
    ▼
isa/rv32/emit.rs ── VInst → RV32 machine code bytes + relocations
    │
    ▼
output/elf.rs ── Minimal relocatable ELF (.text, .symtab, .rel.text)
    │
    ▼
lp-riscv-elf ── Link with Q32 builtins (existing infrastructure)
    │
    ▼
Executable code
```

The crate implements the `LpvmEngine`/`LpvmModule`/`LpvmInstance` traits,
mirroring `lpvm-cranelift`. It plugs into the existing filetest harness as a
new backend target (`rv32lp.q32`).

### VInst: the intermediate layer

VInst is a flat instruction enum sitting between LPIR and machine code. It
knows about virtual registers but not physical ones, and about operation
semantics but not encoding details.

This layer exists so that lowering (LPIR→VInst) and emission (VInst→bytes) can
be developed and tested independently. It also makes the register allocator
target-agnostic—if we ever add another ISA, only the emitter changes.

VInst is intentionally minimal for the POC: 32-bit ALU ops, loads, stores,
builtin calls, and return. 64-bit variants are in the enum but stub
`unimplemented!()`.

## Key design decisions

### 1. ELF output, not raw JIT buffer

The POC emits relocatable ELF objects and links them through the existing
`lp-riscv-elf` infrastructure. This reuses proven linking code and makes the
output debuggable with standard tools (`readelf`, `objdump`).

Raw JIT buffer output (skip ELF, patch builtin addresses directly) is the
eventual on-device path. It is explicitly deferred to post-POC because:

- The linking infrastructure already works
- ELF lets us validate output with external tools
- The POC goal is proving the compilation pipeline, not the output format

### 2. Greedy register allocator first, linear scan later

The POC uses a greedy single-pass allocator: round-robin through x8–x31, spill
to stack when exhausted. This is ~150 lines and sufficient for the target test
case (`op-add.glsl`, < 10 live values).

Linear scan with intervals is the production target. LPIR's structured control
flow enables O(n) interval computation without CFG analysis. The algorithm is
well-understood and should yield ~95% of graph coloring quality for shader-like
code. Budget: ~700 lines total (interval analysis + allocator).

The allocator is behind a `RegAlloc` trait so the swap is mechanical.

### 3. Custom shader ABI, not RISC-V psABI

We control both sides of every call boundary. Builtins are written in assembly
with a known calling convention. There is no reason to implement the full
RISC-V psABI with its struct return, varargs, and exception handling
complexity.

The shader ABI:

| Aspect       | Convention                                           |
| ------------ | ---------------------------------------------------- |
| Arguments    | a0–a7 (first 8 scalar params)                        |
| Returns      | a0–a1                                                |
| Caller-saved | a0–a7, t0–t6                                         |
| Callee-saved | s0–s11 (s0 reserved as frame pointer)                |
| Stack        | Grows down, 16-byte aligned, fixed size per function |
| ra           | Always saved by caller for non-leaf functions        |

### 4. Pre-linked builtin addresses (post-POC)

The POC uses ELF relocations for builtin calls, resolved at link time. The
on-device production path will use a `BuiltinTable` populated at firmware boot:
the JIT compiler emits direct jumps to known addresses, eliminating ELF
generation and runtime linking entirely.

This is the single largest simplification for on-device use. No relocation
records, no section headers, no string tables. Just bytes in a buffer.

### 5. Separate crate, not a module inside lpvm-cranelift

`lpvm-native` is a new crate at `lp-shader/lpvm-native/`. It depends on `lpir`
and `lpvm` (for traits) but has no dependency on Cranelift.

This keeps the Cranelift backend unmodified (clean for upstream merges), allows
independent iteration, and makes the feature gating straightforward: firmware
builds pick one backend or the other.

## Tradeoffs we are accepting

**Worse code quality.** A greedy allocator will produce more spills and
unnecessary moves than Cranelift's regalloc2. For the POC this is acceptable.
For production, linear scan should close the gap to ~95%. The remaining 5% is
lost in the noise of Q32 builtin calls, which dominate shader execution time.

**No optimizations.** No peephole, no constant folding, no dead code
elimination. LPIR is already reasonably optimized by the frontend. The backend
emits what it gets. If code quality proves insufficient, peephole optimization
is the first lever (~200 lines).

**Duplicated instruction encoding.** We could extract Cranelift's RV32
encoding tables, but writing them fresh (~500 lines of pure functions) is
simpler than managing the extraction and keeps the crate dependency-free.

**64-bit is stub-only.** The type system includes I64 for forward
compatibility, but all 64-bit paths panic. This prevents a painful type system
redesign later while adding ~20% complexity now (enum variants, match arms).

**No F32 mode.** Only Q32 (fixed-point) is implemented. F32 requires the F
extension, which ESP32-C6 does not have. Host F32 stays on Cranelift.

## Risks

### ABI and stack frame complexity (HIGH)

A previous attempt at a custom RV32 backend failed on "plumbing"—stack frame
layout, calling convention details, spill slot management, and linking. These
are the hardest parts to get right and the easiest to underestimate.

Mitigation: start with Phase 0 (no calls, no spills, no stack frame at all).
Validate with emulator testing before adding each layer of complexity. Set a
hard complexity budget of 1000 lines for ABI/stack code. If exceeded, simplify.

### ELF emission complexity (MEDIUM)

Generating a valid relocatable ELF with correct section headers, symbol tables,
and relocation entries has many fiddly details.

Mitigation: emit a minimal ELF (5 sections: `.text`, `.symtab`, `.strtab`,
`.rel.text`, `.shstrtab`). If this proves too complex, fall back to raw binary
with a custom loader—the emulator can support either.

### Integration with filetest harness (MEDIUM)

Adding a 4th backend target to `lps-filetests` touches shared infrastructure.

Mitigation: the backend implements the same `LpvmEngine` trait. If harness
integration is painful, a standalone test binary validates correctness
independently.

### Time estimate wrong (MEDIUM, HIGH impact)

The POC is scoped at 7–11 days. If it's not producing correct output by the
end of M2 (~day 5), the approach may be fundamentally harder than expected.

Mitigation: hard decision gate at M2. If basic emission doesn't work, abandon
and keep Cranelift.

## Phased complexity budget

The previous attempt failed by taking on too much at once. This design adds
complexity in strict phases, each validated before proceeding:

| Phase  | What's added                             | What's NOT present            | Validates             |
| ------ | ---------------------------------------- | ----------------------------- | --------------------- |
| 0 (M1) | Types, VInst, lowering stubs             | No emission, no execution     | Pipeline compiles     |
| 1 (M2) | RV32 encoding, greedy alloc, emission    | No calls, no spills, no stack | Correct machine code  |
| 2 (M3) | ELF output, builtin calls, stack frames  | No control flow, no spilling  | End-to-end execution  |
| 3 (M4) | Differential testing, memory measurement | No optimization               | Correctness + metrics |

Each phase has a go/no-go gate. Phase N is not started until Phase N-1
produces correct output verified by tests.

## Scope boundaries

### In scope for POC

- Single filetest: `op-add.glsl` (integer addition via Q32 builtin)
- Integer arithmetic only
- Single function (no inter-function calls)
- Q32 mode via builtin call
- Greedy register allocation
- Minimal ELF with relocations
- Integration with filetest harness as `rv32lp.q32`

### Explicitly out of scope

- Control flow (if/else, loops, switch)
- Spilling optimization
- 64-bit operations (stubbed only)
- F32 float mode
- JIT buffer output
- Peephole optimization
- Multiple ISAs (design supports, only RV32 implemented)
- On-device execution (emulator only)

### Post-POC milestones (if POC succeeds)

- **Control flow**: branches, loops, break/continue, label backpatching
- **Linear scan allocator**: interval analysis, production-quality allocation
- **Full filetest suite**: all `*.glsl` tests passing
- **JIT buffer output**: skip ELF for on-device, pre-linked builtins
- **ESP32 integration**: replace Cranelift in firmware builds

## Relationship to existing crates

| Crate            | Relationship                                                |
| ---------------- | ----------------------------------------------------------- |
| `lpir`           | Input. `lpvm-native` consumes `IrModule`/`IrFunction`       |
| `lpvm`           | Traits. Implements `LpvmEngine`/`LpvmModule`/`LpvmInstance` |
| `lps-shared`     | Types. Uses `LpsModuleSig`, `CompileOptions`                |
| `lpvm-cranelift` | Sibling. No dependency in either direction                  |
| `lp-riscv-elf`   | Downstream. Consumes ELF output for linking                 |
| `lp-riscv-emu`   | Downstream. Executes linked code                            |
| `lps-filetests`  | Integration. New backend target `rv32lp.q32`                |

## Success criteria

The POC succeeds if:

1. `op-add.glsl` compiles through `lpvm-native` and produces correct output in `lp-riscv-emu`
2. Peak compile-time RAM is < 10 KB (measured via counting allocator)
3. The output is verifiable: matches Cranelift's result for the same input
4. The codebase is < 2500 lines and the architecture is clear enough to extend

The POC fails if:

1. Correct RV32 emission is not achieved by M2 (basic encoding + allocation)
2. ABI/stack complexity exceeds the 1000-line budget
3. The approach requires more RAM than expected (> 20 KB), negating the benefit

If the POC fails, we keep Cranelift and invest in further regalloc2 optimization
or a bytecode interpreter fallback for large shaders.
