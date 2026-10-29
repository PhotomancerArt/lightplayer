# segments-64-5 — 512 lamps as 64 spokes

One fixture of 512 lamps: a repeat of 64 copies of an 8-lamp path (a spoke)
turned round the centre, on one output port that takes the rest. It was
map2d format 5 and unpatched (auto-flowed in lamp order) until project
format 12; the upgrade wrote it as format 6 with a start on output `"1"` and every
lamp where it was, so it now has its twin's shape (64 segments; only the
ids and the output's name differ).

**Its twin is `segments-64-6`**, written in format 6 by hand: before the
upgrade the pair measured what a segment costs at the 512-lamp design
target (mapping rebuild M2 P5, lean Q44), against the same lamps held as
one strand. Both stay as heap-ratchet records.

What uses it:

- `cargo test -p lpc-engine --lib format6_projects` — its twin lights the
  output exactly as this does.
- The engine heap ratchet (`just heap-budget-check`).

Its shader paints each lamp by where it is.
