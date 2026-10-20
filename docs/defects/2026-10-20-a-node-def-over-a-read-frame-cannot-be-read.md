---
status: open
found: 2026-10-20      # how: the drawing plan's emulated C6 measurement (P4): `lp-cli emu run --upload catalog/patterns/fluid-draw` stopped at "deploy was acked, but the deployed project failed to run"
area: lpc-shared `ProjectReadStreamSink` (one event per frame, 16 KiB), lpc-engine project read of a node's definition
class: budget-exhaustion
related:
  - docs/adr/2026-10-20-touch-sets-are-momentary-map-panel-writes.md
---
# A node definition bigger than a read frame cannot be read at all

**Symptom** — `lp-cli emu run --upload catalog/patterns/fluid-draw` (and so
`lp-cli upload` to a board) deployed the project, then failed its "is it
running?" read: the board answered `Serialization error: project-read event
exceeded frame budget of 16384 bytes`. The project itself ran.

**Cause** — a project read streams events in frames of at most 16 KiB (less
on a fragmented heap, `lpa_server::read_frame_budget`), and one event must
fit in one frame. A node's whole definition is one event (`slot_root`
`node.N.def`), and each declared shader slot costs it about a kilobyte in
that form. The draw shader's first version declared sixteen slots (eleven
inputs, five of them phasors for the idle choreography, plus three outputs),
so its definition read back as **19.7 KB**. `projects/test/fluid`'s compute,
eight slots, is 9.8 KB. Nothing warned the author: a host load and every
host test passed, and only a read through the board's own sink refused it.

**Fix so far (content, not the limit)** — `fluid-draw`'s shader now declares
ten slots (13.1 KB). The brush and idle time are shader constants, and the
friends move off three phasors, not seven. A guard test streams every
catalog pattern's full detail read through the board's sink at its default
budget: `lpc-engine/tests/catalog_patterns_read_in_one_frame_each.rs`
(largest today: `fluid-draw` 13.1 KB, then `linear-gradient` 8.3 KB).

**Still open** — the limit itself. An authored node with enough slots (a
user's shader, a big playlist) still cannot be read back from a board, and
Studio's editor would stop at the same refusal. Either split a definition
across events (slot by slot, as shapes are) or say so at save time in plain
words.
