# ADR: The GLSL frontend's HIR keeps one type table per module

- **Status:** Accepted
- **Date:** 2026-09-02
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The lps-glsl frontend compiles on the device, out of what the ESP32-C6 has
left after a project loads (~220 KB after the meteor project on a 325 KB
heap; the classic ~126 KB). PR #497 cut the compile transient in half by
making every HIR node refer to its type by id instead of owning an
`LpsType`: each function's `HirArena` grew a small type table
(`intern`, `TypeId`), scalars and structs deduplicated by equality.

That table was per function. A module whose functions all touch the same
struct types holds one copy of those types per function, and the arenas
live until lowering finishes. The filetest `struct/deep-nested.glsl` — a
5.3 KB shader with four nested struct levels and 17 functions — still
peaked at 183,997 B host after #497, with 141 KB resident after the HIR
build: seventeen tables each holding `PanelGrid`/`Box`/`Line`/`Point`. On
the shipped examples the duplication is small (their structs are few and
flat), but the *language* lets a struct-heavy shader cost more than the
device has free, and the frontend has to be robust to what the language
allows, not just to the examples.

Two things are fixed by earlier decisions: `LpsType` in `lps-shared` is the
owning type vocabulary every consumer (Studio, the wire, serde) shares and
must not change shape; and `LpsModuleSig` keeps owning its types so
consumers never depend on frontend internals.

## Decision

1. **The type table is module state.** `HirModule.types: TypeTable` holds
   every distinct type of the compile once, deduplicated by `==`. The build
   state owns it while functions are typed; the module owns it afterwards;
   lowering reads it through `&HirModule`.
2. **`HirArena` stores ids only.** Expressions, places, locals and function
   signatures carry a `TypeId`; a `TypeId` is valid across every arena of
   the module — a global const's initialiser, each function body, the
   synthetic shader-init function and the signature list all share the
   space. Copying a const's expression tree into a function copies ids;
   nothing re-interns.
3. **Readers pair an arena with the table explicitly.** `TypedArena<'t>`
   (typing: owns the arena, borrows the table mutably) and `HirView<'a>`
   (lowering and const folding: borrows both) expose the same
   `expr`/`expr_ty`/`place` surface the arena used to, so a reader never
   resolves a `TypeId` against the wrong table and never needs interior
   mutability. The table is threaded as `&mut TypeTable` into the typing
   context; no `Rc`, no `RefCell`.
4. **Deduplication stays a linear scan.** A module has tens of distinct
   types; comparing a struct is memberwise. If a census ever shows the scan
   on the profile, a hash by shape is a change inside `TypeTable` alone.

## Consequences

Measured 2026-09-02 (host bytes above baseline; device bytes are ~½–⅔):

| shader | before | after | note |
|---|---|---|---|
| `struct/deep-nested.glsl` peak | 183,997 | 105,845 | −42 %; resident HIR 141,027 → 62,875 |
| corpus largest file | 183,997 (deep-nested) | 137,575 (`operators/incdec-matrix-element.glsl`) | ceiling 252 → 189 KB |
| `examples/basic` shader peak | 80,331 | 80,043 | ~0, as expected |
| `examples/meteor` sim peak | 57,470 | 57,398 | ~0, as expected |

The shipped examples do not move: that is the point. This decision buys
language robustness — a struct-heavy shader no longer costs one copy of
its struct types per function — not a flagship number.

Costs: ~100 call sites changed mechanically (readers take a view, writers
take the table); `HirModule` gains a field; a global const's arena is no
longer self-describing (its ids need the module table, which is always at
hand where consts are read). The linear scan is unchanged in cost class:
the table is slightly longer than any one function's was, and every
interned type still clones once to compare.

Not changed: `LpsType`, `LpsModuleSig`, the LPIR output (37,959 filetests,
goldens byte-identical), the backend.

## Alternatives Considered

- **A shared handle inside the arena (`Rc<RefCell<TypeTable>>`).** Keeps
  every call site, but `arena.ty(id)` can no longer hand out `&LpsType`
  (a `RefCell` guard cannot back `HirExprRef<'a>`), and it hides a
  module-wide dependency inside a per-function value. Rejected.
- **`HirArena<'m>` borrowing `&'m mut TypeTable`.** The arena would carry a
  lifetime to a table the module itself must own; `HirFunctionBody` and
  `HirModule` would inherit it. Rejected.
- **Split id spaces** (scalars per function, aggregates per module, tagged
  `TypeId`). Two homes for one concept, and every reader branches.
  Rejected.
- **Interning inside `LpsType`** (`Rc`/global table). Changes what
  `LpsType` is for every `lps-shared` consumer and its serde; rejected in
  #497's evaluation for the same reason.

## Follow-ups

- Resolve a struct *name* to its `TypeId` at the header step so typing a
  reference to `Point` never clones the struct to find the match (the
  deep-nested census still counts thousands of transient member-vector
  clones — compile time, not peak).
- `HirLocal.name` could borrow the source once `HirModule` carries `'src`;
  it does not today.
