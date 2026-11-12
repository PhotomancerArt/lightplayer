## Engine Diagnostics

- **Idea:** Add an engine diagnostics surface for resolution failures, ambiguous
  bindings, kind/type mismatches, and recursive demand cycles.
- **Why not now:** M2 needs to establish the owner, resolver cache, and binding
  discovery contract first; a diagnostics API deserves its own shape once the
  error cases are concrete.
- **Useful context:** Bus resolution is recursive normal resolution, so
  diagnostics should preserve the query/binding stack that led to the error.

## Filetest-Style Engine Setup DSL

- **Idea:** Add text-based engine graph fixtures where a small DSL declares
  nodes, bindings, demand roots, and expected outputs/errors.
- **Why not now:** M2 should first settle the concrete engine/resolver types and
  use a Rust test builder; the DSL grammar will be clearer after several tests
  reveal repeated setup patterns.
- **Useful context:** The DSL should feel like filetests: one block for setup,
  one block for expected resolved values or diagnostics.
