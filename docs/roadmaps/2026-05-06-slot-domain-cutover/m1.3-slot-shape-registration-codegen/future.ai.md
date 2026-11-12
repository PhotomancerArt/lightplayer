# M1.3 Future Work

## Committed Generated Files

- **Idea:** Generate `src/slot_shapes.generated.rs` and commit it with a
  freshness check.
- **Why not now:** User is comfortable trying `OUT_DIR` first, and it avoids
  stale-file workflow friction.
- **Useful context:** If `OUT_DIR` makes generated code too hard to inspect,
  revisit this before broader source/engine adoption.

## Typed Shape Reference Attributes

- **Idea:** Replace string shape refs like `value_ref = "source.param"` with
  typed refs that name the Rust root type.
- **Why not now:** Lazy codegen can recursively ensure `SlotShape::Ref` ids
  without changing macro syntax first.
- **Useful context:** Typed refs would let the derive produce more explicit
  dependency information and better compile-time errors.

## Cross-Crate Static Shape Resolver

- **Idea:** Add a small aggregator that tries generated `ensure_static_slot_shape`
  functions from multiple crates.
- **Why not now:** M1.3 only applies generated bootstrap to the mockup. M2 can
  add `lpc-source`; runtime engine adoption comes later.
- **Useful context:** This may become useful once source, engine, and plugin
  crates each own static shape roots.
