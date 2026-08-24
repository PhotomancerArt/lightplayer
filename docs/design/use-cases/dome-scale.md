# Dome scale — tens of thousands of lamps, re-wired every build

Captured 2026-08-21 (Yona, walk-up patching G1).

## Vocabulary first

**THE Dome** is the 38-foot **Radiance Dome**: roughly **22,000 lamps**
across **190 panels**, driven by **10 PixLite Long Range** controllers.
When "the Dome" appears in a plan or a conversation, that is the piece
meant.

Zook's dome — the `examples/zook-dome` project — is **NOT** "the Dome".
It is a small dome, useful as a test piece and a mapping example, and
confusing the two has already cost time. Say "Zook's dome" when that is
what is meant.

Related pieces at this tier: the **TEDx Orb** (~30,000 lamps), art cars,
and anything else in LXStudio's territory.

## What makes this case its own thing

The pieces are **re-wired differently every build**. A dome goes up in a
field, gets patched, comes down, and next time the boxes sit somewhere
else and the runs are cut differently. The mapping (where a lamp *is* in
space) is stable; the patch (which jack feeds which panel) is thrown away
and re-made — which is exactly why they are two documents.

So **live mapping ergonomics ARE the point**. Not a nicety around the
edge of an editor: the thing being optimized. Every second in the loop is
multiplied by 190 panels, at night, with a crew waiting.

## The flow flag here

`manual`, always. **Never auto-mapped.** At this scale an auto-flowed
guess is not a helpful default — it is 22,000 lamps of plausible-looking
wrong, spread over ten boxes, with no way to tell what has actually been
answered for. Dark means "not yet"; that is the only tractable way to
patch a dome.

The rejected alternative is worth recording: per-object *tombstones*
(a note on each object saying "deliberately unmapped"). Wrong grain —
this is one fact about a fixture's installation, and per-object
bookkeeping for it is absurd when the fixture holds 190 of them.

## What this case asks of the product

- **Scale**: the Patching surface, the trees, and the strips have to stay
  usable at ~22k lamps and hundreds of objects. A per-lamp anything is
  suspect; a per-object anything is the budget.
- **Speed of the loop**: one key, one click, next — with the segment
  sizing itself and `m` walking the free space.
- **Bulk verbs** survive: swap two ports, shift a port, rotate a
  symmetric structure by a sector. At dome scale these are the
  difference between ten seconds and an hour.
- **Host-class LightPlayer is the long-term target here.** The full dome
  exceeds a single ESP32's budget; the editor has to scale to it either
  way, because the person patching it is using this app.
