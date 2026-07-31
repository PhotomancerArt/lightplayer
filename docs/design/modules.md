# Modules, Buses, and Panels

This document is the single source of truth for the **module container
model**: what a module is, how bus scoping works, how slots become public,
and how the panel — the end-user control surface — derives from the bus. It
supersedes the direction of the composite-effects spike (PR #218) and, at
implementation time, its two ADRs (`2026-07-28-scoped-buses.md`,
`2026-07-28-effects-are-projects.md`, both on the spike branch) — the
spike's writer-shadowing model survives here; its promoted-control aliasing
and transient scope implementation do not.

> **Status: DRAFT — awaiting ratification** (gate G1 of
> `planning/2026-07-31-1002-modules-buses-panels`).
>
> **Posture.** Alpha: no wire or artifact compatibility obligations; the
> rename below is one-shot with no migration. The architecture is required
> to mirror the *user model* — a module is one idea (a bus holder with a
> public face) presented one way at every nesting depth. Where this
> document and an implementation convenience disagree, this document wins.
>
> **Related:** `docs/adr/2026-07-26-node-card-faces.md` (face grammar),
> `docs/adr/2026-07-09-declarative-default-bindings.md` (default binds),
> `docs/adr/2026-07-27-node-authoring-operations.md` (create/vendor seam).

## 1. Concepts

| Term | Is | Is not |
|---|---|---|
| **Project** | The *container*: a folder with identity, provenance, history, and format. Not a node. What you open in Studio. | A node kind; a scope; anything the engine resolves against |
| **Module** | The container *node kind* (replaces the `project` node kind). Owns a bus scope, a panel, and child nodes. The root module is an ordinary module that happens to be at the root. | Special at the root; a separate "composite"/"effect" structure |
| **Effect** | A *category* of module (gallery/UI copy): a module authored to be dropped into other projects. | A distinct node kind or schema |
| **Scope** | The bus namespace a module introduces around its children. | Global; a display prefix |
| **Channel** | A named value stream within a scope. Exists iff some binding names it. | A declared registry entry; a place values are stored |
| **Public slot** | A slot with a bus binding. Its channel appears in its scope. | A slot copied, aliased, or mirrored anywhere |
| **Panel** | A *presentation* of a scope's channel list. The root module's panel is the end-user surface. | A place values live; a dataflow construct |
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

## 3. Bus rules (normative)

### R1 — Modules introduce scopes, structurally

Every module node introduces a scope enclosing its children. The root
module's scope is the root scope — root is *not otherwise special*. Scope
introduction is a property of the module kind, not the invocation site.

Scope identity is **structural engine state**: after load, the engine can
answer "which scope contains node X" and "which scope does module M
introduce" (the spike's throwaway loader side-table is explicitly
rejected). The scope is engine-owned runtime structure hung off the module
node; authored artifacts never name scopes.

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
   an inactive sink child's channels (the spike violated this and every
   inactive playlist entry rendered every frame; the fix must live in the
   model, not in the probe).

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
public input is an *invitation*, not an error. This replaces the spike's
"resolve to root, surface unfilled".

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

### R8 — The panel is a view of the scope's channels

The panel of a scope **is its channel list, presented**. No dataflow
construct exists behind a panel; nothing is promoted between levels.

- A module's face renders its own scope's panel.
- An enclosing module's panel additionally presents each child module's
  panel as a **nested group** (presentation recursion — the same shape as
  the card grammar). Two embedded instances of the same effect present
  two independent groups.
- The **root module's face is the end-user panel**. Play mode is "render
  only the root module's face".
