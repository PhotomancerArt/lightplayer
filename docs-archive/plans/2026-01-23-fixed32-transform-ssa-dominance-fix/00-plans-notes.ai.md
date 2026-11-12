# Plan: Fix SSA Dominance Violation in Q32 Transform

## Problem Summary

The Q32 transform is creating constants in blocks where they're first encountered, rather than in blocks that dominate all their uses. This causes SSA dominance violations when constants are used in blocks that can be reached via paths that don't go through the block where the constant was created.

**Example:**
- `v108 = iconst.i8 1` is created in `block9`
- `v108` is used in function calls in `block2`
- `block2` can be reached from `block1` without going through `block9`
- This violates SSA dominance: "uses value v108 from non-dominating inst99"

## Root Cause

The `map_value()` function in `instruction_copy.rs` has a critical bug:
```rust
*value_map.get(&resolved_value).unwrap_or(&resolved_value)
```

**The Bug:** When a value is not in `value_map`, `map_value()` returns the original Value from the OLD function. This is fundamentally wrong because:
1. Values are function-scoped - you cannot use a Value from one function in another function
2. If a value isn't in `value_map`, it means it hasn't been copied/transformed yet, which is an error condition

**Why it causes dominance violations:** When `map_value()` returns an old Value, that Value's instruction is in the old function. When we use it in the new function, Cranelift's verifier sees a Value from a different function context being used, and since the instruction that created it (in the old function) is in a different block than where it's used (in the new function), we get an SSA dominance violation.

**Why it worked before:** The old arithmetic code created constants inline in the same block where they were used, so the constants were always in `value_map` when needed. The new code doesn't create these constants, exposing the underlying bug in `map_value()`.

**The Real Question:** Why isn't `v108` in `value_map` when the `select` instruction tries to use it? If we're copying instructions block-by-block in order, and `v108 = iconst.i8 1` appears right before `select v107, v108, v109`, then `v108` should already be in `value_map` because we just copied the `iconst` instruction. 

**Investigation Needed:** We need to understand:
1. Are `v108` and `v109` coming from the old function, or are they being created newly during the transform?
2. If they're from the old function, why aren't they in `value_map` when the `select` tries to use them?
3. If they're being created newly, where and when are they being created?

**The Immediate Fix:** Regardless of the root cause, `map_value()` should NEVER return an old Value. It should fail if a value isn't found, forcing us to fix the underlying issue.

## Questions

### Q1: Where should constants be created?

**Context:** Constants need to be available in all blocks that use them. In SSA form, this means they must be created in a block that dominates all their uses.

**Options:**
1. **Entry block** - Create all constants in the entry block before processing any other instructions
2. **First use block** - Create constants in the first block where they're used (current behavior, but broken)
3. **Dominating block** - Calculate the dominator tree and create constants in the lowest common dominator

**Suggested Answer:** Option 1 (entry block) - Simplest and most reliable. All constants will be available everywhere since the entry block dominates all other blocks.

### Q2: When should constants be identified and created?

**Context:** We need to identify all constants that will be needed before we start copying instructions, so we can create them in the entry block.

**Options:**
1. **Pre-pass** - Scan all instructions in the old function, identify all constants, create them in entry block before copying any instructions
2. **Lazy creation** - When encountering a constant during instruction copying, check if it exists in value_map, if not create it in entry block
3. **Hybrid** - Pre-pass for constants used as instruction results, lazy for constants used as arguments

**Suggested Answer:** Option 1 (pre-pass) - Most straightforward and ensures all constants are available before any instruction copying begins.

### Q3: How to handle constants that are instruction results vs arguments?

**Context:** Some constants come from `iconst` instructions (results), others are used as arguments to instructions like `select`.

**Options:**
1. **Treat all the same** - Scan for both `iconst` instructions and constant arguments, create all in entry block
2. **Different handling** - Copy `iconst` instructions normally (they'll be in value_map), only handle constant arguments specially
3. **Unify approach** - Always create constants in entry block, even if they come from `iconst` instructions

**Suggested Answer:** Option 1 - Treat all constants the same. Scan for `iconst` instructions and create their results in the entry block. Also scan instruction arguments for constants and ensure they're created in entry block.

### Q4: How to detect constants in instruction arguments?

**Context:** We need to identify when an instruction argument is a constant value (from an `iconst`, `f32const`, etc.) so we can ensure it's created in the entry block.

**Options:**
1. **Check instruction type** - For each instruction argument, check if it's the result of a constant instruction
2. **Use DFG methods** - Use Cranelift's DFG to check if a value is a constant
3. **Track during scan** - When scanning instructions, track which values are constants

**Suggested Answer:** Option 2 - Use Cranelift's DFG methods to check if a value is a constant. This is the most reliable approach.

### Q5: Should we modify `map_value()` or handle constants separately?

**Context:** The current `map_value()` function returns original values when they're not in value_map, which is WRONG because Values are function-scoped. We can't use old Values in the new function.

**Options:**
1. **Fix `map_value()` first** - Make it panic/error when a value isn't in value_map (fail fast). This will expose the real bug.
2. **Pre-create all constants** - Scan all instructions, identify all constants, create them in entry block before copying any instructions
3. **Hybrid** - Fix `map_value()` to fail fast, then investigate why values aren't in value_map, then potentially pre-create constants

**Suggested Answer:** Option 1 first - Fix `map_value()` to fail fast. This will help us understand the real issue. Then we can decide if we need to pre-create constants or fix something else.

### Q6: What about constants that are block parameters or function parameters?

**Context:** Not all values are constants - some are parameters, some are results of other instructions.

**Options:**
1. **Only handle true constants** - Only create `iconst`, `f32const`, etc. results in entry block
2. **Handle all immediate values** - Also handle immediate values in instruction data
3. **Current approach is fine** - Parameters are already handled correctly, only need to fix constants

**Suggested Answer:** Option 1 - Only handle true constant instructions (`iconst`, `f32const`, etc.). Parameters and other values are already handled correctly by the existing transform infrastructure.

## Notes

- The bug was exposed by recent changes that removed inline constant creation from arithmetic operations
- The underlying issue existed before but wasn't triggered because constants were created in the same blocks where they were used
- The fix needs to be in the shared transform infrastructure, not just the Q32 transform
- Need to ensure the fix doesn't break other transforms (identity transform, etc.)
