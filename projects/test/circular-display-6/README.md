# circular-display-6 — Yona's circular display, as soldered

The circular display journey (mapping rebuild M1) as a project: one `circle`
part with rings of 24, 16, 12 and 8 lamps and a one-lamp centre (`r1` …
`r5`), drawn outside in. The fixture starts at lamp 0 of port 0 of output
`main`; ring `r3` is overridden onto port 1 at lamp 0 and reversed, and `r4`
and the centre follow it there. The output has two ports of 64 lamps.

Its map2d is the journey's own document
(`lp-core/lpc-mapping/tests/format6/circular_display.map2d.json`). Its shader
paints each lamp by where it is (`uv.x`, `uv.y`, ½), so distinct lamps are
distinct colours and a placement test can say which lamp is where.

**It has no format ≤ 5 twin**: no format-5 document can say "this ring on
another port, reversed". Where it lands is checked against the patcher
itself.

What uses it:

- `cargo test -p lpc-engine --lib format6_projects` — every lamp where
  `Patching::landing`, computed from this file, says.
- `cargo test -p fw-tests --test scene_render_format6_emu` — the same on
  fw-emu.
- The engine heap ratchet (`just heap-budget-check`).

Studio's Map, Patch and Graph tabs don't read format 6 yet (M4).
