# Object ids, output names, and cross-output scatter

- Status: accepted
- Date: 2026-08-10
- Context: mapping & patching surface vision, slice 2 (planning dir
  `2026-08-09-2332-mapping-patching-slice2`). PR #409. Builds on
  [output fragments and per-fixture patch files]
  (2026-08-10-output-fragments-and-patch-files.md).
- Supersedes: the patch-file schema section of the slice-1 ADR
  (format 1 remains readable; format 2 below is the authored form).

## Context

Slice 1 gave one output fragment-merging with sparse patch anchors,
but patch entries could only address flat lamp ranges on a single
implicit output. A real re-plug session (the mini-dome: five ceiling
sectors and three doors across two 5V boxes, jacks assigned by
whatever cable reached) needs entries that name *which object* went
*into which jack*, survive mapping edits, and scatter one fixture
across several outputs.

## Decisions

### 1. Sticky object ids and path identity (D46)

`map2d` format 3 gives every object a sticky kebab-case slug id
(`ensure_object_ids` slugifies the name once and uniquifies;
subsequent renames do NOT re-slug). Patch entries address objects by
**object-tree path** — `/sector/2` = third instance of the object
with id `sector` — never by lamp offset. Instance lamp boundaries
re-derive from the mapping at resolve time, so re-mapping a sector
never silently re-aims a patch. A path naming a missing or renamed
id degrades that entry to auto-flow with a fixture `patch_error`
naming the id; the frame never dies.

Format 3 also adds per-object `stride` (declared rotation step) and
the `polygon` shape (closed perimeter, wrap) — D44.

### 2. Patch format 2 — compact rows, output names, `at.lamp` (D45)

```jsonc
{ "format": 2,
  "outputs": ["1", "Box 2"],
  "entries": [
    ["/sector/1", 1, 0, "r"],
    ["/sector/2", 0, 0, "", 10],
    [[22], 0, 34, "r"]
  ] }
```

One row per line: `[from, out#, lamp, flags?, offset?]`. `from` is a
path or `[start, count?]` range (`[start]` = to end). `out#` indexes
the `outputs` header table; `-1` = the default output. Position on
the wire is always `lamp` — the word "channel" never means a lamp
position (D45: port / lamp / output name). No VLQ; JSON stays
hand-editable. **Minimal stamping** (D37/D38): documents declare the
lowest format that represents their content, so the peach's format-1
files stay byte-identical through any tool that touches them.

### 3. Output identity — authored names, registry at consume

`OutputDef` gains a `name` slot (`OutputName`, ≤24 printable ASCII).
Outputs self-register their authored name when they consume;
duplicate names error BOTH outputs; a patch row naming a
no-longer-present output degrades per-run with a fixture
`patch_error`. The registry's topology revision joins the fixture
patch-resolve cache key, so renames self-correct without a reload.

`OutputDef.channels` → `ports` (Q31): the D45 rename sweeps the
model, schemas (PROJECT_FORMAT_VERSION 9 → 10), and an
`lpa-upgrade` v9→v10 step (kind-gated key rename, re-runnable).

### 4. Scatter — output-discriminated placement (D40)

`control_patch_placement(product, consumer)` answers, per output:
`None` = "this producer auto-flows on the DEFAULT output only";
`Some(vec![])` = "patched, but nothing lands here" (no fragment, no
gap — a zero-run fixture on an output is not an error). The default
output is the first Fragments-consumer by NodeId, **fail-safe to
true** (A1): when the query cannot resolve, every output places the
producer, because dark lamps on a real installation are worse than a
doubled render.

### 5. Rotation is kernel-owned and honest about being fake

Wire lamp = `c + ((j' + k) mod N)`, `j'` reversed-first — reverse,
then rotate. Render uses `slice::rotate_right`; display placement
wrap-splits spans. Offsets step by the object's declared `stride`
(one sector-side, one door-side), which reads as "rotate by one
side" in tooling — physically re-anchoring the ring without
pretending the geometry moved.

## Consequences

- The mini-dome ships as `examples/mini-dome` with the as-built
  permutation `(1,5,4,3,2)` authored in patch files; the engine
  fragment sets are pinned byte-exact in `mini_dome_scatter.rs` and
  the golden output-sample digests.
- Patch edits round-trip: verbs write documents the text editor
  could have written; undo restores exact prior bytes.
- The G1 review ratified this machinery but **rejected the
  standalone patch-surface shell** (D36 full-page skeleton): the
  interim `/patch` view ships as scaffolding, and patching UI moves
  into the mapping editor (project tabs, mapping/patching modes,
  fixtures + outputs rails) via a UX spike and its own slice. The
  verb layer (`PatchVerbOp` DTOs through the project controller) is
  shell-agnostic by design and carries over.

## Alternatives Considered

- **Lamp-offset patch anchors** (slice-1 format continued): silently
  re-aim when the mapping changes — the exact failure patching
  exists to catch. Rejected.
- **Numeric output codes** in rows without a header table: compact
  but unreadable and unstable across output renames. The header
  table keeps rows short AND names authoritative.
- **Blocking writes on overlap while editing**: rejected; mid-edit
  overlap stays degrade-and-report (red cells), matching the
  runtime posture.
- **True geometric rotation** (transform the mapping): out of scope
  and dishonest for jack re-plugs; stride-stepped wire rotation
  matches what physically happened.

## Follow-ups

- Patching UI unification into the mapping editor (UX spike, then
  its own plan) — including renaming the editor's per-universe wire
  annotations to D45 vocabulary.
- HW-provider "channel" vocabulary (`OutputChannelHandle`,
  `engine_services`), mapping-editor universes toggle,
  `address_of`/`LAMPS_PER_UNIVERSE` — chipped separately.
- Sim/hardware pulse on patch selection (Q27) — chipped.
