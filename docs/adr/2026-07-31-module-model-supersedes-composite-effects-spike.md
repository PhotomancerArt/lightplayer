# ADR: Module model supersedes the composite-effects spike direction

- **Status:** Accepted
- **Date:** 2026-07-31
- **Deciders:** Photomancer
- **Supersedes:** The composite-effects spike direction of PR #218
  (`claude/composite-effects-planning-f4f51b`, **not merged**) and its two
  branch-only ADRs, `2026-07-28-scoped-buses.md` and
  `2026-07-28-effects-are-projects.md` — those never landed on main and
  are retired with the branch. Builds on node card faces
  (`2026-07-26-node-card-faces.md`), declarative default bindings
  (`2026-07-09-declarative-default-bindings.md`), and node authoring
  operations (`2026-07-27-node-authoring-operations.md`).
- **Superseded by:** None

## Context

PR #218 implemented "scoped buses + embeddable projects": writer-shadowing
bus scopes keyed `(scope, name)`, projects embeddable as child nodes,
`PromotedControlDef` control aliasing, and two shipped example effects.
Before merge, Yona's product vision sharpened — "the bus logically lives
in the project node; project nodes are bus holders and the place you
define the public interface" — and asked for a design review of the spike
against it (2026-07-31 session; planning artifact:
`planning/2026-07-31-1002-modules-buses-panels`).

The review found the spike's *model* essentially matched the vision while
its *implementation* did not:

- Scope was a transient loader side-table (`BusScopes`), built and dropped
  inside one binding pass — post-load, nothing could answer "what scope is
  node X in". The bus stayed a flat global index with a compound key.
- The wire had no structured scope — non-root channels reached Studio as
  display strings (`"plasma›visual.out"`), and playlist-entry scopes had
  to be hand-filtered out of the probe after they caused every inactive
  entry to render each refresh.
- Root was special-cased in ~7 sites across three crates, most sharply
  Studio's `channel.name == "visual.out"` string test for "the primary
  visual".
- A latent depth-2 defect: writer collection skipped project nodes, so
  the output mirror's own `visual.out` publish was absent from the
  shadowing table — depth 1 resolved correctly only because the no-writer
  fallback also landed on root; at depth 2 a consumer beside an embedded
  effect resolved to root's visual instead of its sibling's. Untested.
- Control promotion was value-less aliasing resolved twice, in different
  languages: structurally in the loader, then again in Studio by
  string-matching projected UI DTO rows.
- (Incidental: a duplicated `#[test]` attribute on the branch silently
  disabled `binding_graph_probe_reports_bindings_channels_and_values`.)

The subsequent design discussion (session log in the planning directory's
`notes.md`, D1–D8) converged a stronger model — notably: promotion must be
automatic, control meta must follow whatever is currently bound (the
playlist meta-switching case), and panel edits must not dirty the project
— which the spike's aliasing could not express.

## Decision

1. **`docs/design/modules.md` is the normative model** (project/module
   mitosis, binding-is-publicity, panel as a first-class per-node view,
   lazy runtime panel writers, sink scopes as a modeled property). The
   design doc states what is; this ADR records how we got here.
2. **PR #218 is not merged.** The branch is kept as a harvest source and
   the PR closed once harvesting (below) is complete.
3. **Harvest inventory** — what carries forward and what dies:

| Spike piece | Fate |
|---|---|
| Writer-shadowing resolution (produce-local / consume-inherit) | Kept → R4/R5 |
| Anonymous playlist-entry scopes | Kept, promoted → sink scopes as modeled property (R2) |
| Output mirror + non-root fallback publish | Kept, generalized → R7 (authored exports added) |
| `ScopedChannel` key through `QueryKey`/binding index/resolver | Kept as mechanism (resolver stays scope-dumb) |
| Effect-face component + panel widget reuse | Kept as the UX spike's starting point |
| plasma / meteor example content | Kept as future example modules |
| Cross-target uniform-indexing filetest + defect/debt entries | Landed on main independently (side task) |
| `PromotedControlDef` value-less aliasing + DTO string-matching | Dropped → R3 (binding is publicity); only the meta-override vocabulary survives (R9) |
| Transient `BusScopes` side-table | Dropped → R1 (structural scope state) |
| Display-string scope on the wire; probe-side entry filtering | Dropped → structured wire scope; R2 by construction |
| "Resolve to root, surface unfilled" | Replaced → R6 (authored default; channel still lists) |
| Root special-casing (~7 sites) | Deleted — root is not special (R1) |

4. **Defect classes the implementation must pin with tests:** the depth-2
   resolution shape (design doc E5) and sink-scope no-demand (R2.2). Do
   not inherit the branch's disabled probe test.
5. **Naming:** the container node kind is renamed `project` → `module`
   (one-shot, alpha, no migration); "project" survives as the container/
   folder concept only. This reverses the spike-era "Module off the
   table" kickoff lean — the project/module mitosis (container vs node)
   did not exist then and is what makes the name correct now.

## Consequences

- The two spike ADRs are not carried to main; their surviving content is
  re-derived in `docs/design/modules.md` and future implementation ADRs.
- Implementation planning (the M3 roadmap run in the planning directory)
  sequences the rename, engine restructure, wire change, panel runtime,
  and Studio rebuild, harvesting per the table above.
- The "effects are projects" insight survives unchanged as "effects are
  modules": a category, not a structure.

## Alternatives considered

- **Merge #218 and iterate** — rejected: the aliasing layer, transient
  scopes, string-typed wire scope, and root special-casing are all things
  the converged model deletes; merging would mean building the new model
  through partial reverts of the old one, with the depth-2 defect live in
  the meantime.
- **Keep value-less control aliasing** (`PromotedControlDef`) — rejected
  by the converged requirements: promotion must be automatic (aliases
  require authoring), meta must follow the currently-bound slot (aliases
  freeze a target), and panel edits must not dirty the project (alias
  writes land on authored child slots).
- **Keep "project" as the node kind with UI-copy disambiguation** —
  rejected: the copy ambiguity was structural, not cosmetic; the mitosis
  gives each concept one name.
