# Stack Parameter Support - Design

## Scope of Work

Implement RISC-V ILP32 stack parameter support for `lpvm-native`. Enable functions with more than 8 parameter slots (vmctx + user params) by supporting the RV32 ABI stack argument passing convention.

## File Structure

```
lp-shader/lpvm-native/src/
├── regalloc/
│   ├── mod.rs              # UPDATE: Add incoming_stack_params to Allocation
│   └── greedy.rs           # UPDATE: Assign regs for stack params, track in Allocation
├── abi/
│   ├── mod.rs              # (no changes - ArgLoc::Stack already exists)
│   └── frame.rs            # UPDATE: Add caller_arg_stack_size to FrameLayout
└── isa/rv32/
    ├── abi.rs              # (no changes - classify_params already handles stack)
    └── emit.rs             # UPDATE: emit_prologue load incoming, emit_call_* store outgoing

lp-shader/lps-filetests/filetests/lpvm/native/
├── stack-params-simple.glsl    # NEW: 10 scalar params
└── stack-params-mixed.glsl     # NEW: float + mat3 (reg + stack mix)
```

## Conceptual Architecture

### RISC-V Stack Argument Layout

```
Caller Stack Frame (high addresses)
┌─────────────────────────────┐
│  Return Address             │
├─────────────────────────────┤
│  Saved FP (s0)              │ ← s0 points here after prologue
├─────────────────────────────┤
│  Callee-saved regs          │
├─────────────────────────────┤
│  Spill slots (negative)     │
│  s0 - 4, s0 - 8, ...        │
├─────────────────────────────┤
│  LPIR slots (slotaddr)       │
├─────────────────────────────┤
│  Caller arg stack area       │ ← for outgoing calls with >8 args
│  (s0 + 0, s0 + 4, ...)      │
├─────────────────────────────┤
│  ↑ Stack growth (to lower)   │
│  Stack arguments from caller │ ← positive offsets from s0
│  (incoming params 8+)       │
└─────────────────────────────┘
Callee Stack Frame (low addresses)
```

### Data Flow - Incoming Stack Params

```
┌─────────────────┐
│  ABI classify   │──→ ArgLoc::Stack { offset: 0, size: 4 }
└────────┬────────┘
         │
         v
┌─────────────────┐
│  GreedyAlloc    │──→ Assign phys reg, track in incoming_stack_params
└────────┬────────┘
         │
         v
┌─────────────────┐
│  Emit Prologue  │──→ lw reg, s0 + offset (for each stack param)
└────────┬────────┘
         │
         v
┌─────────────────┐
│  Function Body  │──→ Use reg normally (param now in register)
└─────────────────┘
```

### Data Flow - Outgoing Stack Args

```
┌─────────────────┐
│  VInst::Call    │──→ args: [v0, v1, ..., v9] (10 args)
└────────┬────────┘
         │
         v
┌─────────────────┐     ┌─────────────────┐
│  emit_call_*    │──→  │  Store args 8+  │
│                 │     │  to caller arg  │
│                 │     │  stack area       │
│                 │     │  (sp + offset)    │
└────────┬────────┘     └─────────────────┘
         │
         v
┌─────────────────┐
│  jalr to callee │
└────────┬────────┘
         │
         v
┌─────────────────┐
│  Callee prologue│──→ lw arg8, s0 + 0  (same address, callee's view)
│  (loads from    │    lw arg9, s0 + 4
│   caller stack) │
└─────────────────┘
```

## Main Components

### 1. Regalloc - Incoming Stack Param Assignment

**File**: `regalloc/greedy.rs`

When `param_locs[i]` is `ArgLoc::Stack`, instead of erroring:
1. Assign a physical register from the allocatable set
2. Record `(vreg, stack_offset)` in `Allocation.incoming_stack_params`
3. Skip this register from subsequent allocation (mark as used)

### 2. Frame Layout - Outgoing Stack Area

**File**: `abi/frame.rs`

Add to `FrameLayout`:
- `caller_arg_stack_size: u32` - max bytes needed for outgoing stack args across all calls in function
- Computed by scanning `VInst::Call` instructions in the function
- Aligned to 16 bytes (RISC-V stack alignment)

### 3. Emit - Prologue Load (Incoming)

**File**: `isa/rv32/emit.rs`

After `emit_prologue()` saves callee-saved regs:
- For each `(vreg, offset)` in `alloc.incoming_stack_params`
- Emit: `lw phys_reg(vreg), s0 + offset`

### 4. Emit - Call Store (Outgoing)

**File**: `isa/rv32/emit.rs`

In `emit_call_direct()` and `emit_call_sret()`:
- For args 0..7: move to ARG_REGS (current behavior)
- For args 8..: store to `sp + computed_offset` in caller arg area
- Offsets are positive from caller's SP (which equals callee's s0)

## Design Decisions

### Separate Tracking for Stack Params vs Spills

| Aspect | Stack Params | Spills |
|--------|-------------|--------|
| Direction | Load from caller | Store to callee |
| Offset sign | Positive (s0 + off) | Negative (s0 - off) |
| Frame | Caller's frame | Callee's frame |
| Source | ABI-mandated | Regalloc decision |

### Optimization for <=8 Params

The implementation checks arg count at compile time:
- `args.len() <= 8`: Current fast path (no stack setup code)
- `args.len() > 8`: Emit stack store/load sequences

This keeps the common case efficient while supporting the edge case.

## Reference Implementation Pattern

From `lp-riscv-emu/src/emu/emulator/function_call.rs:473`:
```rust
// Stack arguments are at positive offsets from SP (above SP)
let entry_sp = (ram_end - total_stack_space) as i32;
emulator.regs[2] = entry_sp; // SP register
```

Callee accesses via `s0` (frame pointer), which equals the caller's SP at entry.
