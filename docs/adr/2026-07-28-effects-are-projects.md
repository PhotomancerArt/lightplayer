# ADR: Effects are projects — embeddable projects, promoted controls, vendoring by copy

- **Status:** Accepted
- **Date:** 2026-07-28
- **Deciders:** Photomancer
- **Supersedes:** The deliberate authoring guard "kind Project cannot be
  created as a child node" (`lpc-registry` node authoring; the guard
  existed because child-project semantics were undefined — this ADR
  defines them). Builds on scoped buses
  (`2026-07-28-scoped-buses.md`), node authoring operations
  (`2026-07-27-node-authoring-operations.md`), and node card faces
  (`2026-07-26-node-card-faces.md`).
- **Superseded by:** None

## Context

The product goal is a library of visual effects that are easy to author
standalone and easy to drop into an existing project's playlist. The
kickoff design sketched a new `composite` node kind: a folder with a
`composite.json` boundary file, promoted controls, and a forwarded
output. Discovery collapsed that design:

- `ProjectDef` is already a minimal container — `format`, `uid`,
  `name`, `nodes{}`. No fixture/board/bus baggage lives in the def.
- The loader already treats a project node as a generic no-op container;
  the inventory walk follows `nodes{}` refs generically; nothing in the
  load path prevents nesting. The only hard block was the authoring
  guard.
- A separate `composite` kind would duplicate the container shape,
  split the gallery/open/authoring flows into two parallel paths, and
  force an answer to "when does a composite graduate into a project?"

An effect kinda *is* a project. What an effect needs beyond the bare
container — bus isolation, a playable output, curated knobs,
provenance — are additive properties, not a different structure.

## Decision

**No new node kind.** The kind string stays `project`; there is no
format bump and no migration. "Effect" is semantic — a project that
declares promoted controls and ships in the Effects gallery category —
not structural.

The effect boundary contract is:

1. **Bus scope** — every project node is a bus scope (scoped-buses
   ADR): embedding is isolation-safe by construction.
2. **Output mirror** — every project node exposes produced `output`
   mirroring its scope's `visual.out` (scoped-buses ADR): an effect is
   playlist-playable like any visual node.
3. **Promoted controls** — `ProjectDef` gains an optional
   `controls: { <name>: PromotedControlDef }` map. A promoted control is
   an **alias, not a mirror**: it carries a target (`node:./child#slot`,
   a direct child's slot) plus optional label/unit/min/max display
   overrides, and **no value** — values live on the target slot, so
   overlay dirty state, transient edits, and bound-violet UI state all
   observe the one real slot with no sync machinery. (This deliberately
   differs from `ShaderSlotDef`, which owns defaults; an alias that
   carried a default would create two sources of truth.)
4. **Provenance** — `ProjectDef` gains optional `author`, `version`,
   `license` strings (no semver semantics yet). `uid` and `name`
   already exist.

Supporting decisions:

- **Promotion is Studio-side DTO aliasing.** The effect card face emits
  panel controls whose slot **address is the inner child's**; the
  standard address-routed write path does the rest. No server-side
  alias slots, no new write machinery. Targets are restricted to direct
  children in this slice.
- **Vendoring by copy.** Importing an effect copies its folder into the
  host project (`effects/<name>/`) and attaches it with the existing
  byte-oriented create operation (`WireCreateNodeRequest`'s
  file+assets seam, exactly the reuse the node-authoring ADR designed
  for). No reference/link semantics, no update/diff flow this slice;
  the effect's own provenance fields are the only lineage.
- **Non-root `format` tolerance.** `format` is only probed at the
  project root; a child `project.json` carrying `format` loads with the
  field ignored. Vendored effect folders keep their `format` so a
  copied-out folder remains standalone-openable; a future offline
  upgrader owns skew between root and vendored formats.
- **Relative refs survive vendoring by construction.** `node:` binding
  refs are node-tree-relative and artifact refs are file-relative, so a
  copied folder's internal wiring is location-independent.

## Consequences

- One authoring surface: effects are opened, edited, created, and
  version-controlled exactly like projects. The gallery's Effects
  category and the declared `controls` are the only distinctions.
- Existing artifacts are byte-identical: all new `ProjectDef` fields are
  optional and serialize skip-if-default.
- Old firmware rejecting the new optional fields is a non-issue (fields
  are additive; device never upgrades formats — offline upgrading is
  Studio's job).
- The one hard error class this introduces: a `controls` entry whose
  target does not resolve to a direct child's slot is a load error with
  a path-qualified message (fail loud at load, not silently dead knobs).
- Naming: "project" remains the formal name; UI copy leans on the
  Effects category so users mostly meet "effect". The copy ambiguity
  with "the project you have open" is handled in Studio copy, not in
  the model.

## Alternatives considered

- **A dedicated `composite`/`effect` node kind** — rejected: duplicates
  the container structure, forks every authoring/gallery/open flow,
  requires schema+kind churn, and leaves "project vs composite" as a
  permanent taxonomy question. The unification costs one authoring
  guard and two optional field groups.
- **Auto-collected controls** (promote every panel-flagged inner slot)
  — rejected for effects: the promoted set is the effect's public API
  and should be curated. (Auto-collect may still make sense as a
  default for plain projects later.)
- **Server-side alias slots** for promotion — rejected: heavyweight
  (new slot kind, write-path forwarding, dirty-state sync) for what the
  DTO layer does with an address.
- **Import by reference** (shared library folder, linked updates) —
  deferred: update/versioning semantics deserve their own slice;
  copy-vendoring is predictable and offline-safe.
