# Walk-up patching — the assignment flow

Status: draft (design doc, distilled from lp2014 + Yona's account,
2026-08-11). Companion to the unified editor-shell spike
(`spikes/unified-editor-shell/index.html`) and the slice-2 ADR
(`../adr/2026-08-10-object-ids-output-names-and-scatter.md`).

This is one of the most important flows in LightPlayer and one of the
trickiest to get right. lp2014's version represents **years of
tweaking under the real conditions**: late at night at a festival, on
a time crunch, with an audience that starts reacting the moment the
lights come on. lp2014 called this whole activity "mapping"; in
LP2025's D45 vocabulary it is **patching** (assigning objects to
places on output ports). ~90% of lp2014 clicks were this flow.

## The scenario

A person sits down with the app (tablet or laptop) next to the art
piece. The fixture's objects exist (the mapping documents are
authored); the piece is wired to output boxes; **nobody knows which
jack feeds which panel**. The goal: assign every object of every
fixture a place in the outputs, guided by the actual lights.

## The loop (lp2014's shape, verified against its source)

1. **Start** with unpatched objects and free ports — or clear the
   existing patch first.
2. **Select a group of lamps on a port.** The output side is
   subdivided into selectable chunks: existing assignments become
   cells, and the gaps are chopped into groups of an
   *approximate expected object size* (lp2014:
   `defaultChannelGroupSize`, a user-set number). Selecting a group
   drives a **debug pattern on the hardware** — lp2014 pulsed those
   lamps white at low brightness (`debugChannels` on the output
   device). It must be a *group*, not a single lamp: one lamp is
   genuinely hard to spot on a large piece.
3. **The user looks at the piece**, sees which object is pulsing, and
   **clicks that object** (in the canvas or the tree). That click
   creates the assignment — lp2014's `selectShape` auto-mapped on
   click whenever a channel group was selected. The assignment is
   **not final**: the object stays selected in an adjust state.
4. **Adjust.** The object's run shows direction and phase markers —
   lp2014 drew an arrow at the first lamp and a square at the last
   (flipped when reversed) and a moving chase from start to end.
   Controls (all also hotkeys):
   - flip (`r`)
   - rotate coarse (`;` / `'` — lp2014: ±0.3 of the object,
     direction-aware so the keys feel physical regardless of flip)
   - rotate fine (`k` / `l` — one lamp)
   - shift the window on the wire (`[` / `]` — one lamp, adjusting
     length to compensate)
   - grow/shrink the window (`-` / `=`)
   - lp2014 also had a **fraction strip**: the object's lamps in a
     row with ½ ⅓ ¼ tick marks, click a lamp to set the rotation
     phase directly, live colours in the cells.
5. **Finish** (`m` in lp2014, or a button): the assignment commits
   and selection **auto-advances to the next free chunk** on the
   output — sometimes the next port, sometimes the next gap; the
   group-size setting is what makes this guess right.
6. **Repeat until all the lights are on.**

### The guide invariant

The fixture display must always show **which objects are patched and
which are not** — the person navigates the physical piece by it:
"I just did that panel; the one pulsing now is one over, one down."
Live product data on the objects is ideal; a debug mode with a
unique colour per object also works (LP2025 already has per-object
colours). lp2014 exported live `lastFrameRgb` per shape and live
`rgbData` per output lamp and rendered both in the UI — the strips
were alive.

## Everything is editable

Selecting any existing assignment opens the same adjust state:

- **fixture-side params**: flip, rotation offset, window size;
- **output-side params**: position on the wire (port + lamp);
- **unmap** (`u`);
- **manual map**: click the object *first*, then pick the port
  location (the reverse arrow of the same association);
- **swap** (`s`): mark one assignment, click another object — the
  two placements exchange. For "I got those two panels backwards"
  at 2am this is the difference between ten seconds and five
  minutes.

## Bulk operations (lp2014 had, keep in reach)

- Swap two whole ports; flip port groups within a connector
  (lp2014 `flipUniverseGroups` — the ribbon-cable pin-reversal fix).
- Rotate a whole symmetric structure by one sector (lp2014
  `rotateGeodesic` rotated dome layers by fifths and could shift
  every panel's phase at once). LP2025's declared-stride rotation
  verbs are the descendant.

## LP2025 mapping of the model

lp2014's `channelMapping.addressMap` was keyed by shape-group index
with exactly one `{universe, address, offset, reversed}` per group —
the model *could not* split an object. LP2025 equivalents:

| lp2014 | LP2025 |
| --- | --- |
| shape group | object instance (`/sector/2`) |
| universe:address | output name + port + wire lamp (`at.lamp`) |
| offset (0..1 float) | integer `offset`, stepped by declared stride |
| reversed | `reversed` flag |
| shapeWrapping ledCount override | window size (range sub-span) |
| debugChannels white pulse | Q27 pulse (chipped: task_2d2386cb) |
| debugGroupIndex persistence | patch selection |

## The open data question: can an object's mapping split?

Today's patch format 2 *permits* a split: multiple entries can name
the same path with different `range` sub-spans. lp2014 could not
split, and nobody missed it.

**Recommendation: one object = one contiguous wire window** at
object grain (rotation wrap-splitting *within* the window is
placement detail, not a split). Anything that genuinely needs two
windows becomes two objects (or a group), or drops to manual
range-grain entries — which stay as the escape hatch (the peach
already exercises them). What this buys:

- the UI atom stays "one chip / one cell per object instance";
- every verb (flip, rotate, shift, swap) has an unambiguous target;
- auto-advance and gap-chopping stay simple;
- the walk-up mental model ("that panel lives at IO14:31") stays
  true.

If ratified, the kernel should **refuse** path-duplicate entries at
resolve (like every other patch refusal: degrade and report), and
the editor never authors them.

## Scope posture

- Patching is done **fixture-by-fixture**: one fixture holds focus,
  hotkeys act within it; same for the focused output. Multi-select
  is worth *considering* later (a dome fixture spans 10 outputs) but
  is not v1. Symmetric structures may keep assignments
  sector-specific (5-way rotational group), because that is how the
  piece is actually wired.
- The assignment flow is a candidate for its **own implementation
  pass**, separate from (though designed with) the editor-shell
  unification — it is the trickiest UX in the product and deserves
  its own gate. This doc + the spike's round-4 section are its
  design record.
- Real-world validation target: an icosahedral-geodesic-sector
  object in a 5-way rotational group — the actual dome shape. (The
  full dome exceeds current firmware lamp budgets; the editor must
  still scale to it. "Friendly shaders, everywhere" — this editor
  is part of the *friendly*.)

## lp2014 weaknesses to not inherit

- **Single-fixture tunnel vision**: you could only see one fixture
  at a time — the motivation for LP2025's project-scope rails and
  conceptual arrangement canvas.
- Universe-addressed everything (512-channel DMX vocabulary) — D45
  killed this: port / lamp / output name.
- The approximate-group-size number was load-bearing for
  auto-advance; LP2025 can usually do better (object lamp counts
  are known from the mapping — chop gaps by the *next unmapped
  object's* size by default, keep the manual override).
