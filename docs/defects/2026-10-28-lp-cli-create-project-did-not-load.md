---
status: fixed
found: 2026-10-28      # how: a new test (mapping rebuild M3 P5, every creatable project lands)
fixed: pending         # lands with the mapping rebuild's integration branch (PR #1319)
area: lpc-model — the starter project (`starter_project_files`, used by `lp-cli create project`)
class: backend-contract-divergence
related:
  - lp-core/lpc-model/src/nodes/starter_project.rs
  - lp-core/lpc-engine/tests/creatable_projects_land.rs
  - docs/defects/2026-07-27-created-package-unloadable.md (the same shape, for Studio's create)
  - lp2025/2026-10-27-0114-mapping-rebuild (M3 P5)
---
# `lp-cli create project` wrote a project that did not load

**Symptom** — Loading the starter project (`starter_project_files`, what
`lp-cli create project` and `lp serve --init` scaffold) through the
engine's `ProjectLoader` failed outright:
`InvalidProjectReference { path: "/texture.json", reason: "binding `input`
names no declared slot on Texture" }`. Nothing rendered.

**Root cause** — The starter bound its texture node's `input` to
`bus:visual.out` (since f9f1e0a52d, 2026-07-27). A texture declares no
`input` slot (`TextureDef` holds `size` and `bindings` only), and the loader
refuses a binding to an undeclared slot. The starter's own tests checked
that every file parses and is canonical, never that the project loads, so
nothing noticed once the loader started refusing.

**Fix** — The starter's texture is written with no bindings, as every
texture in `catalog/` and `projects/` is. `creatable_projects_land.rs`
loads, renders and reads the landing of every project `lpc-model`'s
templates create (the starter, the 1D and 2D pattern projects), so a
template that does not load fails there.