- Sink scopes surface only per R2 (through the isolating node's face).
- Presentation is kind-dependent: scalar channels render as knobs/faders,
  bools as toggles, visuals as preview tiles, streams as readouts.
  Channels driven by authored writers (an LFO, a clock) render as live
  readouts that can be *grabbed* (R11).

### R9 — Control meta follows the binding

A panel control's display meta (label, unit, min/max, step, widget)
derives from the slot(s) **currently bound** to the channel in that scope
— it is re-derived whenever bindings change. This is what makes E2 work:
when a playlist switches entries, the panel control's range switches with
it, because a different slot is now behind the channel.

Merge rule when several slots in one scope bind the same channel with
conflicting meta: **numeric ranges union (widest wins)**; on label/unit
conflict the channel name wins. A module-level authored meta override
(label/unit/min/max on the module node, per channel) beats derivation —
this is the curation escape hatch, and the only module-side declaration
in the model. It carries no value (that would recreate the alias problem
the spike had).

### R10 — Panel state: lazy, stateful, unauthored runtime writers

Panel controls write through **runtime writer state**, per
`(scope, channel)`:

- **Unauthored.** Touching a control never edits an artifact and never
  dirties the project. Authored artifacts define *defaults and wiring*;
  panel state is live performance state.
- **Lazy.** The writer materializes on **first touch**, in the scope
  where the control was touched. Untouched channels have no panel writer
  — this is load-bearing: if every public slot self-shadowed, an outer
  scope could never drive an inner channel (R5 would always stop at the
  inner scope) and inheritance would be dead on arrival.
- **Stateful.** A panel writer is a data *source*, not a stored scalar —
  it may integrate (the phasor knob: changing speed without phase
  discontinuity), which is why it lives engine-side, identically on sim
  and device, and Studio talks to it via runtime commands (the
  playlist-activate precedent: a poke, nothing staged).

### R11 — Precedence: an engaged panel writer wins its scope

Within a scope, an **engaged** panel writer takes precedence over
authored writers for the same channel, until reset — grabbing a knob
overrides the LFO driving it. Across scopes, plain R5 applies: an engaged
writer in an inner scope shadows outer writers for that subtree.
Corollary: **touching a control detaches that scope from outer control**
until reset — the UI must show engaged/overridden state distinctly from
inheriting state.

### R12 — Reset

Reset removes panel writers — per control, per module (scope), or whole
panel — restoring authored / inherited / default resolution (R5/R6) and
clearing the corresponding persisted entries (R13).

### R13 — Persistence

Panel state persists by default in `state.json` beside the project —
never in authored artifacts. Writes are **throttled (≥ ~10 s apart)** for
flash preservation on device. Auto-save is on by default with a user
toggle; reset clears persisted entries.

The motivating case (verbatim requirement): *4 a.m., Burning Man, LED
scarf dimmed from a phone. Unplug for a minute, replug — it must come
back dim, not blinding. Next night, brighter conditions — connect, hit
reset.* Persist by default; throttle writes; make reset one obvious
gesture.

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

### E5 — Depth 2 (module inside a module) — the spike's latent-bug case

```text
H (root module): clock, …
└─ M_outer (module)
   ├─ M_inner (plasma module)      — publishes visual.out into Scope(M_outer)  (R7)
   └─ compute C   vis ← bus:visual.out   (e.g. analyzes the visual)
```

- `C.vis` resolves: Scope(M_outer) **has** a `visual.out` writer —
  M_inner's publish — so it resolves to the **sibling effect's visual**,
  never walking to root (R5). The spike got this wrong (its writer table
  omitted module publishes; depth 1 worked only because the fallback also
  landed on root). **Implementation must pin this exact shape with a
  test.**
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

## 5. UI corollaries (spike M2 owns the shape)

One face, three zoom levels: the **effect author** works inside the module
(children expanded); the **artist** sees the module face as a card
(preview + panel group); the **end user** sees the root module's face
alone (play mode). Consequences the UX spike must exercise: the root
module returns to the node area as the single top-level card (flat-root
reversal — the root now *does* something); the sidebar bus pane dissolves
into the module face (bus-as-controls) plus a wiring drawer
(bus-as-writers/readers, today's pane content); engaged-vs-inheriting
state and the reset gesture (R11/R12); the drop-time contention pick (E3).

## 6. File layout

```text
my-project/
├─ project.json          # container manifest — NOT a node:
│                        #   format, uid, name, provenance (§8), created
├─ module.json           # root module node: nodes{}, bindings, exports,
│                        #   per-channel meta overrides (R9), optional provenance
├─ shader.json …         # child nodes, one file per node (unchanged)
├─ modules/
│  └─ plasma/            # a vendored module: a module folder, nothing more
│     ├─ module.json     #   (no project.json, no format — see below)
│     └─ …
└─ state.json            # panel state (R13) — unauthored, throttled,
                         #   never dirty-tracked, safe to delete
```

- `format` lives **only** in `project.json`. Vendored module folders carry
  none — the spike's "format tolerated-but-ignored at non-root" rule is
  deleted. A vendored module's format is its host project's; a bare module
  folder opened standalone is wrapped in a workbench project
  (starter-project seam) and assumed current — cross-version module
  *sharing* keeps the alpha posture: version + refuse, never migrate.
- `state.json` shape (proposed, Q3): a versioned map of
  `scope-path / channel → { value, engaged }`. Scope paths are node
  paths, so vendoring/renames invalidate entries gracefully (unknown
  paths are dropped on load).
- Relative `node:` refs and file-relative artifact refs survive vendoring
  by construction (unchanged from the spike's analysis).

## 7. Bus vocabulary — under discovery

The set of standard channel names (`time`, `visual.out`, `audio.*`, …)
is **an unsolved problem, deliberately**. It cannot be designed ahead of a
real example corpus; it will be discovered by building modules and locked
down gradually **at the module boundary** — a module's public channels and
exports are where conventions become contracts. Until then: names above
are provisional conventions, not schema; new channels cost nothing (R3);
nothing in this document depends on the vocabulary's final shape.

## 8. Provenance (proposed field set — Q7)

`author`, `version`, `license`, `created` (ISO date). Optional on any
node and on `project.json`; skip-if-default; no semver semantics yet.
Copy-on-extract per R14. (The spike's `ProjectDef.author/version/license`
survive as this, relocated.)

## 9. Relationship to the #218 spike

| Spike piece | Fate |
|---|---|
| Writer-shadowing resolution (rules 3/4 of its ADR) | **Kept** → R4/R5 |
| Anonymous playlist-entry scopes | **Kept, promoted** → sink scopes as modeled property (R2) |
| Output mirror + non-root fallback publish | **Kept, generalized** → R7 (exports added) |
| `ScopedChannel` key through `QueryKey`/index/resolver | **Kept** as mechanism (resolver stays dumb) |
| Effect-face component, knob/panel widget reuse | **Kept** as M2 starting point |
| plasma / meteor content | **Kept** as future example modules |
| `PromotedControlDef` value-less aliasing | **Dropped** → R3 (binding is publicity); meta-override vocabulary survives in R9 |
| Transient `BusScopes` loader side-table | **Dropped** → R1 (structural scope state) |
| Scope flattened to display strings on the wire; entry scopes probe-filtered | **Dropped** → structured scope on the wire; R2 by construction |
| "Resolve to root, surface unfilled" | **Replaced** → R6 |
| Root special-casing (~7 sites, incl. Studio's `"visual.out"` string test) | **Deleted** — root is not special (R1) |

Defects the implementation must pin with tests: the depth-2 resolution
shape (E5); sink-scope no-demand (R2.2 — the inactive-entry render
regression); and note the spike branch carries a silently-disabled probe
test (duplicated `#[test]`) — do not inherit it.

## 10. Open questions (G1 redline register)

- **Q3 (spec proposed):** panel runtime commands `PanelWrite { scope,
  channel, value }` / `PanelReset { scope?, channel? }`; "touched" =
  first `PanelWrite`; state keyed `(scope-path, channel)`; `engaged`
  flag round-trips through `state.json`. Ratify or bend.
- **Q7:** provenance field set (§8) — enough? (`description`/`homepage`
  deferred?)
- **Q10:** bare-module-folder open assumes current format (§6) — fine for
  alpha?
- **Q11:** R9 tiebreak (channel name wins on label conflict) — fine, or
  prefer first-bound-slot-wins?
- **Q12:** grabbing an authored-driven channel (R8/R11 "grab the LFO")
  — in scope for the first implementation, or panel-writers-only-on-
  otherwise-unwritten-channels initially?
