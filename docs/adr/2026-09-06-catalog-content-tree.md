# ADR: The catalog content tree — buckets mirror kind, the manifest is truth, registration is generated

- **Status:** Accepted
- **Date:** 2026-09-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Studio's built-in content lived in `examples/`: 29 directories, of which
11 were engine and bench test rigs, 8 were single-effect showcases and
8 were real pieces, all in one flat list. The manifest already carried
`kind`, `target` and provenance (`2026-08-07-project-kinds-and-pattern-exports`,
`2026-08-05-project-target-metadata`), and no entry set any of them. What
Studio showed was decided by a hand-maintained Rust table of 14 entries
with 148 `include_bytes!` lines, a literal `"Module"` kind and no
per-entry words; the Explore page's "Projects" filter chip was a
decorative span. Yona's summary opening the vision session: the examples
are "a jumbled mess of random things we've made", and "the main driver of
this work is my annoyance at the homepage feeling cluttered and jumbled".

Two mechanical causes: test fixtures shared the tree with content, and a
Rust table — not the content — decided what counted as content. The
metadata that would organize the tree existed and nothing read it.

The module authoring unit (T1, PR #380) had shipped the pattern shape —
`kind: pattern`, an exported `effect/` folder, the export lint, the
1D/2D templates, import by copy — and recorded that the flat examples
"restructure opportunistically as they enter a pack". None had.

## Decision

1. **`catalog/` with buckets that mirror `kind`; the manifest is the
   truth.** `catalog/patterns/<slug>/` holds `kind: pattern` projects,
   `catalog/projects/<slug>/` holds ordinary pieces, `catalog/templates/`
   is reserved for a future `template` kind. The bucket is a filing
   convenience for authors; `project.json` is what the code reads, and
   `lp-cli/tests/catalog_tree.rs` fails when the two disagree. Engine
   and bench rigs are not content and live under `projects/test/`.
2. **Ids are bucket-free — `catalog/<slug>` — and the legacy spelling
   resolves.** Example ids are persisted (`SeededFrom { source }` in
   every remixed library package, the cloud store, `/p/<slug>` tails), so
   an id that carried the bucket would churn every time an entry was
   reclassified. `embedded_example()` rewrites the pre-catalog
   `examples/<slug>` prefix before lookup, so "Remixed from" lines in
   libraries seeded before the move keep resolving. Nothing writes the
   old spelling anew.
3. **Registration is generated from the tree; typing happens at runtime
   through the one manifest parser.** `lpa-studio-core/build.rs` walks
   `catalog/<bucket>/<slug>/**` and emits the `include_bytes!` file
   tables (bucket display order, slugs sorted, `project.json` then
   `module.json` first); the registry parses each `project.json` once at
   first use with `ProjectManifest::read_json` for `name`, `kind` and
   `description`. The build script needs no JSON dependency and can never
   drift from the parser the tree gates use. Adding content is adding a
   folder: pinned by a test that every catalog directory is registered
   and vice versa.
4. **`description` is a container-manifest field.** Optional, written
   after `name`, no format bump (the same additive posture as `kind` and
   `target`). It is project data: the G1 review kept the copy off the
   card face ("the names and pictures speak for themselves"), so today
   nothing renders it; the gate only bounds its length.
5. **Provenance travels with the shader; `catalog/COPYING.md` is
   enforced both ways.** A pattern's provenance sits on its exported
   `effect/module.json` (the export lint keys on it; the WLED ports keep
   their per-file headers untouched). Every entry whose carrying module
   states a license other than the CC0 default has a row in
   `catalog/COPYING.md` naming its slug, SPDX tag and source URL, and
   every row names an entry — the palette catalog's contract
   (`2026-08-04-palettes-are-values`), applied to projects.
6. **The three listing surfaces group by kind, real pieces first.** Home
   landing, Explore and the device card's project picker render one core
   derivation (`example_groups`): "Projects", then "Patterns", templates
   excluded from every grid once the kind exists. Every entry stays on
   the home page (G1 ruling: "scrolling is fine"). A `featured` manifest
   flag is the declared hook for a curated home once Explore returns as
   the full list; it is not built.
7. **Built-in patterns are importable.** The add-node picker's import
   source lists the catalog's patterns beside the library's; a built-in
   vendors from the compiled-in bytes under `modules/<slug>/` (the slug,
   because every catalog pattern exports `effect`).
8. **Templates and the positional board rewire are sequenced to a
   follow-up plan**, not reversed. The board audit that caused it: only
   2 of 8 catalog boards list more than one wire, labels are not portable
   across boards (`D10` is GPIO18 on one XIAO and GPIO9 on another),
   networked endpoints do not exist in the model, and the Espressif
   devkit label/GPIO mismatch is unfixed
   (`docs/debt/espressif-devkit-led-wire-label-mismatch.md`).

## Consequences

- `fault-demo` is not catalog content: its deliberately unbounded loop
  is only safe under the LPVM's fuel meter, and a gallery card previews on
  a GPU tier that has none — the G1 walk hung Brave, crashed Firefox's
  host, and a native wgpu run of it glitched the desktop
  (`docs/defects/2026-09-06-gpu-tier-executes-unbounded-shaders.md`). It
  lives under `projects/test/` as the engine/server tests' rig; the
  catalog holds 15 entries (8 pieces, 7 patterns).
- `examples/` no longer exists; ~370 path and id sites moved with it.
  Dated ADR, defect and report narratives keep their `examples/`
  spelling as records of commands run at the time.
- The eight restructured patterns (seven in the catalog, fault-demo as a
  rig) are real `kind: pattern` projects; the catalog seven pass the four
  oracles the templates are held to (schemas, loader, library round
  trip, export lint from the installed copy). The board-project generator
  vendors meteor's export folder rather than an inline module.
- Measured on the emulator's C6 layout and re-baselined in
  `scripts/heap-budget-record.json`: meteor's effect submodule costs
  +356 B retained per frame and drops the largest free block at compile
  close by 15.6 KB (147,408 → 131,832 B); a parsed `description` costs
  about 100 B per loaded project.
- `2026-08-07-project-kinds-and-pattern-exports` §"Existing flat
  examples" is amended: the restructuring happened here, as catalog
  content work, not as a migration.
- The load gate walks `catalog/` and `projects/test/`; the byte-identity
  gate joins the rigs once main's canonicalization of the pre-catalog
  rigs (PR #543) is merged into this tree.

## Alternatives Considered

- **Dimensionality or origin as path levels** (`catalog/1d/wled/…`) —
  rejected: the tree would encode browse axes the metadata already
  carries, and a WLED 1D pattern that gains a 2D declaration would move.
- **Keeping the Rust table** with better ordering — rejected: it is the
  cause of the jumble, and it makes every content addition a code edit.
- **Bucket in the id** (`catalog/patterns/comet`) — rejected: persisted
  provenance would break on reclassification (plasma may yet become a
  template).
- **A `demos/` bucket for the home page** — deferred as the `featured`
  hook (D17): a bucket would fork the id and the kind for a presentation
  fact.
- **Parsing `project.json` in the build script** (a `serde_json`
  build-dependency) — rejected: two field extractions drift; the runtime
  parser is the one the tree gates run.

## Follow-ups

- Templates + positional board rewire: the follow-up plan seeded from
  this plan's board audit (`kind: template`, provisioning preferring a
  template, the rewire op named to avoid `retarget`).
- `featured` on the manifest and a curated home, when Explore returns as
  the full list (D17 end state).
- ~~Widen the byte-identity gate to `projects/test/` on the merge of
  PR #543.~~ Done on this branch once #543 merged in.
- ~~A GPU-tier guard for unbounded shaders~~ Landed as PR #556
  (`2026-09-06-gpu-tier-loop-bounds`): static refusal plus an injected
  back-edge budget; the defect above is closed.
- The pattern submodule's 15.6 KB largest-free-block cost on the classic
  is worth a look when the classic's load gate is next re-measured.
