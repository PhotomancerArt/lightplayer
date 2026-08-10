# Output fragments and per-fixture patch files

- Status: accepted
- Date: 2026-08-10
- Context: mapping & patching surface vision, slice 1 (planning dir
  `2026-08-09-0048-mapping-patching-surface`; design record
  `spikes/mapping-patching-surface/index.html`). PR #405.

## Context

LightPlayer had a strictly one-fixture-per-output world: an output
consumed exactly one control product off `bus:control.out` and split
it across its wires; two producers on the channel was a resolve
error by design. Real installations need more: sections of one strip
running different patterns (a fixture *is* a WLED segment), dome
panels plugged into arbitrary jacks and fixed in software, doors
sharing an output with the panels they interrupt. lp2014 "supported"
this as an unvalidated race: fixtures hand-scattered absolute
`{universe, address}` entries and parallel-wrote one shared buffer.

## Decisions

### 1. Output fragments — a consumer-declared merge

`OutputDef::input` declares `merge = "fragments"`
(`SlotMerge::Fragments`). The resolver builds
`ResolvedRoute::MergeFragments` from every provider on the channel
(the same provider walk `ByKey` merges use); the output renders N
control products into disjoint sub-slices of its one sample buffer:
`(producer, offset, len, reversed, source-offset)`.

- The policy lives on the **consumer slot**, not the bus: control
  and visual products are indistinguishable at the binding layer
  (same well-known kind), so slot shape is the only clean
  discriminator. Visual channels keep single-producer semantics.
- Unpatched multi-fixture placement auto-flows in provider order
  (binding priority, then module node order) — "fixture order is
  wire order", the map2d object rule one level up.
- **Overlap = degrade and report.** Contested lamps render dark
  after the uncontested remainder renders; the output (which
  previously had no runtime status at all) reports an `Error`
  naming the contested lamp ranges; gaps report `Warn`. Never a
  frame-killing resolve error. Last-wins was rejected: it needs an
  arbitrary order and silently masks the exact mistake patching
  exists to catch.

### 2. The patch file — sparse anchors over auto-flow, per fixture

`{stem}.patch.json` beside the fixture's mapping
(`FixtureDef::patch: Unset | File`, `AssetContentType::FixturePatch`,
document schema owned by `lpc-mapping::patch`):

```jsonc
{ "format": 1,
  "entries": [
    { "range": { "start": 0,  "count": 22 }, "at": { "channel": 0 } },
    { "range": { "start": 22, "count": 22 }, "at": { "channel": 34 },
      "reversed": true } ] }
```

- Entries are **fixture-relative lamp ranges** anchored at wire lamp
  offsets — dimension-agnostic: identical bytes drive the 1D and 2D
  peach. Omitted `count` = to-the-end; lengths always derive from
  the mapping (anchor-and-reflow). Unanchored producers keep
  auto-flow (partial patching is first-class).
- Clearing the patch restores pure auto-flow; the mapping is
  untouched by construction — addresses were never in it.
- `at.output` and rotation `offset` are **reserved and refused**,
  not ignored: a rotation this build cannot apply would silently
  mis-light. Format is version-and-refuse (`format > 1` rejected
  whole), never migrated.
- An unreadable patch degrades to auto-flow with a fixture status;
  a dangling reference still fails load like a dangling mapping.
- Rejected alternatives: output-side channel tables (a dome fixture
  spans outputs — lp2014 grew `CompositeOutputDevice` to work
  around exactly this); lp2014's fixture-side absolute addressing
  (right side, but unvalidated and invisible — the race above);
  storing addresses in the mapping document (clearing a patch
  between installs must not touch the mapping).

### 3. Display: wire-space geometry has one source

Two latent assumptions surfaced the moment outputs had N producers,
and both are now rules:

- **Fixture-space geometry never crosses the wire boundary
  un-rebased.** The output latches its placement set (with a
  change-stamped revision — a patch edit re-cuts the wire without
  moving any mapping revision) and the frame probe serves the merged
  display layout, every producer's lamps rebased through its
  placement. Clients consume that; they do not stitch fixture
  layouts against wire-ordered samples. (The studio's over-budget
  package-file fallback still predates this rule; its replacement
  is planned separately — display-layout wire-fit plan.)
- **`Unsupported` is a permanent answer; transients must say
  `Omitted`.** An output that had not planned placements yet
  answered `Unsupported` once, and clients rightly stopped asking —
  latching a wrong synthesized picture forever. Momentary
  conditions answer `Omitted` (retryable), never `Unsupported`.

The wire frame now carries the placements themselves
(`WireOutputPlacement`, wireProto 17): auto-flow ordering belongs to
the resolver, so no client can re-derive the cut from project files.
The studio's patch bay renders the same placement cells from both
ends — output face in wire order, fixture face in shader order —
derived in one pass so the two views cannot drift.

### 4. Compat posture (interim)

Until the interactive patching slice locks the formats in: wire
proto bumps freely behind the existing device version gate (16→17
inside this one PR), and the patch document refuses newer formats
rather than migrating. Trunk-based; the in-repo examples are the
corpus and move in the same PR as any format change.

### 5. Scope semantics this relies on (recorded, not invented here)

"Visual products flow project-wide; control products stay within
their rig" is **emergent, not special-cased**: bus channels are
module-scoped, crossing scopes only by authored exports (the peach
submodules export `control.out`); reads inherit outward to the
nearest writer; the fragments merge is consumer-slot-declared. The
"control domain" of the two-rig archetype is a *descriptive*
grouping computed from authored wiring, enforced by validation, not
by product-type rules.

## Consequences

- A fixture is the segment primitive; N fixtures per output is the
  supported shape, validated instead of raced.
- The peach examples (`examples/peach-1d`, `examples/peach-2d`) are
  the living acceptance fixtures: same mapping bytes, same patch
  bytes, different declaration; docs page `the-peach` narrates it.
- Instance-grain patching, object-id-keyed entries (map2d format 3),
  rotation strides, multi-output scatter, and interactive editing
  are slice 2 (`docs/use-cases/2026-08-09-mini-dome.md` is its
  demand document).
- The A1 golden-buffer oracle (`output_control_samples_golden`)
  pins single-producer outputs byte-identical across this change
  and must never be regenerated to make a refactor pass.
