# M2.2 Control Flow Design

## Scope of Work

Implement branching and control flow for if/else statements in the RV32 backend. This includes block labels, conditional branches, and label resolution with forward reference handling.

## File Structure

```
lp-shader/lpvm-native/src/
├── vinst.rs              # UPDATE: Add Br, BrIf VInst variants
├── lower.rs              # UPDATE: Handle IfStart, Else, End, BrIfNot ops
├── isa/rv32/
│   ├── inst.rs           # UPDATE: Add beq, bne, jal encoders
│   └── emit.rs           # UPDATE: Label backpatching in emit_function_bytes
└── lib.rs                # UPDATE: Re-export new VInst variants
```

## Conceptual Architecture

### Data Flow

```
LPIR Control Flow            VInst Lowering              RV32 Emission
─────────────────────────────────────────────────────────────────────────────

IfStart {                   BrIf {                      beq cond, x0, L_else
  cond: v0,                   cond: v0,
  else_offset: 5,              target: L0,
  end_offset: 8                invert: true
}                           }
   │                              │
   ▼                              ▼
[then ops 2..4]             [lowered then VInsts]
   │                              │
   ▼                              ▼
Else                        Br { target: L1 }
   │                              │
   ▼                              ▼
[else ops 5..7]   ───────►  Label { id: L0 }
   │                           [lowered else VInsts]
   ▼                              │
End                             ▼
                              Label { id: L1 }
```

### Label Resolution (Single-Pass with Backpatching)

1. **On `Label(id)`**: Record `label_offset[id] = code.len()`. Resolve any pending fixups for this label.

2. **On `Br`/`BrIf` to unknown target**: Emit placeholder instruction (4 bytes). Record fixup `(byte_offset, target_label_id)`.

3. **On `Br`/`BrIf` to known target**: Calculate PC-relative offset and emit correct instruction immediately.

4. **Post-loop**: Verify all fixups resolved (should be empty if all labels defined before use).

### VInst Variants

```rust
// Unconditional branch
Br {
    target: LabelId,
    src_op: Option<u32>,
}

// Conditional branch
BrIf {
    cond: VReg,
    target: LabelId,
    invert: bool,  // if true, branch when cond == 0
    src_op: Option<u32>,
}
```

### RV32 Instruction Selection

| VInst | RV32 Encoding |
|-------|---------------|
| `Br { target }` | `jal x0, offset` (unconditional jump) |
| `BrIf { cond, target, invert: true }` | `beq cond, x0, offset` (branch if false) |
| `BrIf { cond, target, invert: false }` | `bne cond, x0, offset` (branch if true) |
| `Label(id)` | No code emitted, records offset |

Note: Branch offsets are PC-relative, in bytes, and must fit in 12-bit signed immediate (±4KB range for branches, ±1MB for jal).

## Main Components

### 1. VInst Extensions (`vinst.rs`)

Add `Br` and `BrIf` variants to the `VInst` enum. Update `src_op()`, `defs()`, and `uses()` methods.

### 2. RV32 Encoders (`isa/rv32/inst.rs`)

Add B-type instruction encoders:
- `encode_beq(rs1, rs2, imm)`
- `encode_bne(rs1, rs2, imm)`
- `encode_jal(rd, imm)` (J-type, used for unconditional with rd=x0)

### 3. Label Resolution (`isa/rv32/emit.rs`)

Extend `EmitContext` with:
- `label_offsets: Vec<Option<usize>>` — map label_id to byte offset
- `fixups: Vec<(usize, LabelId)>` — pending branches to resolve

Add methods:
- `record_label(id)` — record current code offset for label
- `emit_branch_placeholder(target)` — emit 4-byte placeholder, queue fixup
- `resolve_fixups_for_label(id)` — patch all pending branches to this label
- `final_backpatch_check()` — verify no unresolved fixups remain

Modify `emit_function_bytes` to use these during VInst iteration.

### 4. Control Flow Lowering (`lower.rs`)

Extend `lower_op` to handle control flow ops:
- `Op::IfStart { cond, else_offset, end_offset }` → emit `BrIf` with invert=true targeting else label, then lower then-block
- `Op::Else` → emit `Br` to end label, then emit else label
- `Op::End` → emit end label
- `Op::BrIfNot { cond }` → emit `BrIf` with invert=true targeting innermost loop end (or handle via loop context)

This requires restructuring `lower_ops` from a simple loop to a recursive descent that can process blocks.

## Design Decisions

1. **Explicit Labels**: Use `LabelId` in VInst rather than LPIR op indices. Direct mapping, cleaner abstraction.

2. **Single-Pass Backpatching**: Emit code in one pass, defer forward branch resolution. Simpler than two-pass, efficient for typical function sizes.

3. **Boolean Branches**: `BrIf` tests a boolean VReg (0/1) against x0. LPIR comparisons already produce boolean results, so no need for compare-and-branch at VInst level.

4. **No Loops in Scope**: `LoopStart`, `Break`, `Continue` deferred to future milestone. This keeps M2.2 focused and testable.

## Acceptance Criteria

1. `control/if_else/basic.glsl` passes on rv32lp.q32
2. `control/if_else/nested.glsl` passes
3. All new VInsts have unit tests
4. Label backpatching handles forward references correctly
5. No regressions in existing tests
