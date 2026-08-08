# ADR: `ProjectKind` and pattern exports — the published unit is the workbench project

- **Status:** Accepted
- **Date:** 2026-08-07
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Amends:** `2026-08-05-project-target-metadata.md` (extends the same
  additive-container-field, no-format-bump posture to a second field pair)
- **Relates:** `2026-07-27-node-authoring-operations.md` (vendoring rides
  `CreateNode`'s byte-oriented create + picker source-dimension seam),
  `docs/design/modules.md` §1/§6/§8 (the container/module mitosis this
  builds on), `planning/lp2025/2026-08-07-1017-module-authoring-unit/vision.md`
  (D1–D17, the decision record this ADR transcribes)

## Context

A module is one container node kind (`docs/design/modules.md` §1), but
*authoring* one needs scaffolding — a clock, test fixtures, panel-driving
controls — that must never ship in what another project imports. Nothing
marked that boundary: every shipped example was a flat root module, §6's
`effect/` sub-module and `modules/` vendor-target patterns were doc-only,
and "this project publishes that module" had no representation anywhere.

The vision session that opened this track (`vision.md`) settled the shape
in detail before any code was written; this ADR is the durable record of
those decisions (D1–D17) plus the planning answers that pinned down their
mechanics. It does not re-decide anything — see that document for the
arguments, alternatives, and the prior-art survey behind each ruling.

## Decision

### The published unit is the whole workbench project

A module author works in an ordinary project; a designated sub-module
folder inside it is the thing another project can import. **Import**
vendors just the designated folder (copy-to-own, §6); **fork** (open as
project) takes everything, scaffolding included. Publish is not an
extraction step — a pack ships workbench projects as-is (minus the
framework-owned `.lp/`), and extraction happens client-side, at import
time, via the same vendoring operation an open project's own "Import
pattern…" gesture uses. One artifact kind serves the library, the cloud,
and the pack (D1–D3).

### `ProjectKind` on the container manifest — a role, never engine input

`project.json` gains two flat keys, `kind` and `exports`
(`lp-core/lpc-model/src/project/manifest.rs`):

```rust
pub enum ProjectKind {
    General,                       // kind absent — the unmarked default
    Pattern { exports: Vec<String> },
    Show,                           // declared, unbuilt (T4)
    Rig { exports: Vec<String> },   // declared, unbuilt (future hw-sharing plan)
}
```

`kind` is a closed four-value vocabulary string (`"general"` is never
authored — absence means it); `exports` is a flat list of module folder
names, meaningful only alongside the two library kinds, and the parser
rejects it alongside any other kind (including absent) as a loud error,
not a silent drop.

**The engine never reads `kind`.** It is a Studio-side concern only —
export lint (P2), the designation UI (P3), templates (P4), and import
(P5) all read it; the loader, resolver, and firmware do not branch on it
anywhere (D14). This mirrors the container/module mitosis itself: "what
this workspace publishes" is a *workspace* concern, not a technical-spec
one, so it lives beside `author`/`license`/`target` rather than on the
root module node.

`General` is the unmarked sandbox default every project starts as; the
first Export gesture on a folder sub-module offers the upgrade to
`Pattern`, and un-exporting the last folder reverts it (D14, P3).

### Flat-key encoding, no format bump

`kind`/`exports` are plain top-level `project.json` keys, resolved into
the `ProjectKind` enum by `ProjectManifest::project_kind()` — the manifest
itself keeps the raw strings (`kind_raw`/`exports_raw`) so an unresolved
manifest still round-trips byte-identically through the hand-rolled
writer. `PROJECT_FORMAT_VERSION` stays **5**.

This is the same call `2026-08-05-project-target-metadata.md` made for
`target`, for the identical reason: `ProjectManifest::read_json` is a
hand-rolled streaming reader that already refuses unknown top-level keys
outright. That closed-vocabulary strictness — not the format number — is
what turns an old build's encounter with `kind`/`exports` into a loud,
immediate parse error instead of a silent drop or misinterpretation. The
fields are purely additive, change the meaning of nothing that already
parses, and the engine never reads them; a bump would buy nothing beyond
what the parser's strictness already guarantees, at the cost of touching
every fixture project in the repo. (Contrast the actual format-5 bump,
same version history entry: `bus:time` becoming a time product changed
what an *already-valid* field means — the case a bump exists for.)

### Pattern naming, and why "effect" was not reused

The unit is named **pattern** (D15), chosen over the more reflexive
"effect" by a prior-art split surveyed across the field:

| Lineage | Examples | What the word names |
|---|---|---|
| Built-in list | WLED, xLights, LedFx | A fixed catalog the *product* ships, browsed by category |
| Authored + shared unit | Pixelblaze **patterns**, Shadertoy shaders, Processing sketches, synth patches | The thing *users* author, publish, and import from each other |

"Effect" belongs to the first lineage — it is gallery/UI copy for a
*category* of module (`docs/design/modules.md` §1's existing Effect row),
not a name for the authored-and-shared unit this track adds. Reusing it
for both would collide the browse-a-fixed-list reading with the
author-and-import reading this whole track exists to build. Pixelblaze is
this project's stated niche, which settled the tie-break. "Effect" stays
reserved for a future *modifier-of-patterns* concept (e.g. a color-cycle
or strobe layered over a pattern); pack copy is free to say "ports of the
classic effects" without contradiction. Three altitudes now have three
distinct words: **module** (the node kind), **pattern** (the project
kind), **modpack** (a collection of patterns).

### Exports live only in the library variants; modules stay untyped

`exports` is a field on `Pattern`/`Rig` only — an enum, not an optional
field on every variant — so "a `General` project has exports" or "a
`Show` exports modules" are unrepresentable states rather than states the
code has to reject at runtime (D14, D16).

Modules themselves remain untyped containers: there is no `rig`-kind
module or `show`-kind module, only categories in the same sense §1
already treats "effect" — a description of intended use, never a schema.
Hardware data stays node-level (fixture/output nodes) inside rig-*shaped*
modules; the manifest's `board` field — proposed once before, then
reversed out of the format — stays dead. The rig *concept* dissolves the
need for a container-level board field rather than reviving one.

**Exports are typed transitively**, not tagged per-module: a pattern
project's exports are patterns, a rig project's exports are rigs, because
the exporting project's own `ProjectKind` already says which. A pack
entry therefore needs no separate per-module type tag, and browse
dimensionality (1D/2D/…) derives from export *content* (a future space
declaration on the module), not from a kind label anyone would have to
keep in sync. The accepted consequence: one project cannot export both a
pattern and a rig — that split is two projects, not one project with
mixed exports.

### Designation lives in the module's own detail popup

The export gesture ("Export from `<module name>`") is a toggle inside the
module's own `NodeDetailPopover` (`node_detail_popover.rs`), beside its
Space/Provenance sections, wearing a new `DetailSectionTint::Export`
(sage — D11; never violet/bound or green/accent, the studio convention
for those hues elsewhere). The exporting state is not permanent chrome:
it appears **reactively**, the first time any child is exported, and
shows up as structure rather than a summary — the workspace child column
splits into an `exports` section (sage header) ahead of a `rig` section
holding everything that stays home, with each exported child wearing a
sage chip (D12; the G1 gate replaced an earlier root-card summary rail
with this split — the rail restated what the grouping now shows). A
device-backed session disables the toggle with an inline reason —
designation is a library-side manifest patch, and a device has no
manifest to patch into.

Lint findings (escaping file refs and folder-shape errors as static
findings, sibling-feed-only consumers as a graph finding, missing
provenance as a warning — vision D5/D6/D8) render as inline rows in the
same popup section rather than a separate review sheet or confirm dialog;
the aggregate also surfaces as a preamble row under the exports section
header, and each affected child's export chip takes the finding's tint.

### Vendoring rides the existing `CreateNode` op

Import does not introduce a new wire operation. It collects an export
folder's files, stamps R14 provenance (the source project's attribution,
only if the folder carries none of its own) through the canonical def
writer — never by splicing JSON text — and re-roots it under the
importing project's `modules/<name>/` via one `CreateNode { file, body,
assets, attach }` call, exactly the byte-oriented create seam
`2026-07-27-node-authoring-operations.md` built and explicitly reserved
for exactly this: "Import nodes from examples/projects" was that ADR's
first-listed follow-up, landing here as the picker's designed-but-unbuilt
*source* dimension gaining an "Import pattern…" entry. No wire-protocol
bump was needed. The vendored copy's internal refs are file-relative and
untouched by the move, so they resolve identically in the new location
by construction (§6) — nothing here rewrites a path, which is also why a
folder that resolved in its home project is guaranteed to still resolve
in the importer's.

### Soft module references and dependency resolution: a certified non-goal

A family workbench (one shared rig, several sibling exports — D4, modeled
now because plural exports are the right ergonomics for porting many WLED
effects without duplicating a rig per effect) can want a common module
leaned on by several exports. Two ways that dependency could leak:

- **File refs escaping the folder** — this is already an error, caught by
  the static lint half, because refs are file-relative and location-
  independent by construction; an escaping ref simply does not resolve
  after vendoring.
- **Bus feeds from a non-exported sibling** — silent, not loud: vendoring
  severs the feed, and the imported module keeps running on its own
  authored defaults (R6), just *looking* wrong rather than erroring. The
  graph half of the lint catches this statically inside the workbench,
  before it ever ships.

Round one's sharing contract is therefore: **no shared libraries.**
Sharing inside a family is duplication (nested copy) or accepted graceful
degradation, never a by-reference dependency. A richer future is sketched
and deliberately not built: a **soft module reference** — a link inside
an export folder resolved and bundled into the copy at import time,
fancy cases disallowed — pointing at the same import-closure-by-copy
model the shadcn analogy already uses for the whole vendoring approach
(`add card` pulls in `button` as owned source, not a linked package).
This is recorded here, per D7, as a **certified non-goal for day one**:
nothing in the current layout blocks building it later, since a shared
module is just another module path inside the same shipped project and
pack entries can grow dependency edges without touching the artifact
format — but it is not part of this track's scope, and should not be
treated as an oversight if a future reviewer notices the gap.

### Rig + show composition — ratified direction, no work done

D17 records a ratified *direction*, not a build: an idiomatic show
project composes as rig + show sub-modules — "you play the show on the
rig." This needs no new primitives, because it is exactly R5/R7 bus
semantics as already shipped: the show module publishes `visual.out`,
the rig's fixtures consume it. Two rigs playing one show (e.g. the same
show module read by two different fixture rigs) is two readers on one
channel; two rig+show container modules are two independent scopes (E5).
`Show`/`Rig` are declared in the `ProjectKind` enum today with no
behavior beyond the parse arm — Show's UI is T4's, Rig's is a future
hardware-sharing plan's.

### Existing flat examples: opportunistic restructuring (Q4)

Every shipped example predates this track and is a flat root module with
no designated export folder. They are **not** migrated as part of this
work, and there is no scheduled big-bang migration. Each restructures
into the workbench + `effect/` shape opportunistically, as it enters a
pack (T3 content work) — the same posture the format-version history
already takes toward optional additive fields: old content keeps working
un-upgraded until something gives it a reason to change.

## Consequences

- `project.json` accepts `kind`/`exports`; `schemas/project.schema.json`
  declares them; the conformance walk over `examples/`/`projects/test/`
  is untouched since both fields are optional and no shipped fixture
  authors them yet.
- A pattern/rig project's `exports` list is the only place export
  membership is recorded; nothing on the module itself says "I am
  exported" except by virtue of being named in its container's list.
- The export lint (static half in `lpc-model`, graph half in
  `lpa-studio-core`) is pure and extraction-ready — T3's pack CI can lift
  either half into a shared crate later without a rewrite.
- Import always lands under `modules/<name>/`, never at the project root,
  keeping "what I wrote" and "what I imported" legible in the file tree
  even though both are equally the user's to edit once vendored.
- **Deviation from the round-one plan, recorded for honesty:** the
  card-side "New project from this…" gesture (P5) composes its result as
  P4's 1D pattern template with the *template's* `effect/` folder name
  kept, rather than renamed to the source export's own folder name. The
  new project's manifest therefore always reads `exports: ["effect"]`
  regardless of what the source export was called, and the template's
  `module.json` (which references `./effect/module.json`) needs no
  rewriting. This is documented, not accidental — see
  `lp-app/lpa-studio-core/src/app/home/pattern_from_export.rs`'s module
  doc — and is exactly the kind of thing D14/D16's transitive typing
  anticipated staying simple: the folder *name* was never semantic.
- Show/Rig variants exist in the type system today with zero behavior;
  a reviewer encountering an unreachable match arm for either is seeing
  intentional day-one scope, not dead code.

## Alternatives Considered

- **"Effect" as the unit name.** Rejected by the prior-art split above:
  the word is already spoken for as a *category* of module (§1) and as
  the built-in-list lineage's name across the field; reusing it for the
  authored-and-shared unit would make "browse effects" and "author an
  effect" collide in exactly the way separate words prevent.
- **`kind` on the module, not the project.** Rejected: caught mid-session
  as the same slippage that produced the once-reversed `project.json`
  `board` field — a workspace-role concept leaking onto the
  technical-spec node. Kind answers "what is this *workspace* for,"
  which mitosis already assigns to the container.
- **`exports` as a field on every `ProjectKind` variant** (always present,
  empty for non-library kinds). Rejected: makes "a `Show` project has
  exports" a representable-but-meaningless state instead of ruling it out
  by construction; the enum-variant encoding costs nothing extra to parse
  or write.
- **Publish-time extraction** (the pack ships stripped module folders,
  not whole projects). Rejected in the vision session, before this ADR:
  bespoke rigs are exactly the ones a template cannot regenerate,
  outside EUPL-1.2 contributors need the rig they are modifying to be
  versioned and runnable, and size is a non-argument at the KB scale
  these projects run (content-addressed cloud sync dedupes further).
- **A dedicated confirm sheet on first export** (given the licensing
  weight of publishing). Rejected for round one in favor of the inline
  lint rows already in the popup (D12/Q5) — revisit at T3's licensing
  gate if the lighter treatment proves insufficient once real outside
  contributors show up.

## Follow-ups

- T3 (WLED-compat pack) consumes this layout directly: a pack entry is
  `(project ref, module path, browse metadata)`, and it owns opportunistic
  example restructuring (Q4) as content work, not a migration.
- T2 (space/dimension model) is the future home of per-module space
  declarations that let browse dimensionality derive from export content
  rather than a manual tag, as D16 anticipates.
- T4 owns Show's UI, the Explore/pack browse chrome, and the in-card
  stateful consume verbs (Add to project / New project from this /
  Open workbench) sketched in the vision session's spike but not built
  here — round one shipped only "New project from this…" as a plain menu
  row and the add-node picker's import source.
- A future hardware-sharing plan owns `Rig`'s real behavior; today it is
  a declared, unbuilt variant.
- Soft module references / import-closure-by-copy dependency resolution
  (D7) remain an explicitly parked design, not a scheduled phase.
