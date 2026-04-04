# Milestone II: Pointer-based stores and loads

## Goal

All filetest files that fail with `store to non-local pointer` or `Load from non-local pointer`
pass on `jit.q32`.

## Suggested plan name

`lpir-parity-milestone-ii`

## Scope

**In scope:**

- **Matrix element stores** (`m[col][row] = x`): Naga represents these as
  `Store(AccessIndex(AccessIndex(local_ptr)))`. The lowering in `lower_stmt.rs` currently rejects
  stores through non-local pointers. Need to recognize the double-`AccessIndex` pattern on matrix
  locals and map it to the correct scalarized VReg assignment.
- **Matrix column inc/dec** (`m[col]++`): similar pattern, column-level access + modify + store.
- **Bvec element assign** (`b[i] = true`): `Store` through `AccessIndex` on a bvec local.
  Structurally identical to vector component stores that already work for float/int vectors —
  may just need the bool/bvec case wired in.
- **Bvec dynamic index load** (`b[i]` where `i` is a variable): `Load` from a computed pointer
  into a bvec. Needs either stack-slot + computed offset, or a `select` chain for small vectors.

**Out of scope:**

- Array element addressing (Milestone IV).
- Struct member access (deferred).
- Matrix returns / invoke ABI (Milestone V).

## Key decisions

- **Dynamic bvec index:** For `bvec2`–`bvec4` (2–4 components), a `select` chain is simpler than
  a stack slot. For larger hypothetical vectors, stack slot + offset. Decision: use `select` chain
  for vec2–vec4; this matches what WASM does.
- **Matrix element store pattern:** Naga's double `AccessIndex(col)(row)` should map to
  `vreg[col * rows + row]` in the scalarized layout (column-major).

## Deliverables

- Updated `lower_stmt.rs` (matrix element stores, bvec element stores).
- Updated `lower_expr.rs` (bvec dynamic index load → select chain).
- ~15 filetest files passing (8 matrix incdec + 2 bvec assign + 5 bvec dynamic index).

## Dependencies

Milestone I (Relational) should land first — some of these files also use `all()` / `any()` and
would fail on both fronts. Fixing stores without Relational would still leave the file failing.

## Estimated scope

Medium. Matrix element store pattern recognition is the main complexity; bvec stores/loads are
smaller variations of existing vector component logic.
