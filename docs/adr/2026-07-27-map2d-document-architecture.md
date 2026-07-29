# ADR: 2D mapping documents — opaque versioned asset + shared resolver crate

- **Status:** Accepted
- **Date:** 2026-07-27
- **Deciders:** Photomancer
- **Supersedes:** The load-time-only `SvgPath` mapping bridge
  (`docs/plans/2026-05-20-svg-path-fixture-mapping/`, explicitly "a narrow
  bridge for the FYeah sign, not a general SVG layout system") as the
  direction for fixture mapping authoring. The legacy `MappingConfig`
  variants remain functional during migration.
- **Superseded by:** None

## Context

2D LED mapping is a core product capability, and the existing authoring
surface was a temporary bridge: a strict SVG-subset importer resolving at
project load, plus slot-modeled `PathPoints`/`RingArray` variants that render
as unusable generic slot rows. An excalidraw-style mapping editor is planned
(standalone module first, then in-place editing on the fixture face), the
runtime already ships lamp geometry to Studio as `ControlLayout2d`, and the
editor's live preview must never disagree with what the device resolves.

Constraints: the device parses and resolves mappings on ESP32 at project
load (`no_std`, flash margin ~1 MB, sans-IO); Studio (`lpa-studio-core`) is
engine-free and reaches shared code only through model-adjacent crates; the
repo bans serde `tag`/`untagged`/`flatten` in the firmware dependency graph
(Content-machinery flash cost, `scripts/check-serde-content.sh`); Studio
asset bodies apply whole (10 KiB client budget).

## Decision

1. **The mapping is an opaque, format-versioned JSON document** (e.g.
   `fixture.map2d.json`) referenced from the fixture as an asset
   (`MappingConfig::Map2d { source }` in M2). Mapping internals are NOT
   modeled as slots: nothing binds to mapping internals, slot machinery
   taxes schema evolution (shape registration, schema dumps, editor hints),
   and the generic slot rendering of mapping trees is hostile UX. The slot
   layer owes only presence/absence (the face's empty-state signal). If
   something inside a mapping ever needs to be bindable/animatable, it
   becomes a runtime input on the fixture — never a reach into the document.
2. **One resolver, one crate: `lp-core/lpc-mapping`** (`no_std + alloc`,
   sans-IO, deps limited to serde/serde_json/libm). It owns the document
   schema, the deterministic resolver (document → ordered lamps), universe
   auto-flow addressing, aspect-fit, and the SVG-subset import as an explicit
   one-time conversion. Engine, device, and Studio all resolve through this
   crate, so editor previews are device-identical by construction.
3. **Wiring order is primary; addresses are derived.** Object order in the
   document is the physical daisy-chain; DMX-style `{universe, channel}`
   addresses auto-flow at 170 RGB lamps/universe. Manual patching, when it
   arrives, layers on top without changing wiring order.
4. **Dimensionality is an enum, not an abstraction.** `Map2d` is one variant
   with room for `Map1d`/`Map3d` later; the editor and this schema are
   deliberately 2D-only.
5. **Schema shape serves the firmware graph.** Shapes are externally tagged
   (`"shape": {"grid": {...}}`) — no `kind` field — keeping serde's Content
   machinery out of the device deserializer. `format` gates parsing (newer
   documents are rejected with a legible error; unknown fields are ignored,
   so additive evolution needs no bump). Dense geometry is plain JSON today
   with explicit room reserved for an additive packed-base64 representation
   (`points_packed`-style) if imported point-heavy documents ever threaten
   the whole-body asset budget; corpus tests enforce headroom.
6. **An authored `canvas` frames aspect-fit.** Import preserves the SVG
   viewBox as the document canvas so converted layouts render identically to
   the legacy bridge; editors can later adjust or clear it deliberately (the
   fyeah "small in the corner" layout becomes a user-editable framing choice
   rather than importer trivia).

## Alternatives considered

- **Slot-modeled mapping (status quo extended):** granular acks and bus
  bindability that nothing needs, at the cost of coupling schema evolution
  to the slot system and rendering mapping as slot-row soup. Rejected.
- **SVG as the source of truth with a WYSIWYG SVG editor:** keeps Illustrator
  interop but makes round-tripping edits through a fragile text convention
  (`path:N,count:N`) the core write path, and full SVG semantics (transforms,
  curves, nesting) a permanent liability. Rejected — SVG stays as import.
- **Internally-tagged schema (`"kind": "grid"`):** nicer flat JSON, but
  banned in the fw graph for measured flash cost. External tagging costs one
  nesting level.
- **Baked point lists as the authored form:** loses parametric editing
  (count/routing/corner changes become re-draws), which the UX spike showed
  is the whole value. `PointList` remains an escape hatch via import only.

## Consequences

- The resolver crate is the contract: engine (M2), read-only face views
  (M3), and the standalone editor (M4) all consume `resolve()` +
  `fit_points()` output; divergence between preview and device is
  structurally impossible rather than tested-for.
- Mapping edits ride the existing whole-body asset pipeline
  (`SetArtifactBody`), atomic at debounce boundaries — no new write path.
- The shared test corpus (button multi-ring, cat ears, 16×16 snake panel,
  the real fyeah sign at 219 lamps / 2 universes) lives in the crate and is
  the fixture set for engine tests, stories, and editor fixtures alike.
- Legacy variants (`PathPoints`/`RingArray`/`PointList`/`SvgPath`) become
  migration fodder; their removal is explicit follow-up debt after M5.
