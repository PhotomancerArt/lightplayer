# Instruction Selection: LPIR to RV32

This document describes the direct mapping from LPIR operations to RISC-V 32-bit machine code.

## Design Philosophy

Unlike Cranelift's ISLE-based pattern matching, LPIR→RV32 uses **direct 1:1 or 1:N mapping**. Each LPIR opcode has a predetermined expansion to RV32 instructions based on:

- Operand types (already known in LPIR)
- Target features (M extension for multiply/divide, Zba/Zbb for bit manipulation)
- Float mode (F32 vs Q32)

## LPIR Operation Categories

### 1. Integer Arithmetic (ALU)


| LPIR Op    | RV32 Instructions    | Notes                         |
| ---------- | -------------------- | ----------------------------- |
| `iadd`     | `add rd, rs1, rs2`   | Register-register             |
| `isub`     | `sub rd, rs1, rs2`   |                               |
| `iadd_imm` | `addi rd, rs1, imm`  | 12-bit immediate              |
| `isub_imm` | `addi rd, rs1, -imm` | Negative immediate            |
| `imul`     | `mul rd, rs1, rs2`   | Requires M extension          |
| `idiv_s`   | `div rd, rs1, rs2`   | Signed divide, M ext          |
| `irem_s`   | `rem rd, rs1, rs2`   | Signed remainder, M ext       |
| `ineg`     | `sub rd, x0, rs`     | Negate via subtract from zero |


### 2. Bitwise Operations


| LPIR Op      | RV32 Instructions   | Notes                   |
| ------------ | ------------------- | ----------------------- |
| `iand`       | `and rd, rs1, rs2`  |                         |
| `ior`        | `or rd, rs1, rs2`   |                         |
| `ixor`       | `xor rd, rs1, rs2`  |                         |
| `inot`       | `xori rd, rs, -1`   | Bitwise NOT             |
| `ishl`       | `sll rd, rs1, rs2`  | Shift left logical      |
| `ishr_s`     | `sra rd, rs1, rs2`  | Shift right arithmetic  |
| `ishr_u`     | `srl rd, rs1, rs2`  | Shift right logical     |
| `ishl_imm`   | `slli rd, rs1, imm` | Immediate shift (5-bit) |
| `ishr_s_imm` | `srai rd, rs1, imm` |                         |
| `ishr_u_imm` | `srli rd, rs1, imm` |                         |


### 3. Integer Comparison

RISC-V comparisons produce 0/1 in destination:


| LPIR Op | RV32 Instructions                      |
| ------- | -------------------------------------- |
| `ieq`   | `sub rd, rs1, rs2` + `seqz rd, rd`     |
| `ine`   | `sub rd, rs1, rs2` + `snez rd, rd`     |
| `ilt_s` | `slt rd, rs1, rs2`                     |
| `ilt_u` | `sltu rd, rs1, rs2`                    |
| `igt_s` | `slt rd, rs2, rs1`                     |
| `igt_u` | `sltu rd, rs2, rs1`                    |
| `ile_s` | `slt rd, rs2, rs1` + `xori rd, rd, 1`  |
| `ile_u` | `sltu rd, rs2, rs1` + `xori rd, rd, 1` |
| `ige_s` | `slt rd, rs1, rs2` + `xori rd, rd, 1`  |
| `ige_u` | `sltu rd, rs1, rs2` + `xori rd, rd, 1` |


**Note**: The pseudoinstructions `seqz`/`snez` expand to `sltiu rd, rs, 1` / `sltu rd, x0, rs`.

### 4. F32 Float Operations (FloatMode::F32)

Requires F extension (not present on ESP32-C6). For completeness:


| LPIR Op | RV32F Instructions    |
| ------- | --------------------- |
| `fadd`  | `fadd.s rd, rs1, rs2` |
| `fsub`  | `fsub.s rd, rs1, rs2` |
| `fmul`  | `fmul.s rd, rs1, rs2` |
| `fdiv`  | `fdiv.s rd, rs1, rs2` |
| `fneg`  | `fsgnjn.s rd, rs, rs` |
| `fabs`  | `fsgnjx.s rd, rs, rs` |
| `fsqrt` | `fsqrt.s rd, rs`      |


### 5. Q32 Float Operations (FloatMode::Q32)

Q32 (Q16.16 fixed-point) is LightPlayer's primary mode. Operations lower to builtin calls:


| LPIR Op | RV32 Instructions                                                   | Notes                           |
| ------- | ------------------------------------------------------------------- | ------------------------------- |
| `fadd`  | `jal ra, __lp_lpir_fadd_q32`                                        | Via function pointer in context |
| `fsub`  | `jal ra, __lp_lpir_fsub_q32`                                        |                                 |
| `fmul`  | `jal ra, __lp_lpir_fmul_q32`                                        |                                 |
| `fdiv`  | `jal ra, __lp_lpir_fdiv_q32`                                        |                                 |
| `fneg`  | `sub rd, x0, rs`                                                    | Negate as integer               |
| `fabs`  | `srai tmp, rs, 31` + `xor rd, rs, tmp` + `sub rd, rd, tmp`          | Branchless abs                  |
| `fsqrt` | `jal ra, __lp_lpir_fsqrt_q32`                                       |                                 |
| `fmin`  | `blt rs1, rs2, use1` + `mv rd, rs2` + `j done` + `use1: mv rd, rs1` |                                 |
| `fmax`  | Similar to fmin with `bgt`                                          |                                 |


