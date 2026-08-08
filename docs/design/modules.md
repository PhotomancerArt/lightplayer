# Modules, Buses, and Panels

This document is the single source of truth for the **module container
model**: what a module is, how bus scoping works, how slots become public,
and how the panel — the control surface — derives from the bus. It
describes what *is* (once implemented, what the system does); the decision
context that produced it, including its relationship to the
composite-effects spike, is recorded in
`docs/adr/2026-07-31-module-model-supersedes-composite-effects-spike.md`.

> **Status: Ratified 2026-07-31** (gate G1 of
> `planning/2026-07-31-1002-modules-buses-panels`). The open-question
> register (§9) records accepted leans — revisit at implementation, not
> before.
>
> **Posture.** Alpha: no wire or artifact compatibility obligations; the
> rename below is one-shot with no migration. The architecture is required
> to mirror the *user model* — a module is one idea (a bus holder with a
> public face) presented one way at every nesting depth. Where this
> document and an implementation convenience disagree, this document wins.
>
> **Related:** `docs/design/panel.md` (control behavior — the normative
> panel treatment; R9–R13 below are summaries of its rules),
> `docs/glossary.md` (terms),
> `docs/adr/2026-07-26-node-card-faces.md` (face grammar),
> `docs/adr/2026-07-09-declarative-default-bindings.md` (default binds),
> `docs/adr/2026-07-27-node-authoring-operations.md` (create/vendor seam),
> `docs/adr/2026-08-01-scoped-bus-engine-architecture.md` (R1–R7 as
> built),
> `docs/adr/2026-08-03-panel-visibility-is-derived.md` (R3/R8/Q13 as
> built).

## 1. Concepts

| Term | Is | Is not |
|---|---|---|
| **Project** | The *container*: a folder with identity, provenance, history, and format. Not a node. What you open in Studio. | A node kind; a scope; anything the engine resolves against |
| **Module** | The container *node kind* (replaces the `project` node kind). Owns a bus scope, a panel, and child nodes. The root module is an ordinary module that happens to be at the root. | Special at the root; a separate "composite"/"effect" structure |
| **Effect** | A *category* of module (gallery/UI copy): a module authored to be dropped into other projects. | A distinct node kind or schema |
| **Scope** | The bus namespace a module introduces around its children. | Global; a display prefix |
| **Channel** | A named value stream within a scope. Exists iff some binding names it. | A declared registry entry; a place values are stored |
| **Public slot** | A slot with a bus binding. Its channel appears in its scope. | A slot copied, aliased, or mirrored anywhere |
| **Panel** | A node's *control surface*: the presentation of its public surface (R8). Every node has one; the root module's is the end-user surface. | A place values live; a dataflow construct; face-specific |
| **Face** | A node's kind-specific card presentation in the workspace — the authoring/monitoring instrument. A face *hosts* the node's panel plus authoring-only surfaces (drawers, code, entries strip). | The panel itself; anything play mode renders |
| **Panel state** | Unauthored runtime writer state (per scope + channel), persisted separately. | Part of the project's authored artifacts; dirty-tracked |

The project/module split ("mitosis") is deliberate: the project carries the
*workspace* concerns (who made this, when, what version, format), the
module carries the *technical spec* (what nodes exist, how they wire, what
is public). A vendored effect is therefore just a module folder; wrapping
it in a project makes it openable; extracting it from a project is a copy
(see R14 for provenance).

## 2. Naming and rename plan

**"Module"** replaces "project" as the node kind. One-shot, no migration,
no format bump semantics to preserve (alpha). Inventory:

- Kind string `project` → `module` in artifacts; `ProjectDef` → `ModuleDef`
  (gains `bindings`/exports and optional provenance; loses nothing).
- Root file split: `project.json` becomes the non-node container manifest;
  the root module moves to `module.json` (§6).
- Schemas regenerated; `examples/` rewritten; conformance fixtures updated.
- Wire/engine identifiers follow the concept they name: things about the
  *loaded tree* or *container* may keep "project" (e.g. "open project"
  flows); things about the *node kind or scope* become "module". The
  implementation plan owns the exhaustive sweep; the rule here is only
  that no identifier may use "project" to mean the node kind.
