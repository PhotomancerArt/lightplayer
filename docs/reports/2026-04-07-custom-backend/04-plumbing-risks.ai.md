# Plumbing Risks: ABI, Stack, Linking, and Risk Mitigation

**Date**: 2026-04-07  
**Context**: Lessons from previous attempt at RV32 backend

## The Trap

Previous attempt failed on:

- Stack frame layout and ABI compliance
- Spill slot management
- Jump islands and range limits
- Static data and ELF generation
- Linking with builtins

These are the "devil's details" that make a backend 5x more work than expected.

## Risk Mitigation Strategy: Start Simple, Add Complexity Only When Needed

### Phase 0: No Calls, No Spills, No Static Data

**Scope**: Straight-line code with loops only. No function calls (including builtins). No spills (max 24 live values). All immediates in instruction encoding.

```rust
// Supported:
func @simple(v1:i32, v2:i32) -> i32 {
  v3:i32 = iadd v1, v2
  v4:i32 = imul v3, v1
  loop {
    br_if_not v4
    v4 = isub_imm v4, 1
  }
  return v3
}

// NOT supported in Phase 0:
func @complex(v1:i32) -> i32 {
  v2:i32 = call @other(v1)      // No calls
  v3:i32 = imul v2, v2          // Would need >24 registers
  v4:i32 = iconst.i32 0x12345678 // Large constant needing lui
  return v3
}
```

**Why this helps**:

- No stack frames (no spills, no calls = no stack usage)
- No calling convention to implement
- No relocations (all offsets within ±4KB for branches, all immediates fit in 12 bits or use sequences we control)
- No linking (single function per buffer)

**Validation**: Run this subset in emulator, verify against interpreter. Get the core pipeline solid before adding ABI complexity.

---

### Phase 1: Add Spills Only (Still No Calls)

When we need more than 24 live values, add spills but keep it simple:

```rust
struct StackFrame {
    // Fixed layout per function:
    // [callee_saved_regs (optional)] [spill_slots] [padding to 16-byte align]
    
    spill_count: u8,        // Determined by allocator
    frame_size: u16,       // Computed once
}

fn emit_prologue(&mut self) {
    // addi sp, sp, -frame_size
    self.emit_i_type(-(self.frame_size as i32), 2, 0, 2, 0x13); // addi sp, sp, -size
}

fn emit_epilogue(&mut self) {
    // addi sp, sp, frame_size
    // ret (jalr x0, 0(ra))
    self.emit_i_type(self.frame_size as i32, 2, 0, 2, 0x13);
    self.emit_i_type(0, 1, 0, 0, 0x67); // jalr x0, 0(ra)
}
```

**Spill strategy**:

- Fixed spill slots assigned by allocator
- Always use frame pointer (s0 = x8) for spills
- Simple mapping: `spill_slot_addr(slot) = s0 + offset`

**Key decision**: Don't implement full RISC-V calling convention. Just enough for our own internal use (spills within a function).

---

### Phase 2: Add Simple Builtin Calls

For Q32 math, we need to call builtins. Don't use standard calling convention—use a **custom minimal convention**:

```rust
/// Custom "Shader ABI"
/// 
/// Arguments: a0-a3 (first 4 arguments)
/// Return: a0-a1 (first 2 return values)
/// Caller-saved: a0-a7, t0-t6 (all temporaries)
/// Callee-saved: s0-s11 (we use s0 as FP, so s1-s11 available)
/// Stack: Managed by caller for spills only
/// ra: Always saved by caller (we're not optimizing tail calls)

fn emit_builtin_call(&mut self, builtin_addr: u32, args: &[VReg]) {
    // Save ra (always, for simplicity)
    // addi sp, sp, -4
    // sw ra, 0(sp)
    self.emit_adjust_sp(-4);
    self.emit_store(1, 2, 0, 0x23, 0x2); // sw ra, 0(sp)
    
    // Move arguments to a0-a3
    for (i, arg) in args.iter().enumerate() {
        let src_reg = self.vreg_to_phys(*arg);
        let dst_reg = 10 + i as u8; // a0 = x10
        if src_reg != dst_reg {
            // mv dst, src (addi dst, src, 0)
            self.emit_i_type(0, src_reg, 0, dst_reg, 0x13);
        }
    }
    
    // jal ra, builtin_addr
    // But builtin_addr is 32-bit, might need lui+jalr
    self.emit_far_call(builtin_addr);
    
    // Restore ra
    // lw ra, 0(sp)
    // addi sp, sp, 4
    self.emit_load(0, 2, 1, 0x03, 0x2); // lw ra, 0(sp)
    self.emit_adjust_sp(4);
}
```

