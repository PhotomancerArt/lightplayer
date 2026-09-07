# Catalog: the built-in projects and patterns

The checked-in content Studio compiles in and lists on the home page,
on Explore and in the device card's project picker. The user-facing word
is **Explore**; the word here is *catalog* because "library" is taken —
Studio's *library* is the user's own store of projects, and this tree is
the built-in stock it remixes from. The public library is a later step.

Design record: `docs/adr/2026-09-06-catalog-content-tree.md`.

## Layout: buckets mirror `kind`, the manifest is the truth

```text
catalog/
  projects/<slug>/    real pieces          — project.json has no `kind` (general)
  patterns/<slug>/    single effects        — project.json says `kind: pattern`, `exports: ["effect"]`
  templates/          reserved for `kind: template` (follow-up plan; absent today)
  COPYING.md          license rows for every entry that is not CC0
```

The bucket is for authors — a place to look. What the code reads is
`project.json`, and `cargo test -p lp-cli --test catalog_tree` fails when
a folder's bucket and its manifest disagree, so the two cannot drift.
Studio's ids are bucket-free (`catalog/<slug>`), so reclassifying an
entry moves a folder and changes nothing persisted; the pre-catalog
spelling `examples/<slug>` still resolves for libraries seeded before
the move.

Engine and bench test rigs are **not** content: they live under
[`../projects/test/`](../projects/test/) and are never embedded.

Every entry is one project in the two-file layout
(`docs/design/modules.md` §6): a `project.json` container manifest beside
a `module.json` root module; every other file is a node artifact or an
asset the module references. A pattern additionally keeps its effect
inside `effect/` — its own `module.json`, the shader def and source —
which is the folder other projects import by copy.

## Registration is automatic

`lp-app/lpa-studio-core/build.rs` walks `catalog/<bucket>/<slug>/` at
build time and embeds every file; the registry types each entry from its
own `project.json` (`name`, `kind`, `description`). Adding an entry is
adding a folder — no Rust edit, pinned by
`every_catalog_directory_is_registered_and_vice_versa`. A change here
reaches Studio after a rebuild, and an already-seeded library keeps the
copy it made (delete the gallery package to re-seed).

## The manifest fields an entry sets

| Field | Where | Rule |
|---|---|---|
| `name` | `project.json` | The card title and the picker row. |
| `description` | `project.json` | Optional, one sentence, ≤160 characters. Project data (the card face does not show it today). |
| `kind`, `exports` | `project.json` | Patterns: `"kind": "pattern"`, `"exports": ["effect"]`. Projects: absent. |
| `provenance` | `effect/module.json` for a pattern; the root `module.json` for a project | `author`, `version`, `license`, `created`. CC0-1.0 unless stated; a pattern's export must state a license or the export lint warns. |

Sample content in this repository is CC0 unless a module's provenance
says otherwise; anything that is not CC0 has a row in
[`COPYING.md`](COPYING.md).

## Add an entry

1. In Studio, **New → Pattern 1D / Pattern 2D** for an effect (or copy a
   neighbouring folder for a piece), author it, and save it into
   `catalog/<bucket>/<slug>/` — Download zip from the project card, or
   copy the library folder. The slug is `[a-z0-9-]`; it becomes the
   `/p/<slug>` address.
2. Set `name` (and `description` if you want one) in `project.json`.
   A pattern's `project.json` carries `kind`/`exports`; its provenance
   goes on `effect/module.json`.
3. If the module is not CC0, add a `COPYING.md` row: slug, name, SPDX
   tag, author, source URL, upstream file/function.
4. Run the gates (below). A pattern must also open onto a populated
   root panel: at least one control bound to a bus channel (`speed` is
   the usual one), per `docs/adr/2026-08-03-panel-visibility-is-derived.md`.
5. Optional: a per-entry `README.md` — source commit for ports, what
   changed, what it stresses, and a bench-test line (board, date, who
   walked it). It ships with the entry and travels with a remix.

No Rust changes anywhere in that list.

## The gates