- Studio copy: users "open a project", "add a module", browse "Effects".
- "Plugin" is rejected (connotes third-party binaries / dynamic loading).

> Status: the kind/type rename (`ModuleDef`, `NodeKind::Module`, kind
> strings `Module`/`module`, tree paths `.module`, schemas, corpora,
> wire bump) landed 2026-08-01. The file split (§6) is the next phase.

## 3. Bus rules (normative)

### R1 — Modules introduce scopes, structurally

Every module node introduces a scope enclosing its children. The root
module's scope is the root scope — root is *not otherwise special*. Scope
introduction is a property of the module kind, not the invocation site.

Scope identity is **structural engine state**: after load, the engine can
answer "which scope contains node X" and "which scope does module M
introduce" — a transient load-time-only scope table is non-conforming.
The scope is engine-owned runtime structure hung off the module node;
authored artifacts never name scopes.

### R2 — Sink scopes (isolating invocation sites)

Isolating invocation sites — today exactly playlist entries — wrap each
owned child in an anonymous **sink scope**, parented into the enclosing
scope. *Sink* is a modeled property with two meanings, and both must be
honored by construction rather than by per-layer filters:

1. **Inward invisibility:** channels in a sink scope never surface on any
   enclosing scope's panel or probe listing. They are reachable only
   through the isolating node's own face (the playlist face presents the
   *active* entry's panel — see E2).
2. **No demand:** listing a panel or probe must never force resolution of
   an inactive sink child's channels — otherwise every inactive playlist
   entry renders on every listing (an observed failure class; see the
   companion ADR). The property lives in the model, never as a per-layer
   filter.

Entries are *alternatives* (isolated from one another); module children
are *collaborators* (share one scope). That is why there are exactly two
scope-introduction primitives and "container ⇒ scope" is wrong.

### R3 — Binding is publicity

A slot is **public iff it has a bus binding** (authored, or a declared
`default_bind`). Public means: its channel exists in the slot's scope,
which is what makes it appear on that scope's panel (R8). Private slots
are simply unbound. There is no other promotion mechanism — no promoted
control defs, no alias slots, no module-side declaration required. A novel
shader param becomes a panel knob by being bound to a channel; that's the
whole gesture.

