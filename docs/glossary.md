# Glossary

Shared vocabulary for LightPlayer. One line per term; follow the links for
the normative treatment. Terms marked **(module model)** follow
[`docs/design/modules.md`](design/modules.md), which may be ratified ahead
of implementation — code can lag these names during the transition.

## Containers & artifacts

- **Project** — the *container* you open in Studio: a folder with
  identity, provenance, history, and `format`. Not a node. (module model)
- **Module** — the container *node kind* (formerly the `project` node
  kind): owns a bus scope, a panel, and child nodes. The root module is an
  ordinary module at the root. (module model)
- **Effect** — a *category* of module authored to be dropped into other
  projects; a gallery/UI term, not a structure. (module model)
- **Node** — one unit of the runtime graph (shader, clock, playlist,
  module, …), authored as one JSON artifact file per node.
- **Slot** — a named, typed value surface on a node (consumed or
  produced); the unit that bindings, edits, and panel controls address.
- **Artifact** — an authored JSON file (node, project manifest);
  serialized deterministically, byte-identical for unchanged models.
- **Vendoring** — importing a module by *copying* its folder into the host
  project: **copy-to-own** (the shadcn model) — the source becomes the
  user's; provenance records origin. No links, no update flow.
  (module model)
- **`.lp/`** — the single framework-owned directory in a project folder
  (panel state, future caches); never authored, always safe to delete.
  Everything else in the folder is user-owned content. (module model)
- **Provenance** — author/version/license/created metadata; may sit on any
  node; copied onto a module when it is extracted from a project.
  (module model)
- **Workbench project** — the wrapper project synthesized to open a bare
  module standalone (preview fixture, clock, etc.). (module model)

## Bus & dataflow

- **Bus** — the "just works" wiring layer: named channels that slots read
  and write via bindings instead of point-to-point references.
- **Scope** — the bus namespace a module introduces around its children;
  scopes form a tree. (module model)
- **Sink scope** — the anonymous isolating scope wrapped around each
  playlist entry: invisible to enclosing panels/probes, never demanded
  while inactive, but entries still inherit outward. (module model)
- **Channel** — a named value stream within a scope; exists iff some
  binding names it — there is no channel registry.
- **Binding** — an authored or default connection between a slot and a bus
  channel (or another node's slot via `node:` ref).
- **Default bind** — a slot-declared binding (`default_bind`) materialized
  at load at fallback priority; the mechanism behind zero-wiring behavior
  like `time`.
- **Public / private slot** — public = has a bus binding (and therefore a
  channel, and therefore a panel presence); private = unbound. Binding
  *is* publicity. (module model)
- **Writer-shadowing** — consume resolution: a read resolves to the
  nearest enclosing scope with a writer; writes always land in the
  producer's own scope. Reads inherit, writes stay local. (module model)
- **Export** — an authored binding on a module node republishing an inner
  channel into the containing scope; `visual.out` is the one automatic
  publish. (module model)
- **Output mirror** — every module's produced `output` slot, reflecting
  its scope's `visual.out`; what makes any module playlist-playable.
  (module model)

## Panel & faces

- **Panel** — a node's *control surface*: the presentation of its public
  surface. Leaf node → its bound slots; module → its scope's channels plus
  nested child-module groups; playlist → the active entry's panel. The
  root module's panel is the end-user surface. (module model)
- **Face** — a node's kind-specific card presentation in the workspace
  (preview hero, entries strip, drawers): the authoring instrument. A face
  *hosts* the node's panel; play mode renders panels without faces.
- **Drawer** — a collapsible authoring surface below a face (code,
  advanced slots, bus wiring).
- **Panel state** — unauthored runtime writer state per (scope, channel);
  persisted to `.lp/state.json` with throttled writes; never dirties the
  project. (module model)
- **Engaged** — a panel control whose lazy runtime writer has
  materialized (it was touched): it overrides authored writers in its
  scope and shadows outer control until reset. (module model)
- **Reset** — removing panel writers (per control / module / whole panel),
  restoring authored, inherited, or default resolution. (module model)
- **Play mode** — rendering only the root module's panel: the end-user
  view. (module model)
- **Bound (violet)** — the UI state family for "this value comes from a
  binding/bus"; always violet, never green (green = valid only).
- **Dirty** — an authored value differing from its saved artifact
  (overlay-derived); panel state is never dirty.

## Runtime & tooling

- **Engine** — the shared runtime (loader, binding index, resolver,
  nodes); identical across sim and device via `lpa-server`.
- **Sim / device parity** — the requirement that model semantics live in
  the shared load path so browser-sim and firmware behave identically.
- **Probe** — a read-only wire query of runtime state (e.g. the
  binding-graph probe feeding bus views).
- **Story** — a captured Studio component state used for visual baselines
  (CI-canonical capture).
