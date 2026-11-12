# Native Backend Design

This document describes the architecture of the native (RISC-V) code generation backend.

## IR Layers

The native backend transforms code through three intermediate representations:

| Layer | Name | Abstraction | Key Properties |
|-------|------|-------------|----------------|
| **LPIR** | LightPlayer IR | High-level | Structured control flow (if/then/else, loops), target-agnostic, frontend interop (including WASM) |
| **VInst** | Virtual Instruction | Machine ISA, virtual registers | RISC-V instructions (Add32, Call, etc.) with **infinite** virtual registers (`i0`, `i1`...). No spills, no stack, no ABI details. Input to register allocator. |
| **PInst** | Physical Instruction | Machine ISA, physical registers | Same RISC-V instructions but with **real registers** (`a0`, `s1`...). Includes explicit `FrameSetup`, `Lw`/`Sw` for spills, and full ABI details. Output of register allocator. |

### Nomenclature

- **VReg**: Virtual register (`VReg(u8)`). Infinite supply. Used in VInst.
- **PReg**: Physical register (`u8`, 0-31). Real RISC-V registers. Used in PInst.
- **VInst**: Virtual instruction. Register fields contain VReg.
- **PInst**: Physical instruction. Register fields contain PReg. May include frame/stack ops.

### Pipeline Flow

```
LPIR (structured control flow)
    ↓
Lowerer (isa/rv32/lower.rs)
    ↓
VInst[] (infinite virtual registers)
    ↓
Peephole optimizer (isa/rv32/peephole.rs)
    ↓
Register Allocator (isa/rv32fa/alloc.rs)
    ↓
PInst[] (physical registers + spills)
    ↓
Emitter (isa/rv32fa/emit.rs)
    ↓
Machine code bytes in RAM
```

### Register Allocator Role

The allocator bridges VInst → PInst:

1. **Input**: VInst sequence with infinite VRegs
2. **Output**: PInst sequence with finite PRegs and explicit spill code
3. **Process**: Backward walk, LRU eviction, rematerialization, call clobber handling

### Textual Representations

| IR | Format | Example |
|----|--------|---------|
| VInst | Custom | `i2 = Add32 i0, i1` |
| PInst | Standard RISC-V asm | `add a0, a1, a2` |

### ABI Integration

PInst includes ABI-aware operations:
- `FrameSetup { spill_slots }` - prologue with spill slot reservation
- `FrameTeardown { spill_slots }` - epilogue
- `SlotAddr { dst, slot }` - compute address of spill slot
- Register reservation for args/returns (`a0`-`a7`, `ra`, `sp`, `fp`)