Publicity carries no value: values live on slots (authored defaults) and
in panel state (R10). Publicity is symmetric — produced slots (a shader's
`visual.out`) and consumed slots (a shader's `speed` uniform) are both
made public by binding.

### R4 — Produces write locally

A produced bus endpoint always writes the producer's **own nearest
scope**. Never outward. A module's interior cannot clobber its host by
construction. (A module node itself resides in its *parent's* scope — only
its children are inside the scope it introduces — so a produced endpoint
*on the module node* writes the parent scope. R7's publishes are
instances of this rule, not exceptions.)

### R5 — Consumes resolve by writer-shadowing

A consumed bus endpoint resolves to the **nearest enclosing scope that has
at least one writer** for the channel, starting at the consumer's own
scope and walking outward. Writers are: authored produce bindings,
produce-direction declared defaults, module publishes/exports (R7), and
engaged panel writers (R10).

Walking *out of* a sink scope is permitted — sinks isolate inward
visibility (R2), not outward inheritance; a playlist entry's shader still
inherits host `time`.

The consume/produce asymmetry is the point: reads inherit (an effect with
no clock animates on host time; an effect *with* a clock shadows time for
its own subtree), writes stay local (encapsulation).

### R6 — No writer anywhere: authored default, channel still lists

If no enclosing scope has a writer, the consuming slot uses its **own
authored default value**, and the channel **still lists** in the
consumer's scope. Listing is what makes the channel appear on the panel,
where touching the control materializes a writer (R10) — an unfilled
public input is an *invitation*, not an error.

### R7 — Module output interface: one automatic publish + authored exports

- Every module node produces an `output` slot mirroring its own scope's
  `visual.out` channel (resolved per R5 within the scope). This is what
  makes any module playlist-playable with zero playlist changes. A scope
  with no visual writer mirrors *cleared* — a module without a visual is a
  legitimate shape.
- Every **non-root** module node additionally publishes `visual.out` into
  its containing scope at fallback priority — the drop-in rule: an
  embedded effect contributes its visual to its host by default. (Root is
  excluded only because its containing scope does not exist.)
- Any **other** channel a module provides outward is an **authored
  export**: a binding on the module node republishing an inner channel
  into the containing scope (per R4, the module node's own produces land
  there naturally). Exports are the module author's curation of the
  output interface — automatic for the universal visual convention,
  deliberate for everything else, because auto-exporting all channels
  would make two side-by-side modules collide in the host scope, which is
  exactly what scoping exists to prevent.
- **Contention** (e.g. a host shader and an embedded effect both writing
  `visual.out` at fallback priority) resolves as *ambiguous until the
  author picks*. The pick is authorable — module nodes carry bindings —
  and Studio should make it a one-gesture choice at drop time.

Inputs need no counterpart: consumed inheritance (R5) already lets a host
feed a module's inner consumers with zero authoring (see E6).

> Status: implemented 2026-08-01 (engine C1–C3) — structural scopes,
> scoped resolver keys with writer-shadowing, the module mirror runtime
> (root included), automatic publish + authored exports, and the
> engine-reported primary-visual role. See
> `docs/adr/2026-08-01-scoped-bus-engine-architecture.md`.

> **Presentation amendment (2026-08-07, PR #387).** The R7 mirror still
> defines the module's *output interface*, but Studio's module-face
> **hero no longer leads with it**: when the module's scope resolves
> both primaries, the hero shows `control.out` — the lamps the project
> actually drives — with an icon-only toggle back to the visual
> (per-card `NodeCardUiState` preference, default control). The visual
> mirror is the fallback, not the lead; a fixture project's output IS
> its lamps. Project/Explore card thumbnails follow the same default
> with no toggle. See the amendment note on
> `docs/adr/2026-07-16-primary-visual-product.md`.

### R8 — The panel: one concept, every node, derived from publicity

The **panel** is a first-class per-node concept, not a feature of
particular faces: every node has a panel, and it presents the node's
*public surface*. No dataflow construct exists behind a panel; nothing is
promoted between levels.

> **Naming (2026-08-04, wiring spike G2).** In Studio the section
> *labeled* `panel` appears on **module** cards only, wearing the
> panel-primary tint and a ▶ rail mark — the panel is the thing play
> mode renders. A **leaf** card's own bound-slot strip is labeled
> `settings`: its knobs configure that one node, and the same publicity
> simultaneously surfaces as controls on the enclosing module's panel.
> The derivation below is unchanged — every node *has* a panel in the
> model; the label is reserved for the module/product surface so the two
> readings stop colliding. See
> `docs/adr/2026-08-04-wiring-flow-and-panel-settings.md`.

- A **leaf node's** panel presents its public (bound) slots. Publicity
  (R3) is the one gesture that puts a control on a panel — this
  **subsumes the legacy `panel: bool` slot flag** — deleted outright in
  P2: "add to panel" *means* "bind to a channel". (Widget choice, step,
  unit remain slot meta.)
- A **module's** panel presents its scope's channel list — which is the
  aggregate of its children's publicity — plus each child module's panel
  as a **nested group** (presentation recursion). Two embedded instances
  of the same effect present two independent groups.
- An **isolating node's** panel (playlist) presents the *active* sink
  child's panel (R2); inactive siblings surface nowhere.
- The **root module's panel is the end-user surface**. Play mode renders
  panels only — no faces.
- A **face** is a different concept: the kind-specific card presentation
  in the workspace (preview hero, code drawers, entries strip — the
  authoring instrument). A face *hosts* its node's panel; it is never
  itself the panel. Faces appear in the workspace; panels appear on faces
  *and* stand alone in play mode.
- Presentation is kind-dependent: scalar channels render as knobs/faders,
  bools as toggles, visuals as preview tiles, streams as readouts.
  Channels driven by authored writers (an LFO, a clock) render as live
  readouts that can be *grabbed* (R11).

### R9 — Control meta follows the binding *(summary — normative: panel.md P6/P7)*

A control's display meta derives from the slot(s) **currently bound** to
the channel in that scope, re-derived whenever bindings change — a
playlist switch re-derives the control (E2). Ranges union on conflict; a
module-level authored per-channel override (label/unit/min/max, **no
value**) beats derivation — the curation escape hatch, and the only
module-side declaration in the model.

### R10 — Panel state: lazy, unauthored runtime writers *(summary — normative: panel.md P1–P3)*

Panel controls write through runtime writer state per `(scope, channel)`:
**unauthored** (never dirties the project — authored artifacts are
defaults and wiring; panel state is performance state) and **lazy** (the
writer materializes on first touch, in the scope where touched). Laziness
is load-bearing for R5: if every public slot self-shadowed, an outer
scope could never drive an inner channel. Writers hold values — they
shape, never integrate (panel.md P3).

### R11 — Precedence: an engaged panel writer wins its scope *(summary — normative: panel.md P2/P4/P5)*

Within a scope, an engaged panel writer outranks authored writers for the
same channel until cleared; across scopes, plain R5 applies — an engaged
inner writer shadows outer control for that subtree (touching detaches,
clearing re-attaches). The panel is latch-mode capture: values hold until
an explicit clear.

### R12 — Reset *(summary — normative: panel.md P2)*

Clear removes panel writers — per control, per module, or whole panel —
restoring R5/R6 resolution and dropping the persisted entries.

### R13 — Persistence *(summary — normative: panel.md P10/P11)*

Panel state persists by default to `.lp/panel.json` (§6) with throttled
writes (≥ ~10 s, flash preservation), auto-save on by default, and
restore **before first render** on boot. Never in authored artifacts.

### R14 — Provenance is a node capability; extraction copies it

Provenance metadata (§8) may sit on **any node**. Projects normally carry
it. When a node leaves a project (vendoring a module out), the copy
receives the project's provenance **unless it already carries its own**.

## 4. Worked examples

Notation: `Scope(X)` is the scope module `X` introduces; `sink(e)` is a
playlist entry's sink scope. Channel tables list `channel — writers →
readers` within one scope.

### E1 — Plain project (single module)

```text
project/  (project.json + module.json)
└─ M (root module)
   ├─ clock      seconds  → bus:time      (produced default)
   └─ shader S   time     ← bus:time      (consumed default)
                 speed    ← bus:speed     (authored: made public)
                 color    (unbound: private)
                 visual   → bus:visual.out (produced default)
```

Scope(M): `time — clock → S` · `speed — ∅ → S` · `visual.out — S → M.output`.

- `speed` has no writer anywhere → S uses its authored default (R6); the
  channel lists, so M's panel shows a `speed` knob with S's meta (R9).
  Touching it materializes a panel writer in Scope(M) (R10); the project
  stays clean; the value survives restart (R13).
- `color` is private: no binding, no channel, no knob (R3).
- Identical to today's behavior for existing flat projects, modulo the
  file split — single scope, same resolutions.

### E2 — Playlist, two speeds, meta follows the active entry

```text
M (root module)
└─ playlist P
   ├─ entry a: shader A   speed ← bus:speed  (meta 0–1,  "Drift")
   └─ entry b: shader B   speed ← bus:speed  (meta 0–10, "Whirl")
```

Scopes: Scope(M) ⊃ sink(a), sink(b). `speed` lists separately in each
sink scope (R2); neither surfaces on M's panel directly.

- P's face presents the **active** entry's panel (R2.1): with `a` active,
  one knob, range 0–1, label "Drift". Switch to `b`: the control
  re-derives (R9) — range 0–10, "Whirl".
- Panel state is per `(scope, channel)` (R10): tweak `a`, switch to `b`,
  switch back — `a`'s tweak is still there. The two entries never share
  or clobber a value.
- Inactive `b` is never demanded (R2.2) — listing M's panel or probing
  the bus must not render `b`.
- Both entries still inherit `time` from Scope(M) (R5 — sinks don't block
  outward walks).

### E3 — Drop-in embed (plasma into a host that already has a visual)

```text
H (root module): clock, shader S_h (visual → bus:visual.out, fallback)
└─ plasma (module)
   └─ shader S_p   time ← bus:time, speed ← bus:speed (0–1),
                   visual → bus:visual.out (fallback)
```

- `S_p.time` resolves: Scope(plasma) has no `time` writer → walk out →
  clock in Scope(H) (R5). **An effect with no clock animates on host
  time, automatically.**
- `S_p.visual` writes Scope(plasma) (R4). plasma's node publishes
  `visual.out` into Scope(H) at fallback (R7).
- Scope(H) now has **two** fallback `visual.out` writers (S_h, plasma) →
  ambiguous until the author picks (R7); Studio offers the pick at drop
  time. Had H no visual of its own, plasma's publish would win alone —
  true drop-in.
- H's panel shows its own channels plus a **plasma group** containing the
  `speed` knob (R8). Turning it writes Scope(plasma) — host channels
  untouched.

### E4 — Shared control (one speed driving two effects) + detach

```text
H (root module): control K (value → bus:speed, authored)
├─ plasma₁ … S₁.speed ← bus:speed
└─ plasma₂ … S₂.speed ← bus:speed
```

- Before K exists: neither `speed` has any writer → each S uses its
  authored default (R6); each plasma group shows its own knob.
- With K: untouched inner consumers walk out and inherit Scope(H)'s
  `speed` (R5). One host knob drives both effects. **Sharing is
  inheritance, not promotion** — no channel moved anywhere.
- Touch plasma₁'s own knob: a lazy writer materializes in Scope(plasma₁)
  (R10) and shadows K for that subtree (R11) — plasma₁ detaches; plasma₂
  still follows K. The UI shows plasma₁'s control as *engaged/overridden*
  (R11). Reset plasma₁ (R12) → re-inherits K.

### E5 — Depth 2 (module inside a module)

```text
H (root module): clock, …
└─ M_outer (module)
   ├─ M_inner (plasma module)      — publishes visual.out into Scope(M_outer)  (R7)
   └─ compute C   vis ← bus:visual.out   (e.g. analyzes the visual)
```

- `C.vis` resolves: Scope(M_outer) **has** a `visual.out` writer —
  M_inner's publish — so it resolves to the **sibling effect's visual**,
  never walking to root (R5). Module publishes and exports (R7) count as
  writers in resolution exactly like any other producer; an
  implementation whose writer accounting omits them fails only at depth
  ≥ 2, so **this exact shape must be pinned with a test** (see the
  companion ADR for the observed failure).
- M_outer's own `output` mirrors Scope(M_outer)'s `visual.out` = M_inner's
  visual (R7), and publishes it up into Scope(H) — nesting composes.
- Feedback loops via the bus are not a supported idiom: a node that both
  consumes and produces the *same* channel in one scope is a load error;
  chains that need explicit topology use direct `node:` refs.

### E6 — Audio pipeline module (non-visual; inputs & exports)

```text
A (module, "audio-analysis"):
   in    ← bus:audio.in       (consumed by FFT compute node)
   fft ⇒ energy → bus:audio.energy, beat → bus:audio.beat   (in Scope(A))
   exports (authored on A):    audio.energy ↑, audio.beat ↑

H (root module): audio-input node (samples → bus:audio.in)
├─ A
└─ plasma … S.brightness ← bus:audio.energy
```

- **Inputs are free:** A's inner `audio.in` consumer inherits H's
  audio-input writer by R5 — zero authoring on either side.
- **Outputs are curated:** `audio.energy`/`audio.beat` write Scope(A)
  (R4) and reach Scope(H) only via A's authored exports (R7). plasma's
  shader then inherits `audio.energy` from Scope(H) — **modules compose
  through the host scope**. Two audio modules exporting the same channel
  into H is the same authorable contention as E3.
- Standalone, A has no `audio.in` writer → FFT sees the authored default
  (silence, R6); its workbench project supplies a test-tone writer. A
  module with no visual is legitimate: its `output` mirrors cleared (R7).

### E7 — Fluid drawing module (gesture input + idle fallback)

The interactive-installation shape: a fluid sim driven by touches — from
an XY pad on a phone, an IR camera, or, when nobody is playing, a
generated idle animation of wandering synthetic touches.

```text
F (module, "fluid-draw"):
   gate node      in  ← bus:touches.in   (public gesture input; timeout
                  idle ← bus:touches.idle    param `idle_after`, public)
                  out → bus:touches
   friends node   (synthetic wandering touches) → bus:touches.idle
   emitter        touches → forces/dye        fluid sim → sim state
   visual shader  sim state → bus:visual.out
```

- `touches.in` has **no internal writer** — it is the module's inheritable
  input, exactly E6's pattern. Its panel control is a **multi-XY pad**, a
  *momentary* control (panel.md P14): touching writes Scope(F), releasing
  despawns the writer.
- Resolution while playing: pad engaged → your touches (nearest scope).
  Pad released → writer gone → walk outward → a host camera node writing
  `touches.in` in Scope(H), if present (R5). No camera → unwritten →
  the gate reads an empty set (R6).
- **Idle is a node, not a resolution feature**: the gate crossfades to
  the friends-generator's touches after `idle_after` seconds of
  empty-or-absent input. "Empty" matters — a connected camera seeing
  nobody is a live writer producing an empty set, which no
  writer-priority scheme can distinguish from activity; only domain
  logic can (which is why the timeout is a param, on the panel). Both
  silence cases collapse into the gate's one code path.
- In play mode on a phone, F's panel — the pad plus `idle_after`,
  friend-count, and the sim's public params — *is* the installation
  controller.

## 5. UI corollaries

One face, three zoom levels: the **effect author** works inside the module
(children expanded); the **artist** sees the module face as a card
(preview + panel group); the **end user** sees the root module's face
alone (play mode). The consequences: the root module returns to the node
area as the single top-level card (flat-root reversal — the root now
*does* something); the sidebar bus pane dissolves into the module face
(bus-as-controls) plus a wiring drawer (bus-as-writers/readers, the
pane's own rows); engaged-vs-inheriting state and the reset gesture
(R11/R12); the drop-time contention pick (E3).

> Status: landed 2026-08-03 (`planning/2026-08-03-1021-modules-vision-push`
> P2–P3, gate GV). The flat-root reversal, the module face at every depth
> (children below as sibling cards), the per-scope wiring drawer, and play
> mode (`#/sim|device/<key>/play`) are all real; the sidebar bus pane is
> deleted. 2026-08-04: the drawer's rows became the **flow view**
> (writers → value box → readers, from the wiring-UI spike;
> `docs/adr/2026-08-04-wiring-flow-and-panel-settings.md`). The
> contention pick (E3) is authorable on `ModuleDef.bindings` but has no
> gesture yet — the drawer shows the ambiguity as display only.

## 6. File layout

Two ownership tiers, and only two: everything in the project folder is
**user-owned content** except the single framework-owned `.lp/` directory.

```text
my-project/
├─ project.json          # container manifest — NOT a node:
│                        #   format, uid, name, provenance (§8), created
├─ module.json           # root module node: nodes{}, bindings, exports,
│                        #   per-channel meta overrides (R9), optional provenance
├─ shader.json …         # child nodes, one file per node (unchanged)
├─ effect/               # a LOCAL sub-module: any folder with a module.json,
│                        #   referenced by explicit path — no blessed location
├─ modules/
│  └─ plasma/            # an IMPORTED module: visible, committed, copy-to-own
│     ├─ module.json     #   (no project.json, no format — see below)
│     └─ …
└─ .lp/                  # the ONE framework-owned dir: never authored,
   └─ panel.json         #   always safe to delete (panel state per R13;
                         #   future caches/locks land here, never beside content)
```

- `format` lives **only** in `project.json` — module folders never carry
  one. A vendored module's format is its host project's; a bare module
  folder opened standalone is wrapped in a workbench project
  (starter-project seam) and assumed current — cross-version module
  *sharing* keeps the alpha posture: version + refuse, never migrate.
- **Imported modules are copy-to-own** (the shadcn model): once vendored,
  the source is the user's — readable, editable, committed; provenance
  (R14), not the directory, records origin. `modules/` is Studio's
  default import target and pure convention — refs are explicit paths, so
  any folder with a `module.json` is a module wherever it sits. If
  import-by-reference ever exists, read-only dependency storage goes
  *outside* the project (shared library + cache), not into a hidden
  in-tree dir.
- `.lp/` is a project-folder concept; the device keeps its own filesystem
  conventions and needs only the panel-state *data*, not the layout.
- `panel.json` shape (proposed, Q3): a versioned map of
  `scope-path / channel → { value, engaged }`. Scope paths are node
  paths, so vendoring/renames invalidate entries gracefully (unknown
  paths are dropped on load).
- Relative `node:` refs and file-relative artifact refs survive vendoring
  by construction — a module folder's internal wiring is
  location-independent.

> Status: the project.json/module.json split, the container-manifest
> format gate (missing manifest = hard refuse, format bumped to 3), and
> the split schemas landed 2026-08-01. `.lp/panel.json` arrives with the
> panel phases.

## 7. Bus vocabulary — under discovery

The set of standard channel names (`time`, `visual.out`, `audio.*`, …)
is **an unsolved problem, deliberately**. It cannot be designed ahead of a
real example corpus; it will be discovered by building modules and locked
down gradually **at the module boundary** — a module's public channels and
exports are where conventions become contracts. Until then: names above
are provisional conventions, not schema; new channels cost nothing (R3);
nothing in this document depends on the vocabulary's final shape.

One entry graduated 2026-08-04: **`time` is a product channel** like
`visual.out`/`control.out` — the clock publishes a `TimeProduct`
handle, and raw seconds never ride the bus (an `f32` slot bound to
`bus:time` warns loudly and runs on its default). Consumers declare
`phasor`/`seconds` uniform kinds instead of doing time arithmetic;
a phasor slot's binding names a *config channel* carrying a
`PhasorConfig` (period only when driven). There is no raw-seconds
vocabulary to standardize anymore
(ADR 2026-08-04-time-is-a-product).

Known vocabulary pressure beyond scalars: **touch/gesture sets** (E7 —
multi-point, per-touch identity; shape question tracked as panel.md
P-Q5). The old `phase` channel convention (panel.md P3) is superseded
by the phasor uniform kind above.

## 8. Provenance (field set settled — Q7)

`author`, `version`, `license`, `created` (ISO date). Optional on any
node and on `project.json`; skip-if-default; no semver semantics yet.
Copy-on-extract per R14.

> Status: landed 2026-08-01 as `ProvenanceDef` (module defs carry it;
> the container manifest carries the same four keys at its top level).
> Copy-on-extract mechanics arrive with the vendoring flows.

## 9. Open questions (G1 redline register)

- **Q3:** → specced as `panel.md` P8/P11 (wire ops, state file shape);
  ratify there.
- **Q7:** SETTLED (2026-08-01, implementation P3): the §8 four-field set
  as proposed (`author`/`version`/`license`/`created`); `description`/
  `homepage` deferred until a real need shows up in the registry/import
  flow.
- **Q10:** SETTLED (2026-08-01, implementation P2): format is a
  container-level concept. A module folder inside a project is gated by
  the project's container manifest; the loader never re-runs the gate for
  child artifacts (pinned by test). Bare-module-folder standalone opening
  keeps the §6 assume-current posture; import-time gating arrives with
  the registry/import flow.
- **Q11:** → moved to `panel.md` P6 (merge/tiebreak rules); ratify there.
- **Q12:** → resolved by the latch model: grabbing authored-driven
  channels is core behavior (`panel.md` P2/P5), not an increment.
- **Q13:** SETTLED — IMPLEMENTED (2026-08-03, P2; refined P6). The flag
  is deleted, not kept in parallel. `ShaderSlotDef.panel`,
  `SlotMeta.panel` and their DTO/agent surfaces are gone; a shader
  uniform reaches a panel exactly when its binding derives a
  `(scope, channel)` panel target, and the fixture face's brightness
  fader is that face's own named affordance. **Refinement (2026-08-03,
  P6): publicity is AUTHORED wiring only** — a binding the loader
  materialized from a slot's own `default_bind` is not publicity, so
  `bus:time` no longer materializes a time knob on every panel. Recorded
  as `docs/adr/2026-08-03-panel-visibility-is-derived.md`.
  **Second refinement (2026-08-03, amendment in the same ADR): one
  additive override** — a slot declaration may carry `panel = "show"`
  beside its `default_bind`, promoting that default wiring to publicity
  (fixture `brightness` → `bus:brightness`, the master fader). Absent
  hint = the refined rule unchanged; there is no `hide`.
- `panel.md` carries its own register (P-Q1–P-Q5: slew defaults,
  three-state affordance requirement, state-file versioning/flush,
  clear-all vs sink scopes, touch-set value shape).

## 10. Future work register

Not open *questions* — settled directions with no home yet. Each is
either a named spike, a registered defect/debt entry, or a design pass
somebody owes. Recorded 2026-08-03 at the close of
`planning/2026-08-03-1021-modules-vision-push` unless noted.

**Design passes owed**

- **LFO node.** Registered 2026-08-04 at the TimeProduct G1 (D11): the
  panel exposes a phasor's *period only*; wanting waveform, phase
  offset, or free-running modulation on a panel is answered by a
  dedicated LFO node publishing a config/value channel — never by
  widening the panel contract. No owner yet.

- **Wiring-drawer redesign.** DONE 2026-08-04 — the flow view
  (writers → value box → readers, arrowed wires, tone-matched colors,
  child-scope readers listed, stacked tree layout in narrow containers)
  shipped from the wiring-UI spike (`spikes/wiring-ui/`,
  `docs/adr/2026-08-04-wiring-flow-and-panel-settings.md`). Still open
  from that pass: the E3 pick gesture (§5) and the `control.out`
  channel rename (§7 vocabulary).
- **CONTROLS-vs-PANEL nomenclature.** DONE 2026-08-04 — `panel` is the
  module card's tinted ▶ section (what play mode renders); leaf cards
  say `settings` (R8 naming note, same ADR). The `control.out` product
  channel keeps its name until §7 vocabulary lock-down.
- **Auto-naming.** Manually naming nodes is a tax the node type usually
  pays for you. A node/module should present a good name with nothing
  authored — derived from kind, role, or position, with an authored
  label overriding. The root card shipped only the minimal fix (it
  wears the project's manifest display name).
- **What a module's canonical self-portrait is.** The hero draws the
  scope's resolved `visual.out` (live beats black). Yona leans
  `control.out` — the fixture view — with 3D mapping renders later.
- **Driving time from a panel.** With `default_bind` wiring no longer
  publicity (Q13 refinement), `bus:time` has no panel presence at all.
  The sanctioned answer should be the CLOCK's own transport/scrub
  controls published to the panel, not a knob materialized from a
  default binding. See `docs/debt/clock-transport-has-no-transport-ui.md`.
- **Playlist edit-vs-play, and entry progress.** A playlist card is both
  an authoring surface and a live transport; the two readings collide,
  and an entry's progress has no presentation. Spun off at GV2.
- **Parenthood rules.** Add-node should only offer kinds that make sense
  at the site — one `can_attach(parent_kind, child_kind)` legality
  function shared by the picker, drag-reparent, and wrap. Belongs with
  `planning/2026-08-03-1515-module-authoring-ops`.
- **Authored panel layouts.** A curated promoted-control list per module
  (the closed #218 spike's `controls{}`), as an additive override on the
  derived default. See the rejected-alternatives section of
  `docs/adr/2026-08-03-panel-visibility-is-derived.md`.
- **Data-driven playlist activation** (R2.2 carve-out), an **input
  design doc** (R13's external sources), **vocabulary lock-down** (§7),
  and a **module registry** (§6 import/vendor flows) — carried from the
  predecessor plan's register, unchanged.

**Known gaps in what shipped**

- **A kind with no face publishes no controls.** Module panels are
  assembled from face controls, so a `ComputeShader`'s bound uniforms
  reach the wiring drawer but never a knob — `examples/meteor` publishes
  `speed` and `count` as channels with no control above them. Either
  compute shaders grow a face, or panel assembly stops depending on one.
- ~~**Authored source bindings on non-hand-listed slots are dropped
  silently**~~ — FIXED 2026-08-03 (shape-driven registration, loud
  unknown-key errors on closed-namespace kinds; see the defect doc's
  resolution note). The fixture fader is now not merely publishable but
  default-bound to `bus:brightness` with `panel = "show"` (Q13's second
  refinement).
- **Sims do not restore panel state** (settled D-B: device-first,
  deliberately). Persistent sims are the follow-up, not a bug — do not
  "fix" a sim that fails to restore.
- **Panel-state serde flash cost** — parked by decision (Q3 of the
  vision push); `docs/debt/panel-state-serde-flash-cost.md`.
- **G4 hardware walk is still owed.** Decoupled from merging (DY2,
  opportunistic); materials at
  `planning/2026-08-01-1003-modules-impl-roadmap/g4-hardware-walk.md`.
