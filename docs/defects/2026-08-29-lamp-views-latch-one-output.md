---
status: fixed
found: 2026-08-29      # how: report (small-dome full-scale example, browser sim, after PR #460)
fixed: this change
area: lpa-studio-core (card_feed + preview_output_feed + output_frame_cache + project_controller scope hero)
class: config-masked-defect
related:
  # Lands with PR #460 (the full-scale small-dome example), where this
  # defect's symptom was first seen and where its "Remaining symptom"
  # section names this entry.
  - 2026-08-28-wire-load-skips-link-engine-state.md
---
# Every lamp view latched ONE output and dropped the rest

**Symptom** — the full-scale small-dome example (two Output nodes: "1" at
3,335 lamps, "Box 2" at 2,975) drew only output "1"'s lamps in the
module preview and the Simulator pane; Box 2's 2,975 lamps never
appeared anywhere, while the Outputs panel proved the wire fully alive
(2975/2975, live cells). The engine was exonerated by direct probe: at
unbounded budget `read_project_output_frame_probe` answers BOTH outputs'
full display layouts.

**Root cause** — every client that turns published frames into a lamp
picture reduced the probe's per-output answer to ONE output, each with
its own copy of the same rule:

- `CardFeedState` (device/sim card ▶, the Simulator pane) documented
  "a project can drive several outputs; the card shows one" and latched
  the first entry that published (`pick_entry`).
- `PreviewOutputFeed` (preview-host slots — gallery/home cards) had the
  identical latch.
- The module-face hero (`ProjectController::scope_output_frame`) had
  the multi-output `OutputFrameCache` in hand and reduced it with
  `find_map` — first output child with a drawable frame wins.

The rule was invisible because every project to date drove one output:
the reduction and the whole answer coincided until the first two-output
project (the small dome) arrived. The classic config-masked shape — no
test could falsify "show one output" while one output was all there was.

**Fix** — compose, don't pick. `OutputFrameCache::composed_frame`
builds one picture from many outputs: buffers concatenate in node
order, each part's spans and lamps rebase onto its stretch, geometries
overlay (the same shared-mapping-document-space property the engine's
own `merge_fragment_display_layouts` already relies on WITHIN one
wire). One part passes through untouched; a cached composite keeps the
`Rc` identities the lamp renderer repaints by (fresh bytes per new
frame, stable layout while geometry stands). `CardFeedState` and
`PreviewOutputFeed` now fold every probe entry through an embedded
`OutputFrameCache` and expose the composed frame; the module hero
composes all output children. The read gate is the cache's shared one
(`Always` while any output lacks geometry, `IfChanged` at the minimum
known revision, `None` only when all refused).

**Regression coverage** —
- `output_frame_cache::two_outputs_compose_into_one_picture_with_rebased_lamps`
  (plus Rc-stability, single-part passthrough, and layout-less-part
  tests beside it),
- `card_feed::every_published_output_joins_the_composed_picture`,
- `preview_output_feed::every_published_output_joins_the_composed_picture`,
- end to end against a real engine and link:
  `studio_link_e2e_tests::a_two_output_project_feeds_the_card_both_wires_composed`
  (one fixture patched across two outputs by port counts; the card's
  frame carries all 231 lamps).

**Lesson** — a deliberate simplification ("a card shows one X") is a
config-masked defect waiting for the first project shaped otherwise,
and it multiplies: three compositor seams each carried a private copy
of the same reduction, so no single fix could have healed the symptom.
When a probe answers a LIST, every consumer that renders "the picture"
must either compose the list or point at shared code that does — a
consumer that indexes the list is making a product decision, and it
should be able to say why. The engine-side merge doc now has its twin:
outputs cut from one project share the mapping document's normalized
space, which is what makes client-side overlay composition sound.