**Key simplification**: We control both sides. The builtin functions are written in assembly with this exact ABI. No need to match RISC-V psABI.

---

### Phase 3: Add Local Calls (If Ever Needed)

Local function calls within a shader module are rare—most shaders are single functions. Consider deferring this or limiting to:

- No recursion (simpler stack management)
- Fixed stack frame per function (no dynamic alloca)
- Inline small functions instead of calling

If we do add it:

```rust
fn emit_local_call(&mut self, target_offset: u32) {
    // Simple: save ra, jal to target, restore ra
    // Target must be within ±1MB (22-bit offset) or use far call sequence
}
```

---

## Avoiding ELF and Linking

### The Problem

Generating ELF, managing sections, handling relocations, then linking with builtins—all of this is complex and error-prone.

### The Solution: Pre-Linked Builtin Table

Don't link at compile time. Pre-link at firmware build time:

```rust
/// Built-in function addresses, populated at firmware startup
/// 
/// The JIT compiler receives this table and emits direct jumps to known addresses.
struct BuiltinTable {
    fadd_q32: u32,
    fsub_q32: u32,
    fmul_q32: u32,
    fdiv_q32: u32,
    fsqrt_q32: u32,
    // ... etc
}

/// At firmware boot:
/// 1. Load builtin ELF (or embedded symbols)
/// 2. Resolve all builtin addresses
/// 3. Store in global BUILTIN_TABLE

/// At JIT compile time:
fn compile_with_builtins(ir: &IrModule, builtins: &BuiltinTable) -> Vec<u8> {
    // Emit code that references BUILTIN_TABLE directly
    // No runtime linking, no relocations in JIT code
}
```

**Benefits**:

- JIT output is pure position-dependent code
- No relocation records to manage
- No ELF headers/sections
- Simple: emit bytes → mark executable → call

---

## Handling Large Constants and Jump Range

### Large Constants (>12-bit immediates)

RISC-V I-type has 12-bit signed immediates. For larger constants:

```rust
fn emit_large_const(&mut self, value: i32, dst: u8) {
    // lui dst, hi20(value)
    // addi dst, dst, lo12(value)
    
    let hi = (value + 0x800) >> 12; // Round then shift for addi sign extension
    let lo = value & 0xFFF;
    
    self.emit_u_type(hi as i32, dst, 0x37); // lui
    self.emit_i_type(lo, dst, 0, dst, 0x13); // addi
}
```

Or, for known constants, load from constant pool:

```rust
// At end of function, emit constant pool:
// .align 4
// const_0: .word 0x12345678

// Load:
// auipc tmp, %pcrel_hi(const_0)
// lw dst, %pcrel_lo(const_0)(tmp)
```

### Jump Islands

RISC-V B-type has 13-bit signed immediate (±4KB). J-type has 21-bit (±1MB).

**Strategy 1**: Assume JIT buffer < 1MB, use J-type for all jumps. Valid for our use case.

**Strategy 2**: For branches beyond ±4KB, invert and use far jump:

```rust
// Instead of: beq a0, a1, far_target (out of range)
// Emit:
//   bne a0, a1, skip      // Invert condition
//   j far_target          // J-type, ±1MB
// skip:
```

**Strategy 3**: Trampoline (if we ever exceed 1MB, unlikely):

```rust
// Within range of original branch:
//   beq a0, a1, trampoline
// ...
// trampoline:
//   lui t0, %hi(far_target)
//   jalr x0, %lo(far_target)(t0)  // tail call
```

Given our JIT buffer sizes (<64KB typically), ±4KB branches are sufficient for control flow within a function. For builtin calls, use absolute address loading.

---

## Learning from Cranelift Without Forking

### Study, Don't Fork

Cranelift's `cranelift-codegen/src/isa/riscv32/abi.rs` (39KB) contains the RISC-V ABI implementation. Read it to understand:

- Stack frame layout requirements
- Which registers are callee-saved
- How to handle struct returns
- Calling convention details

Then write a **minimal implementation** that only handles our use cases:


