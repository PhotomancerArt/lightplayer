# segments-64-6 — 512 lamps as 64 segments, map2d format 6

`segments-64-5` in map2d format 6: the spoke is a `path` part held by a
`repeat` of 64 copies (`path1`, `path1@1` … `path1@63`, one segment each),
and the fixture starts at lamp 0 of port 0 of output `main`. Every lamp
lands where its twin's does.

**Its twin is `segments-64-5`**: the pair is the per-segment memory twin of
mapping rebuild M2 P5 (lean Q44's 512-lamp, 64-segment case).

What uses it:

- `cargo test -p lpc-engine --lib format6_projects` — lights its output
  exactly as `segments-64-5` does.
- The engine heap ratchet (`just heap-budget-check`).

Studio's Map, Patch and Graph tabs don't read format 6 yet (M4).
