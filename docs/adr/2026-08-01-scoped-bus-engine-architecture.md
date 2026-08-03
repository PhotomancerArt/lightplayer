# Scoped-bus engine architecture: structural scopes, scoped resolver keys, module runtime

- Status: accepted
- Date: 2026-08-01
- Context: implements docs/design/modules.md R1–R7 (ratified 2026-07-31);
  supersedes the two ADRs on the closed #218 spike branch —
  `2026-07-28-scoped-buses.md` and `2026-07-28-effects-are-projects.md`
  (branch `claude/composite-effects-planning-f4f51b`, kept as a harvest
  source). The spike's *model* survived review; this records how the
  restructured implementation differs.
- Plan: `planning/2026-08-01-1003-modules-impl-roadmap` P4–P6.

## Decision

**Scope is structural engine state, not a load-time side table.**
`RuntimeNodeEntry` carries the scope a node inhabits
(`ScopeRef::Module { owner }` / `ScopeRef::Sink { owner, entry }`) plus an
introduces-scope bit, assigned inside `ensure_runtime_spine` — the single
code path both fresh load and `apply_project_changes` run — so an edited
project can never wear different scopes than a reloaded one (pinned by a
load-vs-apply differential test). `Pending`/`Failed` entries carry scope
too (R1: the engine always answers), and payload reattach never touches
it. The spike's `BusScopes` table, built and dropped inside one loader
function, is the rejected alternative.

**The resolver stays scope-dumb; scope arrives as a richer key.**
`QueryKey::Bus` is `{ scope, channel }`: the reading node's scope is part
of the cache and cycle-detection identity, so same-named channels in
different scopes can neither collide in the cache nor fake a cycle. The
host answers "which providers win for a read from this scope"
(`NodeTree::providers_for_bus_read`): a pure outward writer-shadowing walk
(R5) that never descends into a scope — which is what makes sink-scope
no-demand (R2) hold *by construction*. The spike's probe-side filter for
inactive playlist entries is the rejected alternative; the pinned test
asserts a probe read with `include_values` never ticks a sink-scope
producer. The playlist ownership-suppression rule is deleted outright:
entry children publish into their entry's sink scope like any producer.

**Reading-scope rule.** A node reads from the scope it inhabits — except
scope *introducers*, whose bus reads face inward (the root's unscoped
reads are root-scope reads; a module export republishing an inner channel
reads it from the scope the module introduces). Write-side classification
is always the owner's inhabited scope (R4: produces write locally; a
module node resides in its parent's scope).

**A real `ModuleNode` runtime, root included.** Every module-kinded node
wears the mirror runtime (harvested from the spike's `ProjectNode`):
`produce` resolves the introduced scope's `visual.out` and forwards
render/sample dispatch to the producer; no writer renders cleared. The
loader registers the R7 surface: authored bindings (the contention pick),
authored exports, and the automatic `output` → `visual.out` fallback
publish for non-root modules (drop-in embedding). Root is no longer a
placeholder special case — the same runtime attaches on load and on the
apply path's reattach. The runtime is deliberately never feature-gated
(every project has a root module; C6 headroom after: ~244 KB).

**Primary visual is an engine-reported role.** `WireBusChannel` gains
`primary_visual`, decided once in the probe (the root scope's listing of
the vocabulary channel the root mirror reads). Studio and friends consume
the flag; `channel.name == "visual.out"` string tests are dead. The name
comparison survives in exactly one place, engine-side, next to the
vocabulary constant.

**Persisted scope identity.** A scope's stable string is its owner's tree
path; a sink scope keys by the authored playlist entry
(`…/entries[k]`) — stable under sibling reorder (names and keys, never
indices), stable across reattach/reload, and following the entry SLOT
rather than its content, so swapping what an entry plays keeps the
entry's panel state. This string becomes the panel-state key prefix in
the panel phases; it was chosen before any device persists one.

## Consequences

- Depth-2 composition works and is pinned (E5): module publishes and
  exports count as writers in resolution like any producer — the spike's
  latent `collect_writers` omission cannot recur silently.
- Two fallback writers in one scope (host visual + embedded module's
  publish) resolve ambiguous-until-authored — the accepted consequence;
  the pick is authorable on the module node (`ModuleDef.bindings`).
- Feedback via one channel in one scope reports as a cycle; chains that
  need explicit topology use `node:` refs (E5 note).
- The wire still lists channels flat; the structured `WireScopeRef`
  surface and per-scope listings land in the next phase (P7).