Some Q32 ops can be inlined when profitable:

- `fneg`: single `sub` instruction
- `fabs`: 3-instruction sequence (no branches)
- `fmin`/`fmax`: branchy but small; may be inlined or called

### 6. Memory Operations


| LPIR Op       | RV32 Instructions          | Notes                    |
| ------------- | -------------------------- | ------------------------ |
| `load` (i32)  | `lw rd, offset(rs1)`       | Base + offset addressing |
| `load` (ptr)  | `lw rd, offset(rs1)`       |                          |
| `store` (i32) | `sw rs2, offset(rs1)`      |                          |
| `store` (ptr) | `sw rs2, offset(rs1)`      |                          |
| `slot_addr`   | `addi rd, s0, slot_offset` | Frame pointer relative   |


LPIR `load`/`store` specify base and offset separately. RV32 immediate offsets are 12-bit signed.

### 7. Control Flow


| LPIR Op         | RV32 Instructions         | Notes                       |
| --------------- | ------------------------- | --------------------------- |
| `br_if_not`     | `beqz cond, exit_label`   | Branch if condition false   |
| `break`         | `j loop_exit`             | Direct jump to exit         |
| `continue`      | `j loop_continue`         | Direct jump to continue     |
| `return`        | `mv a0, retval` + `jr ra` | Or `ret` pseudoinstr        |
| `call` (local)  | `jal ra, offset`          | Relative offset to function |
| `call` (import) | `jal ra, addr`            | Absolute address of builtin |


## Instruction Encoding

RISC-V 32-bit instruction formats:

```rust
// R-type: register-register ALU
fn encode_r(funct7: u32, rs2: u8, rs1: u8, funct3: u32, rd: u8, opcode: u32) -> u32 {
    (funct7 << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) |
    (funct3 << 12) | ((rd as u32) << 7) | opcode
}

// I-type: immediate ALU, loads
fn encode_i(imm: i32, rs1: u8, funct3: u32, rd: u8, opcode: u32) -> u32 {
    let imm_bits = (imm as u32) & 0xFFF;
    (imm_bits << 20) | ((rs1 as u32) << 15) | (funct3 << 12) |
    ((rd as u32) << 7) | opcode
}

// S-type: stores
fn encode_s(imm: i32, rs2: u8, rs1: u8, funct3: u32, opcode: u32) -> u32 {
    let imm_bits = (imm as u32) & 0xFFF;
    let imm_hi = (imm_bits >> 5) & 0x7F;
    let imm_lo = imm_bits & 0x1F;
    (imm_hi << 25) | ((rs2 as u32) << 20) | ((rs1 as u32) << 15) |
    (funct3 << 12) | (imm_lo << 7) | opcode
}

// B-type: branches
fn encode_b(imm: i32, rs2: u8, rs1: u8, funct3: u32, opcode: u32) -> u32 {
    let imm_bits = (imm as u32) & 0x1FFF; // 13-bit signed (12:1 + implicit 0)
    let imm_12 = (imm_bits >> 12) & 0x1;
    let imm_10_5 = (imm_bits >> 5) & 0x3F;
    let imm_4_1 = (imm_bits >> 1) & 0xF;
    let imm_11 = (imm_bits >> 11) & 0x1;
    
    (imm_12 << 31) | (imm_10_5 << 25) | ((rs2 as u32) << 20) |
    ((rs1 as u32) << 15) | (funct3 << 12) |
    (imm_4_1 << 8) | (imm_11 << 7) | opcode
}

// U-type: lui, auipc
fn encode_u(imm: i32, rd: u8, opcode: u32) -> u32 {
    let imm_bits = (imm as u32) & 0xFFFFF000;
    imm_bits | ((rd as u32) << 7) | opcode
}

// J-type: jal
fn encode_j(imm: i32, rd: u8, opcode: u32) -> u32 {
    let imm_bits = (imm as u32) & 0x1FFFFF; // 21-bit signed (20:1 + implicit 0)
    let imm_20 = (imm_bits >> 20) & 0x1;
    let imm_10_1 = (imm_bits >> 1) & 0x3FF;
    let imm_11 = (imm_bits >> 11) & 0x1;
    let imm_19_12 = (imm_bits >> 12) & 0xFF;
    
    (imm_20 << 31) | (imm_10_1 << 21) | (imm_11 << 20) |
    (imm_19_12 << 12) | ((rd as u32) << 7) | opcode
}
```

## Example: Complete Function Lowering

Input LPIR:

```
func @add(v1:i32, v2:i32) -> i32 {
  v3:i32 = iadd v1, v2
  return v3
}
```

Lowered RV32 (assuming parameters in a0, a1, result in a0):

```asm
# No prologue needed (leaf function, no spills)
add a0, a0, a1   # v3 = iadd v1, v2
ret              # return v3 (in a0)
```

