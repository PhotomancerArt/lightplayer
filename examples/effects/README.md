# Effect examples

Each directory here is a **workbench project** — the unit the gallery
opens — wrapping a vendorable **effect**:

```
examples/effects/<name>/
  project.json      workbench root: clock + preview fixture + output,
                    plus one nodes{} entry referencing the effect
  fixture.json      the preview rig (its input reads the effect's
                    output mirror via a node: ref)
  <name>/           THE EFFECT — the folder Studio vendors by copy
    project.json    kind Project, with promoted controls{} and
                    author/version/license provenance
    …               the effect's shaders and assets
```

The effect is a plain project (effects-are-projects ADR,
`docs/adr/2026-07-28-effects-are-projects.md`): its bus channels are
scoped to its own subtree, it inherits `time` from whatever hosts it, and
its produced `output` mirror is what playlists and fixtures read. Copying
the inner folder out keeps it standalone-openable (it carries its own
`format`).

- `plasma/` — pure visual: classic folded-sine plasma, promoted `speed`
  and `scale`.
- `meteor/` — compute + visual pair: particle heads persisted in compute
  state (`sim`), trail rendering (`render`), promoted controls from both
  children (`speed`, `count`, `decay`).

All sample content in this repository (examples and effects) is CC0
unless otherwise noted.
