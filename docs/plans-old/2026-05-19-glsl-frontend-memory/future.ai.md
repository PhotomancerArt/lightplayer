# Future Work

## Explicit-Stack Typechecker

- **Idea:** Convert expression typechecking from recursive calls to an explicit postorder worklist.
- **Why not now:** Arena-backed IDs should first make recursion cheap enough to measure honestly. A worklist rewrite is more invasive around coercions, constant folding, lvalue typing, and out/inout arguments.
- **Useful context:** Revisit after phases 3 and 4 if RV32 stack frames remain unsafe.

## Compile Scratch Allocator

- **Idea:** Allocate parse/typecheck/lower temporary data from a per-compile scratch arena that is freed wholesale after codegen.
- **Why not now:** The HIR arena already consolidates many tiny allocations and should be measured first.
- **Useful context:** This is most useful if device traces still show fragmentation or suspicious OOM behavior after arena-backed HIR.

## Identifier Interning

- **Idea:** Intern identifiers and field names to `SymbolId` to reduce repeated `String` allocations across parse, HIR, imports, uniforms, globals, and places.
- **Why not now:** It is a second-order improvement compared with large recursive expression/place values.
- **Useful context:** Texture operand paths currently depend on field names during type/lower, so path construction needs a clear replacement before removing strings from places.

## Compact Lower Values

- **Idea:** Replace many small lane `Vec<VReg>` allocations in lowering with a compact small-vector or fixed lane buffer.
- **Why not now:** This plan focuses on frontend typechecking and HIR memory first.
- **Useful context:** `LowerValue` and `LoweredPlace` commonly hold scalar/vector lane lists with at most 16 lanes for matrices.