Encoding:

```
add:  0x00b50533  # 0000000 01011 01010 000 01010 0110011
ret:  0x00008067  # 0000000 00000 00001 000 00000 1100111 (jalr x0, 0(ra))
```

More complex example with Q32:

```
func @lerp(v1:f32, v2:f32, v3:f32) -> f32 {
  # Q32 mode: f32 values are actually i32 fixed-point
  v4:f32 = fsub v2, v1      # (b - a)
  v5:f32 = fmul v4, v3      # (b - a) * t
  v6:f32 = fadd v1, v5      # a + (b - a) * t
  return v6
}
```

Lowered RV32 (Q32, builtins at known addresses):

```asm
# v1 in a0, v2 in a1, v3 in a2
# Save ra to stack (non-leaf)
addi sp, sp, -4
sw ra, 0(sp)

# v4 = fsub(v2, v1)
mv a0, a1           # Arg 0 = v2
mv a1, a0           # Arg 1 = v1 (clobber! careful)
# Actually need to preserve v1, v3 across calls
# ... register allocation decides ...

# Simplified with spills:
sw a0, 8(sp)        # Spill v1
sw a2, 12(sp)       # Spill v3

mv a0, a1           # a0 = v2
lw a1, 8(sp)        # a1 = v1
jal ra, __lp_lpir_fsub_q32  # Result in a0
sw a0, 16(sp)       # Spill v4

# v5 = fmul(v4, v3)
lw a1, 12(sp)       # a1 = v3
jal ra, __lp_lpir_fmul_q32  # a0 = v4 * v3
sw a0, 16(sp)       # Spill v5

# v6 = fadd(v1, v5)
lw a0, 8(sp)        # a0 = v1
lw a1, 16(sp)       # a1 = v5
jal ra, __lp_lpir_fadd_q32  # a0 = result

# Result in a0, restore and return
lw ra, 0(sp)
addi sp, sp, 4
ret
```

This shows why register allocation matters: with good allocation, we could keep v1 in s0, v3 in s1, avoiding most spills.

## Instruction Selection Implementation Sketch

```rust
struct Emitter<'a> {
    output: &'a mut [u8],
    pos: usize,
    vreg_map: Vec<Loc>,  // From register allocator
    labels: Vec<Label>,
}

impl<'a> Emitter<'a> {
    fn emit_op(&mut self, op: &Op, pc: usize) -> Result<(), EmitError> {
        match op {
            Op::Iadd { dst, lhs, rhs } => {
                let rd = self.vreg_to_phys(*dst)?;
                let rs1 = self.vreg_to_phys(*lhs)?;
                let rs2 = self.vreg_to_phys(*rhs)?;
                let inst = encode_r(0b0000000, rs2, rs1, 0b000, rd, 0b0110011);
                self.emit_u32(inst);
            }
            
            Op::IaddImm { dst, src, imm } => {
                let rd = self.vreg_to_phys(*dst)?;
                let rs1 = self.vreg_to_phys(*src)?;
                let imm12 = Self::imm_to_i12(*imm)?;
                let inst = encode_i(imm12, rs1, 0b000, rd, 0b0010011);
                self.emit_u32(inst);
            }
            
            Op::BrIfNot { cond } => {
                let rs = self.vreg_to_phys(*cond)?;
                // Emit beqz rs, exit_label (placeholder)
                let inst = encode_b(0, 0, rs, 0b000, 0b1100011); // beq rs, x0, offset
                // Record for backpatch
                self.labels.push(Label::LoopExit { pc, pos: self.pos });
                self.emit_u32(inst);
            }
            
            Op::Fadd { dst, lhs, rhs } if ctx.float_mode == FloatMode::Q32 => {
                // Q32: emit call to builtin
                self.emit_q32_call(dst, lhs, rhs, BuiltinId::LpirFaddQ32)?;
            }
            
            // ... other ops
        }
        Ok(())
    }
    
    fn vreg_to_phys(&self, vreg: VReg) -> Result<u8, EmitError> {
        match self.vreg_map[vreg.0 as usize] {
            Loc::Reg(r) => Ok(r),
            Loc::Spill(slot) => {
                // Need to emit load from stack
                // Returns temporary register holding the value
                self.emit_spill_load(slot)
            }
        }
    }
    
    fn emit_spill_load(&mut self, slot: u8) -> Result<u8, EmitError> {
        // Pick a temporary register (x5-x7 = t0-t2)
        let tmp = 5; // t0
        let offset = self.spill_offset(slot);
        let inst = encode_i(offset, 2, 0b010, tmp, 0b0000011); // lw tmp, offset(sp)
        self.emit_u32(inst);
        Ok(tmp)
    }
}
```

## Summary

Instruction selection for LPIR→RV32 is direct mapping without pattern matching complexity. The main challenges are:

1. **Register allocation**: Where to keep values (handled separately)
2. **Branch resolution**: Forward jumps need label backpatching
3. **Calling convention**: Correct setup for builtin calls

The actual encoding is mechanical: ~800 lines of straightforward match statements.