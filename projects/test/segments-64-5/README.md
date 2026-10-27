# segments-64-5 — 512 lamps as 64 spokes, map2d format 5

One fixture of 512 lamps: a repeat of 64 copies of an 8-lamp path (a spoke)
turned round the centre, on one output port that takes the rest. Unpatched,
so the output auto-flows it in lamp order.

**Its twin is `segments-64-6`**, the same lamps in format 6, where each copy
is its own segment (64 segments): the pair measures what a segment costs the
format-6 path at the 512-lamp design target (mapping rebuild M2 P5, lean
Q44), against today's path holding the same lamps as one strand of 64
instances.

What uses it:

- `cargo test -p lpc-engine --lib format6_projects` — its twin lights the
  output exactly as this does.
- The engine heap ratchet (`just heap-budget-check`).

Its shader paints each lamp by where it is.
