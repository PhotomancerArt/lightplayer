# GLSL Frontend Memory Refactor Summary

## What Was Built

- Added a function-local HIR arena with `ExprId`, `ExprList`, and `PlaceId`.
- Changed recursive typechecking to return `ExprId` instead of owned `HirExpr` trees.
- Changed HIR statements and expression children to store IDs/ranges instead of `Box<HirExpr>` and `Vec<HirExpr>`.
- Moved assignment targets, writebacks, and indexed place expressions to arena IDs.
- Updated coercion, builtin typing, constant folding hooks, and lowering to consume arena-backed HIR directly.
- Removed unused uniform/global name strings from expression nodes.
- Added baseline/final memory notes and recorded the successful ESP32-C6 device trace.

## Decisions For Future Reference

#### Lower Directly From Arena

- **Decision:** Lowering reads `ExprId` and `PlaceId` through `HirArena`; there is no freeze back to recursive HIR.
- **Why:** Rebuilding the old tree would preserve or worsen peak heap pressure.
- **Rejected alternatives:** A temporary arena only for typechecking.
- **Revisit when:** Only if a later backend needs a different immutable IR view.

#### Keep Recursion, Shrink Frames

- **Decision:** Keep recursive typechecking for now, but make recursive values small IDs.
- **Why:** This attacks the stack problem with less semantic risk than an explicit worklist rewrite.
- **Rejected alternatives:** Full postorder worklist typechecking in the first pass.
- **Revisit when:** RV32 stack measurements still show unsafe frontend frames after this arena migration.

#### Defer Scratch Allocator

- **Decision:** Do not add a custom compiler scratch allocator in this pass.
- **Why:** Arena-backed HIR already consolidates many tiny allocations and should be measured first.
- **Rejected alternatives:** Per-compile bump allocation for the whole frontend.
- **Revisit when:** Device traces show remaining fragmentation/OOM behavior after this structural cleanup.
