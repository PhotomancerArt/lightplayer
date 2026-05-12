# FA3 Perf M4: VInst Rearchitecture — Notes

## Scope of Work

Redesign the VInst layer in `lpvm-native` to model RISC-V instruction formats
rather than LPIR semantics. Replace 15+ individual binary arithmetic variants
with `AluRRR { op: AluOp }` (R-type) and `AluRRI { op: AluImmOp }` (I-type).
Add immediate folding at lowering time. Generalize `IeqImm32` to `IcmpImm`.

This is Milestone 4 of the `fa3-perf` roadmap, expanded from "add immediate
folding" to "fix the VInst abstraction so immediate folding is native."

## Current State

### VInst (vinst.rs)

- 28 enum variants, 15 of which are binary arithmetic with identical
  `{ dst: VReg, src1: VReg, src2: VReg, src_op: u16 }` shape
- `for_each_def`, `for_each_use`, `src_op`, `mnemonic`, `format_alloc_trace_detail`
  each have 28+ match arms
- `IeqImm32` is the only immediate-form variant (ad-hoc, equality only)
- Size assertion: `size_of::<VInst>() <= 32`

### Lowering (lower.rs)

- 1:1 mapping from LpirOp to VInst (Iadd → Add32, Isub → Sub32, etc.)
- LPIR already has `IaddImm`, `IsubImm`, `ImulImm`, `IshlImm`, `IshrSImm`,
  `IshrUImm`, `IeqImm` variants — the FA lowering ignores all except `IeqImm`
- No use-count analysis, no constant folding

### Emitter (rv32/emit.rs)

- 15 nearly-identical match arms for binary ops, all doing:
  `use_vreg(1) + use_vreg(2) + def_vreg(0) + push_u32(encode_X())`

### Encoding (rv32/encode.rs)

- Has: `encode_addi`, `encode_xori`, `encode_sltiu`
- Missing: `encode_andi`, `encode_ori`, `encode_slti`, `encode_slli`,
  `encode_srli`, `encode_srai`, `encode_blt/bge/bltu/bgeu`

### Allocator (fa_alloc/walk.rs)

- Only inspects VInst shape through `for_each_def`/`for_each_use`
- Special-cases `Mov32` for copy coalescing and `Call` for ABI constraints
- Does not care about opcode — only operand count

### Debug/vinst parser (debug/vinst.rs)

- Text format parser/formatter for filetests
- 28+ match arms in `format_vinst`, ~15 in `parse_def_instruction`

### Filetests

- `filetests/alloc/spill/` — 7 files
- `filetests/call/` — 7 files
- `filetests/param/` — 4 files
- All use text VInst format; snapshots will need updating

## Questions

### Q1: Should `Neg` and `Bnot` become `AluRRI` or stay as separate variants?

**Context**: `neg rd, rs` is a pseudo for `sub rd, x0, rs` (R-type with rs1=x0).
`bnot rd, rs` is `xori rd, rs, -1` (I-type with imm=-1).

**Suggested answer**: Keep them as separate unary variants (`Neg`, `Bnot`).
They have a different operand shape (1 def, 1 use) from `AluRRR` (1 def, 2 uses)
and `AluRRI` (1 def, 1 use, 1 imm). Folding them into the generic shapes would
require x0 as a sentinel vreg or a special imm value, adding complexity for no
real benefit.

**Answer**: Keep as separate unary variants. Simple, clean, correct operand shape.

### Q2: Should `Mov` become `AluRRI { op: Addi, imm: 0 }` or stay separate?

**Context**: `mv rd, rs` is `addi rd, rs, 0`. The allocator special-cases `Mov32`
for copy coalescing.

**Suggested answer**: Keep `Mov` separate. The allocator needs to identify copies
for coalescing, and a dedicated variant is clearer than pattern-matching on
`AluRRI { op: Addi, imm: 0, .. }`. The emitter already handles it specially.

**Answer**: Keep `Mov` separate for copy coalescing.

### Q3: Should `Icmp` remain a pseudo or decompose into `AluRRR { op: Slt/SltU }`?

**Context**: RISC-V has `slt`/`sltu` (R-type) and `slti`/`sltiu` (I-type) for
`<` comparisons, but `==`, `!=`, `<=`, `>=` need multi-instruction sequences.

**Suggested answer**: Keep `Icmp` and `IcmpImm` as pseudo-instructions that
expand in the emitter. The multi-instruction sequences don't fit cleanly into
`AluRRR`/`AluRRI`, and the emitter already handles the expansion correctly.

**Answer**: Keep as pseudo. The emitter handles the multi-instruction expansion.

### Q4: How to handle `SubImm` given RISC-V has no `subi`?

**Context**: `sub rd, rs, imm` must be encoded as `addi rd, rs, -imm`. But
`-(-2048)` = `2048` which doesn't fit in 12 bits.

**Suggested answer**: During lowering, when folding `Isub` with a constant rhs:
negate the constant and check if it fits. If `-val` is in `-2048..2047`, emit
`AluRRI { op: Addi, imm: -val }`. Otherwise fall back to `IConst32` + `AluRRR { op: Sub }`.
No `SubImm` variant needed — subtraction of an immediate is just addition of the
negated immediate.

**Answer**: Map to `AluRRI { op: Addi, imm: -val }` when `-val` fits. No SubImm needed.

### Q5: Should `ImulImm` from LPIR be handled?

**Context**: RISC-V has no `muli` instruction. LPIR has `ImulImm` but it would
need to be materialized as `IConst32` + `AluRRR { op: Mul }` anyway.

**Suggested answer**: Don't add a `MulImm` variant. When the lowering encounters
`LpirOp::ImulImm`, emit `IConst32` + `AluRRR { op: Mul }`. Power-of-2 multiply
could be lowered to `AluRRI { op: Slli }` as a future optimization.

**Answer**: No MulImm variant. Materialize as IConst32 + Mul. Power-of-2 → Slli later.

## Notes

- The `AluOp` and `AluImmOp` enums should be `#[repr(u8)]` for compact size.
- RISC-V shift immediates (slli/srli/srai) use 5-bit shamt (0..31), not full
  12-bit immediate. The lowering should validate this range.
- The refactoring is mechanical — every match arm on VInst needs updating, but
  the pattern is always "collapse N arms into 1."
- Net line count should decrease despite adding new functionality.

## Filetest / perf baseline

Before this work, the filetest suite summary was:

                  pass    fail   unimpl  unsupported  compile-fail    total inst  vs fastest
     rv32c.q32    4423       0      675           52            78  558,352 inst       1.00×
     rv32n.q32    4423       0      675           52            78  610,155 inst       1.09×
      wasm.q32    4406       0      688           56            81             —           —
