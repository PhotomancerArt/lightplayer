# M2.2 Control Flow Plan Notes

Date: 2026-04-09
Roadmap: lpvm-native-rainbow-path
Stage: ii (M2.2)

## Scope of Work

Implement branching and control flow for if/else statements in the RV32 backend.

### In Scope
- Block labels and branch targets
- Conditional branches (br_if) for if/else
- Basic block lowering from LPIR
- Label resolution (forward references)

### Out of Scope
- Loops (while, for) - deferred to future milestone
- Switch statements
- Indirect branches

## Current State of Codebase

### LPIR Control Flow Ops (already exist in `lpir/src/op.rs`)
- `IfStart { cond, else_offset, end_offset }` - marks beginning of if, with skip offsets
- `Else` - marks else block start
- `End` - marks end of control flow construct
- `BrIfNot { cond }` - branch if condition is false

### Current VInst (`lpvm-native/src/vinst.rs`)
- Has `Label(LabelId, Option<u32>)` variant already defined
- No branch instructions yet

### Current Lowering (`lpvm-native/src/lower.rs`)
- `lower_op()` handles individual straight-line ops
- `lower_ops()` iterates through function body linearly
- No block-based lowering yet

### Current Emission (`lpvm-native/src/isa/rv32/emit.rs`)
- Has instruction encoders for arithmetic, comparison
- No branch encoders (beq, bne, jal) yet
- No label resolution mechanism

## Key Questions

### Q1: Lowering Strategy ✓ ANSWERED
**Context**: LPIR uses flat op stream with `IfStart`/`Else`/`End` markers and byte offsets. We need to convert this to a block-based representation with explicit labels and branches for RV32 emission.

**Options**:
a) Lower LPIR markers directly to VInst labels + branches during `lower_ops`, doing a two-pass to resolve offsets to labels
b) First convert LPIR to a block structure (CFG), then lower each block
c) Keep flat structure but use PC-relative calculations at emit time

**Decision**: Option (a) — use explicit labels in VInst.

Rationale: Labels provide a direct `label_id → byte_offset` mapping without needing to track LPIR op indices through lowering. Both approaches need backpatching for forward branches, but labels are cleaner and generalize better to future control flow (loops, etc.). Memory overhead is negligible (a few dozen entries per function).

### Q2: Label Resolution Approach
**Context**: RV32 branch instructions use PC-relative offsets. We need to resolve labels to actual byte offsets.

**Options**:
a) Two-pass emit: first pass collect label positions, second pass emit with resolved offsets
b) Single pass with backpatching: emit placeholder, record position, patch after label is seen
c) Calculate offsets ahead of time during lowering

**Decision**: Option (b) — single-pass with deferred backpatching.

Rationale: One pass over VInsts is sufficient. When emitting a branch to a forward label, emit a placeholder and record `(byte_offset, target_label)`. When the target label is encountered, resolve any pending fixups. After the loop, verify all fixups are resolved. This avoids a second iteration over all VInsts.

### Q3: Branch VInst Design ✓ ANSWERED
**Context**: We need VInst variants to represent branches for lowering LPIR control flow.

**Options**:
A) Simple branches with boolean condition
- `Br { target: LabelId }` — unconditional
- `BrIf { cond: VReg, target: LabelId, invert: bool }` — branch if (cond != 0) or (cond == 0)

B) RV32-style compare-and-branch
- `Br`, `Beq { rs1, rs2, target }`, `Bne { rs1, rs2, target }`, etc.

**Decision**: Option A — `Br` and `BrIf` with `invert` flag.

Rationale: LPIR already produces boolean results in VRegs via `Icmp32` and `IeqImm`. `BrIf { cond, target, invert=true }` directly maps `IfStart` semantics (branch to else when cond is false). No need for compare-and-branch combos at the VInst level.

## Notes

- Need to add RV32 branch encoders: `beq`, `bne`, `blt`, `bge`, `jal`
- Need to handle forward references (label used before defined)
- The LPIR `else_offset` and `end_offset` are op indices, not byte offsets - we need to map these to labels
