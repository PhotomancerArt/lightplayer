# LPVM-Native POC Roadmap

Date: 2026-04-07

## Motivation

Cranelift is too heavy for embedded JIT compilation on ESP32-C6 (500KB RAM). Peak memory usage during compilation (50-100KB from regalloc2's data structures) limits shader complexity. A custom lightweight backend can reduce compile-time RAM by 10x and binary size by 5x.

**Goal of POC**: Prove the approach is viable by implementing a vertical slice that compiles and executes a single simple filetest (`op-add.glsl`).

Current `fw-esp32` size is: `2,381,536/3,145,728 bytes, 75.71%`

### Success Criteria

- Compile `lps-filetests/filetests/scalar/int/op-add.glsl` via new backend
- Execute in `lp-riscv-emu` and produce correct output
- Passes through existing filetest harness as new target `rv32lp.q32`
- Peak compile-time RAM < 5KB (vs ~75KB for Cranelift)

## Architecture

### Directory Structure

```
lp-shader/lpvm-native/
├── Cargo.toml
└── src/
    ├── lib.rs                 # Public API: compile_to_elf(), etc.
    ├── error.rs               # CompileError types
    ├── lower.rs               # LPIR → VInst (virtual instructions)
    ├── vinst.rs               # VInst enum (target-independent IR)
    ├── types.rs               # Extended IrType (I32, I64 stub, F32, Ptr)
    ├── regalloc/
    │   ├── mod.rs             # RegAlloc trait interface
    │   └── greedy.rs          # Greedy single-pass allocator (upgradeable)
    ├── isa/
    │   ├── mod.rs             # IsaBackend trait
    │   └── rv32/
    │       ├── mod.rs         # RV32 backend implementation
    │       ├── inst.rs        # RV32 instruction encoding (R/I/S/B/U/J)
    │       ├── emit.rs        # VInst → RV32 byte emission
    │       └── abi.rs         # Shader ABI: frame layout, calling convention
    └── output/
        ├── mod.rs
        └── elf.rs              # Minimal relocatable ELF emitter
```

### Pipeline Flow

```
Input: LPIR (IrModule)
    │
    ▼
┌─────────────────────────────────────┐
│ lower.rs                            │
│ - Map LPIR ops to VInst             │
│ - Handle types (I32, stub I64)      │
│ - Track virtual registers           │
└─────────────────────────────────────┘
    │
    ▼
VInst sequence (target-independent)
    │
    ▼
┌─────────────────────────────────────┐
│ regalloc/greedy.rs                  │
│ - Assign x8-x31 to vregs            │
│ - Spill to stack slots (emergency)  │
│ - Return: vreg → phys_reg mapping   │
└─────────────────────────────────────┘
    │
    ▼
VInst with assigned registers
    │
    ▼
┌─────────────────────────────────────┐
│ isa/rv32/emit.rs                    │
│ - Encode RV32 instructions          │
│ - Handle immediates (12-bit check)    │
│ - Emit builtin calls with relocs    │
│ - Frame setup/teardown              │
└─────────────────────────────────────┘
    │
    ▼
Raw code bytes + relocations
    │
    ▼
┌─────────────────────────────────────┐
│ output/elf.rs                       │
│ - Minimal ELF: .text, .symtab, .rel │
│ - Undefined symbols for builtins    │
└─────────────────────────────────────┘
    │
    ▼
Output: ELF object bytes
    │
    ▼
┌─────────────────────────────────────┐
│ lp-riscv-elf::link_object_with_     │
│ builtins() (existing)               │
│ - Resolve __lp_lpir_fadd_q32 etc.   │
│ - Emit final executable ELF         │
└─────────────────────────────────────┘
    │
    ▼
Executable in emulator
```

### Key Data Structures

```rust
// vinst.rs - Virtual instructions (post-lowering, pre-regalloc)
pub enum VInst {
    // 32-bit ALU
    Add32 { dst: VReg, src1: VReg, src2: VReg },
    Load32 { dst: VReg, base: VReg, offset: i32 },
    Store32 { src: VReg, base: VReg, offset: i32 },
    
    // 64-bit (stubbed for POC)
    Load64 { dst: VReg, base: VReg, offset: i32 },  // unimplemented!()
    
    // Control (not used in POC, but in enum)
    Call { target: Symbol, args: Vec<VReg> },
    Ret,
    
    // Meta
    Comment(&'static str),
}

// types.rs
pub enum IrType {
    I32,
    I64,     // Design includes, implementation stubs
    F32,     // Q32 uses I32 representation
    Ptr,
}

// regalloc/greedy.rs
pub struct GreedyAlloc;
impl RegAlloc for GreedyAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_types: &[IrType]) -> Allocation {
        // Round-robin through x8-x31
        // Spill when exhausted (emergency only)
    }
}
```

### Shader ABI (RV32)

Designed for LPIR's characteristics:


| Aspect       | Convention                      |
| ------------ | ------------------------------- |
| Args         | a0-a7 (first 8 scalar params)   |
| Returns      | a0-a1 (first 2 values)          |
| Callee-saved | s0-s11 (but we avoid for POC)   |
| Caller-saved | a0-a7, t0-t6                    |
| Frame        | Fixed size, s0 as frame pointer |
| Stack        | Grows down, 16-byte aligned     |


Builtin calls:

- Save ra to stack (non-leaf)
- Args in a0-a3
- Result in a0
- Restore ra

## Scope

### In Scope for POC

- Single filetest: `op-add.glsl`
- Integer arithmetic only (no control flow)
- Single function (no calls between user functions)
- Q32 mode via builtin call (`__lp_lpir_fadd_q32`)
- Greedy register allocation
- Minimal ELF with relocations
- Integration with existing filetest harness as `rv32lp.q32`

### Explicitly Out of Scope

- Control flow (if/else, loops, switch)
- Function calls between user functions
- Float mode (F32) - Q32 only
- Spilling optimization (just emergency fallback)
- 64-bit operations (stubbed in design only)
- JIT buffer output (ELF only for POC)
- Optimizations (peephole, coalescing)
- Multiple ISAs (design supports, only RV32 implemented)
- Complex register allocation (linear scan, graph coloring)

## Alternatives Considered

### 1. Optimize Cranelift Further

**Rejected**: Already forked regalloc2 with `ChunkedVec`, disabled optimizer/verifier. Peak RAM still ~50KB from regalloc2's core algorithm. Cannot reduce further without replacing the allocator entirely.

### 2. Bytecode Interpreter

**Rejected**: Would eliminate compilation RAM entirely, but 4000 shader executions per frame makes interpreter too slow. JIT compilation is the product requirement.

### 3. Precompile Shaders on Host

**Rejected**: Violates core product requirement (embedded JIT). Also breaks the development workflow where shaders are edited on-device.

### 4. Use Existing Minimal Compiler (QBE, 8cc)

**Rejected**: These target C (irreducible CFG). LPIR has structured control flow enabling simpler algorithms. Adapting them would require more work than building fresh.

### 5. Cranelift with Custom Allocator

**Rejected**: Cranelift's API assumes regalloc2's interface. Swapping allocators would require forking more of Cranelift than building a minimal backend.

## Risks


| Risk                                     | Likelihood | Impact | Mitigation                                                     |
| ---------------------------------------- | ---------- | ------ | -------------------------------------------------------------- |
| ELF emission more complex than expected  | Medium     | Medium | Can fall back to raw binary + custom loader                    |
| Greedy allocator produces unusable code  | Low        | Medium | Test early; can upgrade to linear scan in days                 |
| 64-bit stub design blocks implementation | Low        | Low    | Type system is cheap; emit can panic                           |
| Can't match Cranelift's code quality     | High       | Low    | POC goal is feasibility, not parity                            |
| Integration with filetest harness tricky | Medium     | Medium | Fallback: standalone test binary                               |
| Time estimate wrong (4-6 days)           | Medium     | High   | Hard deadline: abandon and keep Cranelift if not working by M2 |


## Milestones


| Milestone             | Goal                                                                     | Documents                                      |
| --------------------- | ------------------------------------------------------------------------ | ---------------------------------------------- |
| **M1: Core + Traits** | Crate structure, types, VInst, lowering, stub Lpvm trait implementations | `[m1-core-traits.md](./m1-core-traits.md)`     |
| **M2: RV32 Emission** | Instruction encoding, Shader ABI, VInst → RV32 bytes                     | `[m2-rv32-emission.md](./m2-rv32-emission.md)` |
| **M3: Integration**   | ELF emission, linking with builtins, filetest harness integration        | `[m3-integration.md](./m3-integration.md)`     |
| **M4: Validation**    | Differential testing vs Cranelift, memory metrics, next steps            | `[m4-validation.md](./m4-validation.md)`       |


**Total estimated**: 7-11 days for working POC

## Scope Estimate

- **Lines of code**: ~2,500
- **Files**: ~12
- **Time**: 4-6 days (per milestone estimates)
- **Key complexity**: ELF emission, ABI compliance, register allocation interface

Most code is straightforward table-driven emission. Risk is in the "plumbing" (stack frames, calling convention, relocations).