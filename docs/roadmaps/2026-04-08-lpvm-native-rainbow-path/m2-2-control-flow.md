# Milestone 2.2: Control Flow

**Goal**: Implement branching and control flow for if/else in rainbow.

## Suggested Plan

`lpvm-native-m2-2-control-flow`

## Scope

### In Scope

- **Labels**: Block labels for branch targets
- **Branches**: br (unconditional), br_if (conditional)
- **Block structure**: Basic block lowering from LPIR
- **Conditional execution**: if/else lowering (no loops yet)

### Out of Scope

- Loops (while, for) — deferred to future milestone
- Indirect branches
- Switch statements
- Exception handling

## Key Decisions

1. **Block-based lowering**: LPIR is already in blocks, lower directly to labeled VInsts
2. **Branch emission**: br → jal, br_if → beq/bne with label resolution
3. **Label resolution**: Two-pass emit (collect labels, emit code) or pc-relative offsets
4. **Straight-line fallback**: For simple if/else, consider predication if rainbow needs it

## Deliverables


| Deliverable       | Location           | Description                           |
| ----------------- | ------------------ | ------------------------------------- |
| `VInst` additions | `vinst.rs`         | Br, BrIf, Label                       |
| `lower_function`  | `lower.rs`         | Full block-based lowering             |
| Block lowering    | `lower.rs`         | Convert LPIR blocks to VInst sequence |
| Branch emission   | `isa/rv32/emit.rs` | jal, beq, bne, blt, bge emission      |
| Label resolution  | `isa/rv32/emit.rs` | Resolve LabelId to byte offsets       |
| Tests             | `emit.rs`          | Branch and label tests                |
| Filetests         | `filetests/`       | if-else.glsl, branch.glsl             |


## Dependencies

- M2.1: Core integer operations (icmp needed for conditions)

## Estimated Scope

- **Lines**: ~200-300
- **Files**: 3 modified (`lower.rs`, `vinst.rs`, `emit.rs`)
- **Time**: 1-2 days

## Acceptance Criteria

1. Simple if/else statements compile and execute correctly
2. Nested conditionals work
3. Branch instructions use correct RV32 encodings (beq, bne, jal)
4. Label resolution handles forward references
5. Filetests for control flow pass