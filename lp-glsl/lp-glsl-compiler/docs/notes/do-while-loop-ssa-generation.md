# Do-While Loop SSA Generation Issue

## Problem

When generating Cranelift IR for do-while loops, the `cond_block` fails to create proper block parameters for variables that are modified in the `body_block` and then used in the `cond_block`. Instead, Cranelift creates aliases (e.g., `v7 -> v6`) which prevents proper SSA construction.

## Symptoms

- Do-while loop tests fail with incorrect results (e.g., expected 10, got 0)
- `cond_block` has 0 block parameters when it should have at least 1
- Variables used in `cond_block` that were modified in `body_block` are accessed via aliases instead of block parameters
- The generated CLIF shows `cond_block` using values directly without block parameters, but the back edge to `body_block` doesn't pass the correct arguments

## Root Cause

The issue occurs when `cond_block` is sealed **after** using variables in it. When `use_var` is called on an unsealed block with a single predecessor, Cranelift's `SSABuilder.find_var` function follows the single-predecessor chain to find variable definitions. However, if the block is not sealed, it cannot properly optimize the SSA construction.

The key insight is that Cranelift can optimize SSA construction for single-predecessor blocks by using values directly (without block parameters) **only if the block is sealed first**. When sealed, Cranelift recognizes the single-predecessor relationship and can safely use values directly, passing them as arguments when branching back.

## Solution

**Seal `cond_block` BEFORE using variables in it.**

### Correct Order:

```rust
// Switch to cond_block
ctx.switch_to_block(cond_block);

// CRITICAL: Seal cond_block BEFORE using variables
ctx.seal_block(cond_block);

// Now use variables - Cranelift will optimize for single-predecessor
let condition_value = ctx.translate_expr(condition)?;
ctx.emit_cond_branch(condition_value, body_block, exit_block)?;
```

### Incorrect Order (what causes the bug):

```rust
// Switch to cond_block
ctx.switch_to_block(cond_block);

// BUG: Using variables before sealing
let condition_value = ctx.translate_expr(condition)?;
ctx.emit_cond_branch(condition_value, body_block, exit_block)?;

// Too late: sealing after use_var doesn't help
ctx.seal_block(cond_block);
```

## Why It Works

When `cond_block` is sealed before `use_var`:

1. Cranelift's `find_var` function sees that `cond_block` is sealed with a single predecessor (`body_block`)
2. It follows the single-predecessor chain and finds the variable definition in `body_block`
3. Since the block is sealed and has only one predecessor, Cranelift can use the value directly without creating a block parameter
4. When branching back to `body_block`, the values are passed as arguments to the branch instruction

This is valid SSA form - the values flow through branch instruction arguments rather than through block parameters, which is more efficient for single-predecessor blocks.

## Related Code

- **Fixed implementation**: `lightplayer/crates/lp-glsl-compiler/src/codegen/stmt/loop_do_while.rs`
- **Debug test**: `lightplayer/crates/lp-glsl-compiler/tests/ssa_3_debug.rs` (test_do_while_loop_ssa_debug)
- **Working test without cond_block**: `lightplayer/crates/lp-glsl-compiler/tests/ssa_3_debug_no_cond.rs`
- **Cranelift SSA implementation**: `cranelift/frontend/src/ssa.rs` (find_var function)

## Test Files

- `lightplayer/crates/lp-glsl-filetests/filetests/control/loop_do_while/basic.glsl` - Basic do-while loop (passes)
- `lightplayer/crates/lp-glsl-filetests/filetests/control/loop_do_while/nested.glsl` - Nested do-while loops (may have separate issues)

## Key Takeaway

**For single-predecessor blocks that use variables from their predecessor: seal the block BEFORE using variables to allow Cranelift's SSA optimization to work correctly.**





