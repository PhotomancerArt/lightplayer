# Milestone 2.1: Core Integer Operations

**Goal**: Lower integer arithmetic, comparisons, and selection operations required for rainbow.

## Suggested Plan

`lpvm-native-m2-1-core-integer`

## Scope

### In Scope

- **Arithmetic**: idiv, irem (iadd, isub, imul already done)
- **Comparisons**: icmp (eq, ne, slt, sle, sgt, sge)
- **Selection**: select (ternary-like operations for smoothstep, mix)
- **Constants**: All iconst variants (already have iconst32, may need wider)

### Out of Scope

- Control flow (M2.2)
- Function calls (M2.3)
- Float operations (M2.4)
- 64-bit ops (stub only)

## Key Decisions

1. **Integer division**: Lower to div/rem instructions on RV32 (no builtin needed)
2. **Comparisons**: Lower to icmp VInst, emit as RV32 slt/sltu + conditional moves
3. **Select**: Branchless implementation using slt + xor + and sequence

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| Extended `lower_op` | `lower.rs` | idiv, irem, icmp, select |
| `VInst` additions | `vinst.rs` | Div32, Rem32, Icmp, Select |
| Branchless select | `isa/rv32/emit.rs` | Emit slt-based conditional move |
| Comparison emission | `isa/rv32/emit.rs` | slt/sltu for integer compares |
| Tests | `lower.rs`, `emit.rs` | Unit tests for each operation |
| Filetests | `filetests/scalar/` | icmp.glsl, select.glsl |

## Dependencies

- M1: ABI with spills, sret complete
- Current lower.rs has iadd, isub, imul, iconst

## Estimated Scope

- **Lines**: ~250-350
- **Files**: 2-3 modified (`lower.rs`, `vinst.rs`, `emit.rs`)
- **Time**: 1-2 days

## Acceptance Criteria

1. `scalar/icmp.glsl` passes (integer comparisons)
2. `scalar/select.glsl` passes (ternary selection)
3. Division and remainder operations emit RV32 div/rem instructions
4. All new ops have unit tests
5. No regressions in existing tests
