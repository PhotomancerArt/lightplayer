# Future Work

## Schema Versioning

- **Idea:** Add explicit schema versioning and compatibility policy for slot
  authored data and wire payloads.
- **Why not now:** Unknown fields should remain hard errors while the slot codec
  model is still being shaped.
- **Useful context:** Definition and message migrations will reveal which
  compatibility boundaries matter.

## Compact Semantic Syntax

- **Idea:** Add opt-in compact syntax for semantic leaves such as refs,
  products, and single-value enum cases.
- **Why not now:** First migration should prefer correctness and one generic
  path over format polish.
- **Useful context:** `BindingEndpoint`, `ResourceRef`, `ProductRef`, and
  `Affine2d` are likely pressure points.
