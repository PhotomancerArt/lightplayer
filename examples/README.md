# Example projects

Each directory is one project in the ratified two-file layout
(`docs/design/modules.md` §6): a `project.json` container manifest
carrying workspace identity (`format`, `name`, provenance), beside a
`module.json` root module node carrying the technical spec. Every other
file is a node artifact or an asset the module references.

These are checked-in fixtures as much as they are content:
`cargo test -p lp-cli` walks every directory here and fails if one does
not load (`checked_in_examples_load_as_core_projects`) or does not
survive a load → write round trip byte-for-byte
(`checked_in_examples_rewrite_byte_identically`).

## In the Studio gallery

Three are compiled into the app and listed in the gallery's *Examples*
section — `fyeah-sign`, `plasma`, `meteor`. Their file lists live in
`lp-app/lpa-studio-core/src/app/home/embedded_example.rs`
(`include_bytes!` against this directory), so a change here reaches
Studio only after a rebuild, and an already-seeded library keeps the copy
it made (delete the gallery package to re-seed).

A gallery example must open onto a **populated panel**: at least one
root-scope control, published the only way publicity happens — an
authored binding to a bus channel
(`docs/adr/2026-08-03-panel-visibility-is-derived.md`). Pinned by
`every_gallery_example_opens_onto_a_populated_root_panel`.

| Example | Publishes | Shows off |
|---|---|---|
| `fyeah-sign` | `glow`, `palette` (via the active playlist entry) | the full bus: clock, button + radio onto `bus:trigger`, playlist switching idle/blast, and an authored palette cycling three moods. The Studio demo project. |
| `plasma` | `speed`, `scale` | the smallest non-empty panel: one shader, two bound uniforms |
| `meteor` | `decay` | a compute/render pair — `sim` integrates meteor heads into a persistent map, `render` draws their tails over a `node:` binding |
| `basic`, `basic2` | — | the minimum viable project; `basic2` adds a texture |
| `button` | — | input nodes and playlist triggering |
| `button-playlist`, `button-sign`, `fyeah-button` | `palette` | input nodes and playlist triggering, on authored palettes |
| `events` | — | compute shaders publishing control messages |
| `fluid` | — | the fluid solver driven by compute-shader emitters |
| `fiber-headband`, `rocaille` | — | real fixtures with real 2D mappings |
| `fast`, `perf`, `shader-oracle` | — | benchmark and oracle rigs, not showcase content |

Sample content in this repository is CC0 unless a project's
`module.json` provenance says otherwise.
