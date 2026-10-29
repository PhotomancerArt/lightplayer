# shader-oracle-6 — the shader oracle, mapped in map2d format 6

`projects/test/shader-oracle` with its fixture's map2d written in format 6
(one fixture's parts and patching in one file): the same 8 × 8 grid as one
`grid` part, and a start that puts it at lamp 0 of port 0 of the output
`main`. The output is named `main` because a format-6 start names the output
it lands on; nothing else differs, so every lamp lands where the oracle's
does and the frame is the oracle's, byte for byte.

**Its twin is `shader-oracle`.** Everything that project's README says binds
here: no clock, the neutral output pipeline, exactly 64 LEDs.

What uses it:

- `cargo test -p lpc-engine --lib format6_projects` — lights its output
  exactly as `shader-oracle` does, sample for sample.
- `cargo test -p lpa-server --test shader_oracle_6_frame` — its host frame
  equals `shader-oracle`'s, byte for byte.
- `PROJECT=projects/test/shader-oracle-6 just walk-esp32c6-emu` — the
  emulated C6 renders it against the unchanged `shader_oracle_frame` oracle.
- The engine heap ratchet (`just heap-budget-check`), beside its twin.

Studio's Graph edits format 6 from mapping rebuild M4 (the Map tab is
hidden and the Patch tab deleted since project format 12).
