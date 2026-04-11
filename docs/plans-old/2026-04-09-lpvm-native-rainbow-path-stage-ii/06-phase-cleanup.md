# Phase 6: Cleanup & Validation

## Scope of Phase

Final cleanup, validation, and plan completion.

## Cleanup & Validation

### 1. Grep for TODOs and debug code

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native
grep -r "TODO\|FIXME\|println!\|dbg!" lp-shader/lpvm-native/src/ --include="*.rs" | grep -v "// TODO:"
```

Remove any temporary debug code added during development.

### 2. Fix all warnings

```bash
cargo check -p lpvm-native
cargo clippy -p lpvm-native 2>&1 | head -50
```

Fix any warnings about unused imports, dead code, etc.

### 3. Run full test suite

```bash
# Unit tests
cargo test -p lpvm-native

# Filetests - all control flow
./scripts/glsl-filetests.sh --target rv32lp.q32 "control/if_else/" --concise

# Filetests - key regression tests
./scripts/glsl-filetests.sh --target rv32lp.q32 "scalar/int/op-equal.glsl"
./scripts/glsl-filetests.sh --target rv32lp.q32 "scalar/int/op-divide.glsl"

# ESP32 build validation
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

### 4. Format code

```bash
cargo +nightly fmt -p lpvm-native
```

### 5. Verify exports in lib.rs

Ensure new VInst variants are exported if needed:
```rust
pub use vinst::{Br, BrIf, IcmpCond, IeqImm32, /* ... */};
```

## Plan Cleanup

### 1. Create summary

Write `summary.md`:

```markdown
# M2.2 Control Flow - Summary

## Completed Work

- Added `Br` and `BrIf` VInst variants
- Added RV32 branch encoders (beq, bne, jal)
- Implemented single-pass label backpatching
- Extended lowering for if/else control flow

## Filetests Passing

- control/if_else/basic.glsl
- control/if_else/nested.glsl
- control/if_else/chained.glsl

## Key Design Decisions

1. Explicit labels in VInst (not LPIR op indices)
2. Single-pass emit with deferred backpatching
3. Boolean BrIf with invert flag (not compare-and-branch)
4. Recursive block lowering for nested ifs

## Known Limitations

- Loops (while, for) not implemented - deferred to M2.x
- BrIfNot not implemented outside loops
- Switch statements not implemented
```

### 2. Move to done

```bash
mv docs/plans/2026-04-09-lpvm-native-rainbow-path-stage-ii/ \
   docs/plans-done/
```

## Commit

Create conventional commit:

```bash
git add lp-shader/lpvm-native/
git commit -m "feat(lpvm-native): control flow (if/else) for RV32

- Add Br, BrIf VInst variants with label resolution
- Add RV32 branch encoders: beq, bne, jal
- Implement single-pass label backpatching in emit
- Extend lowering for IfStart, Else, End ops
- Support nested if/else through recursive block lowering

Filetests passing:
- control/if_else/basic.glsl
- control/if_else/nested.glsl
- control/if_else/chained.glsl"
```

## Acceptance Criteria Check

- [ ] Simple if/else statements compile and execute
- [ ] Nested conditionals work
- [ ] Branch instructions use correct RV32 encodings
- [ ] Label resolution handles forward references
- [ ] Filetests for control flow pass
- [ ] No regressions in existing tests
- [ ] Code formatted, no warnings
- [ ] ESP32 build passes
