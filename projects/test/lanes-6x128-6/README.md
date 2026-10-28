# lanes-6x128-6 — six lanes, mapped in map2d format 6

`projects/test/lanes-6x128` (six 128-lamp fixtures, each on its own bus
channel, each driving its own output) with every fixture's map2d written in
format 6: each lane is one `grid` part, and each fixture starts at lamp 0 of
port 0 of its own named output (`lane1` … `lane6`). Every lamp lands where
its twin's does.

**Its twin is `lanes-6x128`**, a project the engine heap ratchet already
records: the pair is how the format-6 path's memory is compared with
today's at device width (mapping rebuild M2 P5).

What uses it:

- `cargo test -p lpc-engine --lib format6_projects` — lights all six outputs
  exactly as `lanes-6x128` does.
- `cargo test -p fw-tests --test scene_render_format6_emu` — the same on
  fw-emu, every output byte for byte.
- The engine heap ratchet (`just heap-budget-check`).

Studio's Graph edits format 6 from mapping rebuild M4 (the Map tab is
hidden and the Patch tab deleted since project format 12).
