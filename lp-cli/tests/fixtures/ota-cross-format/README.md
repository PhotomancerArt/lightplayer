# The crossing walk's projects, at project format 11

What a board on the last format-11 release holds when an update brings it to
format 12: the "before" of the over-the-air project upgrade
(`docs/adr/2026-10-28-firmware-updates-upgrade-the-boards-projects.md`).
**Never migrated, never re-saved**: they are the old bytes the update's
project stage must find on the board and upgrade.

- `v11/eaves` — `lpa-upgrade`'s v11 corpus `eaves-wired` (two runs of a
  strip, a manual patch, the front run reversed, D10 with 60 lamps and D9
  with 50: the C6 scale), with one change: its shader is
  `projects/test/shader-oracle`'s, a static pattern coloured by position
  with no clock, and its `shader.json` consumes nothing. So the WS281x
  frames decoded off D10 (GPIO18) and D9 (GPIO20) before the update and
  after it compare byte for byte: the same lamps, the same colours. It is
  the startup project (first by path).
- `v11/headband` — the v11 corpus `fiber-headband` unchanged (a second
  project, so the stage writes more than one; its upgrade adds a map file).

Seeded onto a board running the published release with plain `filesystem`
`write` requests over that release's own wire (the file requests did not
change across the bump), then loaded: `lp-cli/tests/emu_ota.rs`
(`with_v11_projects`), `scripts/ota/emu-cut-sweep.sh` (`OPS=lpfs`) and
`scripts/emu/walk-ota-emu.mjs` (the `cross-format` steps).