| Cranelift Handles                     | We Need                                 |
| ------------------------------------- | --------------------------------------- |
| All RISC-V extensions (F, D, V, etc.) | Just IM (integer + multiply)            |
| Dynamic stack allocation              | Fixed frame size per function           |
| Varargs                               | No varargs in shaders                   |
| Struct returns >2 words               | We handle via sret pointer, simple case |
| Exception handling                    | No exceptions                           |
| Tail call optimization                | Not needed                              |


### Key Files to Study (Not Copy)

From `lp-cranelift/cranelift/codegen/src/isa/riscv32/`:

- `abi.rs`: Frame layout, register assignment, calling convention
- `inst/emit.rs`: Instruction encoding details
- `lower.isle`: Instruction selection (overkill, but see patterns)

From your current `lpvm-cranelift/src/`:

- `emit/mod.rs`: Signature building, struct return handling
- `emit/call.rs`: How you currently handle calls to builtins
- `emit/memory.rs`: Stack slot management

---

## Concrete Simplification Checklist

### Immediate (Phase 0)

- No function calls at all
- Max 24 live virtual registers (spill not implemented)
- All immediates fit in 12 bits (or use lui+addi sequence)
- Single function per compilation unit
- No stack frame (leaf functions only)

### Phase 1

- Fixed-size stack frame per function
- Simple spill slots: `s0 + offset`
- No callee-saved registers (save/restore nothing)
- Return via `jalr x0, 0(ra)`

### Phase 2

- Builtin calls via custom ABI (not standard RISC-V psABI)
- Pre-linked builtin addresses (no runtime linking)
- Caller saves/restores ra only
- Arguments in a0-a3, returns in a0-a1

### Phase 3 (Deferred/Optional)

- Local function calls (only if benchmarks show need)
- Callee-saved register optimization
- Tail call optimization

---

## Debugging Strategy

### Use Emulator Aggressively

```rust
#[test]
fn test_single_op() {
    let ir = parse_module(r"
func @test(v1:i32) -> i32 {
  v2:i32 = iadd_imm v1, 42
  return v2
}").unwrap();
    
    let code = custom_lower(&ir).expect("compile");
    
    // Run in emulator
    let mut emu = RiscvEmu::new();
    emu.load_code(&code);
    emu.set_reg(10, 100); // a0 = 100
    emu.run_at(0x1000);
    
    assert_eq!(emu.get_reg(10), 142); // 100 + 42
}
```

### Differential Testing

For every test case:

1. Compile with Cranelift
2. Compile with custom backend
3. Run both in emulator
4. Assert identical results
5. Assert custom backend uses less memory to compile

### Symbolic Debugging

Emit metadata even if not ELF:

```rust
struct DebugInfo {
    pc_to_op: Vec<(usize, Op)>,        // Map code offset to LPIR op
    vreg_to_reg: Vec<(VReg, u8)>,      // Where each vreg ended up
    stack_layout: StackLayout,
}

// On crash/panic in emulator, print:
// "PC 0x104: executing Op::Iadd v3, v1, v2 (v3 in x15, v1 in x10, v2 in x11)"
```

---

## Success Metric: Complexity Budget

Track lines of code as a proxy for complexity:


| Component             | Budget   | Rationale                    |
| --------------------- | -------- | ---------------------------- |
| Instruction encoding  | 500      | Mechanical, low risk         |
| Instruction selection | 800      | 1:1 mapping, low risk        |
| Interval analysis     | 400      | Well-understood algorithm    |
| Linear scan allocator | 600      | Well-understood algorithm    |
| **ABI/Stack/Calls**   | **1000** | **This is the risk zone**    |
| Label resolution      | 200      | Simple backpatching          |
| Total                 | 3500     | Previous attempt failed here |


**Rule**: If ABI/stack code exceeds 1000 lines, stop and simplify. Cut features until it fits.

---

## Recommendation

1. **Start with Phase 0 only** (no calls, no spills, no stack)
2. **Validate with extensive emulator testing** before adding complexity
3. **Use custom "Shader ABI"** for builtins, don't implement RISC-V psABI
4. **Pre-link builtins** via address table, no ELF generation
5. **Study Cranelift's abi.rs for understanding**, write minimal version from scratch
6. **Set hard complexity budget** for ABI/stack code (1000 lines)

The previous attempt likely failed because it tried to build a "proper" compiler backend with full ABI compliance from day one. Instead, build a "shader compiler" that only handles the specific patterns we need, and add generality only when forced by test cases.