| Test | Holds |
|---|---|
| `checked_in_catalog_entries_load_as_core_projects` (`lp-cli`, `examples_valid`) | every entry (and every rig under `projects/test/`) loads through the real `ProjectLoader` |
| `checked_in_catalog_entries_rewrite_byte_identically` | load → write is byte-identical for `project.json` and `module.json` |
| `authored_artifacts_conform_to_checked_in_schemas` (`schema_conformance`) | every JSON validates against `schemas/` |
| `every_bucket_agrees_with_its_manifest_kind` (`catalog_tree`) | bucket ⇔ `kind`; a pattern's every export has a `module.json` |
| `copying_manifest_matches_the_tree_both_ways` | non-CC0 entries have a `COPYING.md` row; every row names an entry |
| `every_description_is_one_line` | a description, when present, is ≤160 characters |
| `every_catalog_directory_is_registered_and_vice_versa` (`lpa-studio-core`) | the registry equals the tree (≥16) |
| `every_gallery_example_opens_onto_a_populated_root_panel` | each entry boots on a real server and publishes a control |
| `catalog_pattern_oracles` (`lpa-studio-core/tests`) | every pattern passes the four template oracles: schemas, loader, library round trip keeping `kind`/`exports`, export lint clean from the installed copy |

Run them with `cargo test -p lp-cli` and `cargo test -p lpa-studio-core`,
or the whole gate with `just check test`.

## What is here

**Projects** (`projects/`): `fyeah-sign` (the Studio demo — the full bus,
a playlist and an authored palette), `logo-sign` (the brand as a
buildable piece; generated by `logo_sign_gen.rs` in `lpa-studio-web`,
whose drift test fails if the committed mapping falls behind),
`zook-dome` (a real 16' dome, 1,500 LEDs on five channels), `small-dome`
(Yona's 16' 2V dome at full scale, 6,310 lamps — the patching archetype
and the desktop-class stress fixture; regenerate with `cargo run -p
lpt-geodome`), `peach-1d` / `peach-2d` (the mapping-and-patching pair:
byte-identical patch files, opposite declarations — see
[the peach](../docs/user-guide/the-peach.md)), `fiber-headband` (a real,
battery-powered wearable) and `rocaille` (a real 2D piece on a
hand-authored mapping).

**Patterns** (`patterns/`): `plasma` (the smallest non-empty panel; also
the docs' live figure), `plasma-duo` (one shader, two fixtures — its
shader and clock stay byte-identical with `plasma`, test-pinned),
`meteor` (a compute/render pair over a `node:` binding; the board-project
generator vendors its export), `pulse` (the hardware-walk subject: if a
strip is dark under it, that is the wiring), and the three WLED ports
below.

Not here on purpose: `projects/test/fault-demo`, the shader that faults
every frame to demonstrate "a fault is never black". Its loop never ends
and only the LPVM's fuel meter traps it; a GPU tier used to hang on it
(`docs/defects/2026-09-06-gpu-tier-executes-unbounded-shaders.md`, fixed
by the loop-bounds pass, which now refuses it at GPU compile time), so a
gallery card would show a placeholder instead of the pattern. It stays a
rig for `lp-cli dev` and the engine/server tests.

## Ports from WLED

`comet`, `palette-waves` and `fire2012` are re-authored from **WLED's
MIT-era** source: commit `44e28f96e0af0c78cb1b902a45b6332dcacd10e0` (2024-10-15),
one commit before WLED relicensed to EUPL. Their exported
`effect/module.json` provenance says `MIT`, each `.glsl` carries a
provenance header naming the upstream repo, file, function and SHA,
[`COPYING.md`](COPYING.md) carries their rows, and WLED's license text
is vendored at [`licenses/WLED-MIT.txt`](../licenses/WLED-MIT.txt) — the
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

The two peaches are original content: their geometry is sampled at even
arc-length stations along the wire-true segment paths of Yona Appletree's
reference drawing, so both mapping documents describe the strand as it is
actually run — 22 lamps up one side of the body, 12 across the leaves, 22
back down the other side. No upstream project is involved.

## Test rigs

Engine, bench and hardware bring-up projects live under
[`../projects/test/`](../projects/test/): not content, not embedded, but
walked by the same load and schema gates.
