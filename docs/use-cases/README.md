# Use cases

Two kinds of documents live here.

**Install demands** (the original kind, e.g.
[FYeah sign](2025-05-08-fyeah-sign.md)): a specific real
installation and the features it needs from the engine. Customer
and delivery details stay out of the repo (they live in the
planning workspace).

**Archetypes**: recurring *structural shapes* with the identifying
details boiled off — the thing several installs have in common,
written down so plans can cite it instead of re-deriving it in
every design conversation. An archetype doc contains:

- the shape, in a paragraph;
- the real installs it abstracts (by description, not by client);
- the **demands** it makes of the system, as checkable claims —
  these become acceptance criteria in plans.

Rules of the practice (ruled 2026-08-09, mapping & patching vision):

- **Register on the second design conversation a shape steers.**
  Once is an anecdote; twice is an archetype.
- **The doc leads; the example lands with the enabling feature.**
  Each archetype names a miniature example project
  (structure-faithful, scale-reduced) that ships as the
  definition-of-done of whatever plan enables it — not before.
- Archetype docs are demand documents, not tutorials. Tutorials go
  in `docs/user-guide/`.

## Archetypes

| Doc | Shape | Example project |
|---|---|---|
| [The peach](2026-08-09-peach.md) | Sections of one strip wanting different looks (stained-glass) | `examples/peach-1d`, `examples/peach-2d` — shipped (PR #405; ADR `2026-08-10-output-fragments-and-patch-files`) |
| [The small-dome](2026-08-09-mini-dome.md) | N-way symmetric repeat, re-patched every install | `examples/small-dome` — shipped full-scale 2026-08-28 (50×119 panels + door, `lpt-geodome`) |
| [The two-rig scene](2026-08-09-two-rig-scene.md) | Shared visuals, disjoint control | future |

## Install demands

| Doc | Install |
|---|---|
| [FYeah sign](2025-05-08-fyeah-sign.md) | Bar sign + big red button, SOAK 2026 |
