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

Fourteen are compiled into the app and listed in the gallery's *Examples*
section — `fyeah-sign`, `logo-sign`, `plasma`, `meteor`, `comet`,
`palette-waves`, `fire2012`, `plasma-duo`, `zook-dome`, `small-dome`,
`peach-1d`, `peach-2d`, `pulse`, `fault-demo`.
Their file lists live in
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
| `logo-sign` | `speed`, `bands`, `tilt`, `palette` | the brand as a buildable piece: a shaped PCB matrix in the outline of the play triangle (132 lamps, map2d `filled_polygon` — the outline and the 11.5 pitch are authored, the count is *derived*) plus "LightPlayer" as 11 single-stroke letter strands (109 lamps) on ONE canvas, which is the landing hero's own stage. Generated, not drawn: `sign.map2d.json` comes from the brand triangle geometry and from `letters.svg` through the corpus SVG importer, and `logo_sign_gen.rs` in `lpa-studio-web` fails if the committed document falls behind the mark |
| `plasma` | `speed`, `scale`, `palette` | the smallest non-empty panel: one shader, three bound uniforms. Also the `what-is-a-shader` article's live figure. |
| `plasma-duo` | `speed`, `scale`, `palette` | one shader and one palette channel feeding two fixtures (disc + grid) with separate outputs |
| `zook-dome` | `speed` | a real 1500-LED dome across five output channels |
| `meteor` | `decay` | a compute/render pair — `sim` integrates meteor heads into a persistent map, `render` draws their tails over a `node:` binding |
| `comet` | `speed`, `tail`, `palette` | a true 1D shader: `vec4 render_1d(float)` against a 120-lamp strip, declaring `OneD { in_2d: Project { extrude-x } }` — the factored default projection. Ported from WLED |
| `palette-waves` | `speed`, `scale`, `depth`, `palette` | the declared-projection example: a 1D shader declaring `OneD { in_2d: Project { radial } }`, so the strip it is written along arrives on the disc fixture as rings. Ported from WLED |
| `fire2012` | `speed`, `reach`, `sparks`, `palette` | a fire climbing a 120-lamp strip, declaring `OneD { in_2d: Project { extrude-x } }`. Ported from WLED — but *stateless*: the per-cell heat simulation is not ported, the closed form writes down what it settles into |
| `peach-1d` | `speed`, `glow` (one set per submodule) | the patching example: two fixtures (body + leaves), each in its own submodule (`body/`, `leaf/`), sharing ONE 56-lamp wire, placed by hand-authored `.patch.json` files — the body claims two discontiguous ranges and its second range is `reversed`. Both fixtures run `render_1d` shaders along the strand (`strip_order_meaningful`), which is how this art runs on WLED today |
| `peach-2d` | `speed`, `glow` (one set per submodule) | the same artwork, the same wiring, the *byte-identical* patch files, declared 2D: `render_2d` planes sampled at the lamps' mapped positions. The pair is the whole mapping-and-patching argument — presentation and sampling are separate questions. See [the peach](../docs/user-guide/the-peach.md) |
| `small-dome` | `speed`, `bands`, `warmth` (per submodule) | Yona's real 16' 2V dome at FULL scale — the patching archetype and the desktop-class sim stress fixture (6,310 lamps). Ten panel-position objects, each a 5-way repeat of a closed 119-lamp polygon (50 suspended lucite panels — the 40 2V faces plus the riser rung's 10 downward triangles), and ONE always-lit 360-lamp chevron door, scattered across TWO named outputs ("1", "Box 2": the build's two control boxes, 13 ports each) with the door sharing a box-1 port tail — many-to-many. The `.patch.json` files are format-2 path-identity rows (`/band-a/3`) carrying the as-built install — one panel reversed, one rotated a side, the door turned a leg; ALL six wiring artifacts regenerate via `cargo run -p lpt-geodome`. See [patching the dome](../docs/user-guide/patching-the-dome.md) and [the three domes](../docs/use-cases/2026-08-28-three-domes.md) |
| `pulse` | `speed` | the plainest possible shader — one colour breathing on a phasor. The hardware-walk test subject: if a strip is dark under `pulse`, that is the wiring or a fault, never the content |
| `fault-demo` | `speed` | a shader that compiles but FAULTS at run time (fuel exhaustion) — the demo for "a fault is never black": the outputs show the red breathe and the device card reads Degraded |
| `basic`, `basic2` | — | the minimum viable project; `basic2` adds a texture |
| `button` | — | input nodes and playlist triggering |
| `button-playlist`, `button-sign`, `fyeah-button` | `palette` | input nodes and playlist triggering, on authored palettes |
| `events` | — | compute shaders publishing control messages |
| `fluid` | — | the fluid solver driven by compute-shader emitters |
| `fiber-headband`, `rocaille` | — | real fixtures with real 2D mappings |
| `fast`, `perf`, `shader-oracle` | — | benchmark and oracle rigs, not showcase content |

Sample content in this repository is CC0 unless a project's
`module.json` provenance says otherwise.

The two peaches are original content: their geometry is sampled at even
arc-length stations along the wire-true segment paths of Yona Appletree's
reference drawing, so both mapping documents describe the strand as it is
actually run — 22 lamps up one side of the body, 12 across the leaves, 22
back down the other side. No upstream project is involved.

## Ports from WLED

`comet`, `palette-waves` and `fire2012` are re-authored from **WLED's
MIT-era** source: commit `44e28f96e0af0c78cb1b902a45b6332dcacd10e0` (2024-10-15),
one commit before WLED relicensed to EUPL. Their `module.json`
provenance says `MIT`, each `.glsl` carries a provenance header naming
the upstream repo, file, function and SHA, and WLED's license text is
vendored at [`licenses/WLED-MIT.txt`](../licenses/WLED-MIT.txt) — the
per-file discipline
`docs/adr/2026-07-29-license-provenance-discipline.md` (and its
2026-08-01 addendum, which established that pre-relicense WLED is MIT)
requires of any permissively-licensed upstream.

These are ports of the *effect*, not transliterations: WLED's effects
are frame-rate-coupled integer routines over a pixel buffer, and all
three are re-expressed here as closed-form functions of a phasor, which
is what lets them be `render_1d` shaders at all. Their palettes are
original Photomancer ramps, not WLED's — an embedded palette is
redistribution, so none were copied.

`fire2012` goes furthest from its source, and says so in its header:
upstream keeps a byte of heat per cell and advances it every frame
(cool, drift up and diffuse, ignite sparks), which this engine cannot
express — a compute node producing a dense scalar array has no home in
the graph today. So no simulation was ported. The shader writes down
what that simulation converges to: an exponential heat gradient anchored
at the base, modulated by layered value noise scrolling upward, with the
crests of the finest layer standing in for the spark die-roll. The look
and the name are WLED lineage; the algorithm is original.
