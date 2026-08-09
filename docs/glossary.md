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
- **Public / private slot** — public = has an **authored** bus binding
  (and therefore a channel, and therefore a panel presence); private =
  unbound. Binding *is* publicity. A binding the loader materialized from
  the slot's own `default_bind` does NOT make it public — the channel is
  wired and listed, but there is no control
  ([ADR](adr/2026-08-03-panel-visibility-is-derived.md)). (module model)
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
  root module's panel is the end-user surface. In Studio the section
  *labeled* `panel` appears on module cards only, wearing the
  panel-primary tint and a ▶ rail mark — the panel is the thing play mode
  renders (wiring spike gate 2, ADR
  2026-08-04-wiring-flow-and-panel-settings). (module model)
- **Settings** — the label on a *leaf* card's own bound-slot strip: the
  knobs configuring that one node. Same publicity underneath as a panel
  control — the leaf's bound slots simultaneously surface on the
  enclosing module's panel — the label just distinguishes "one node's
  knobs" from "a scope's performable surface". Replaced the leaf-side
  `controls` section label. (module model)
- **Face** — a node's kind-specific card presentation in the workspace
  (preview hero, entries strip, drawers): the authoring instrument. A face
  *hosts* the node's panel; play mode renders panels without faces.
- **Drawer** — a collapsible authoring surface below a face (code,
  advanced slots, wiring).
- **Wiring drawer** — the drawer on a module card drawing its scope's
  channels as a flow: writer chips → arrowed wires → the channel's value
  box (name + live value/product) → reader chips, wire color matching
  chip color (violet = driving write, orange = engaged panel writer,
  grey = reader / out-ranked); child-scope readers list with their scope
  path (R5). Bus-as-plumbing, where the panel above is bus-as-controls.
  One per scope, hung off the module that owns it. It replaced the
  sidebar *bus pane*, which is gone. (wiring spike, ADR
  2026-08-04-wiring-flow-and-panel-settings)
- **Panel state** — unauthored runtime writer state per (scope, channel);
  persisted to `.lp/panel.json` with throttled writes; never dirties the
  project. (module model)
- **Engaged (Latch)** — a panel control whose lazy runtime writer has
  materialized (it was touched): it captures the channel, overriding
  authored writers in its scope and shadowing outer control until
  cleared — lighting-console programmer / DAW latch semantics.
  (module model)
- **Reset (Clear)** — removing panel writers (per control / module /
  whole panel), restoring authored, inherited, or default resolution.
  (module model)
- **Slew** — optional shaping of a panel writer's output toward its held
  value (anti-zipper); controls shape, they never integrate — anything
  accumulating state is a node. (module model)
- **Takeover** — the policy for grabbing a channel authored dataflow is
  moving: jump (default for touch controls), pickup/scaled reserved for
  absolute hardware inputs. (module model)
- **Momentary** — the control class for gesture channels (touch sets,
  buttons): writes while the gesture is active, despawns on release —
  which is itself the fallback mechanism — never latches, never
  persists. (module model)
- **Play mode** — rendering only the root module's panel: the end-user
  view. (module model)
- **Debug** — a slot that is *transient by nature*: a diagnostic or authoring
  override with **no durable value underneath** (clock `rate`, output
  `test_pattern`). Session-only — it dies on project unload or reboot, so a
  restarted installation never comes up in a debug state. Not a Panel: a
  panel control *exposes a bound slot via its channel* (authored value =
  default), which is why latching panel state persists and Debug does not
  (momentary panel gestures don't persist either, but their fallback is bus
  resolution, not a shape default). Never dirty, never saved;
  its verb is **Clear**. Declared `#[slot(role = "debug")]`, rendered in the
  node card's own hazard-striped Debug section
  ([ADR](adr/2026-08-01-debug-slots-taxonomy.md)).
- **Dimensionality** — the label on the drawer where a node's *declared
  space* is authored: on a shader card between `code` and `advanced`, on
  a fixture card below `settings`. The two are mirror images — the shader
  says what it renders and how a 2D fixture should receive it, the
  fixture says what it does with a 1D source. Its rows are claimed out of
  the advanced drawer, so it is the only writer for them.
  ([ADR](adr/2026-08-09-dimensionality-authoring-surface.md))
- **Declared space** — the authored `1D`/`2D` fact on a node, never
  inferred: a shader's `space` slot, which must agree with the GLSL entry
  it defines (`render_1d(float)` / `render_2d(vec2)`) or the card shows a
  mismatch error.
  ([ADR](adr/2026-08-07-two-sided-space-model.md))
- **Projection** — how a 1D source fills a 2D consumer, authored as a
  base **shape** (extrude-x, extrude-y, radial, angular) plus two
  modifiers, **mirror** (fold around the midpoint) and **flip** (reverse
  the strip). Executed by the *producer* at the sampling boundary — never
  a node in the graph. The producer always declares one; a fixture may
  override it.
- **Along the wire** — the fixture-side choice that runs a 1D source in
  wire order and ignores the mapping (`strip_order_meaningful`), with a
  forward/reversed direction. The WLED-familiar behavior, and the default
  for mapped fixtures.
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
