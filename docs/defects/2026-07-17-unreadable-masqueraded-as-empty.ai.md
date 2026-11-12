---
status: fixed
found: 2026-07-17      # how: hardware-walk
fixed: 35f763d94
area: lpa-studio-core/roster
class: state-conflation
related: ["2026-07-22-read-failure-vs-unreadable-content"]
---
# Unreadable device content rendered as "Connected — nothing loaded"

**Symptom** — A device holding old-format content showed up on the
roster as "Connected — nothing loaded" — the same card a genuinely
blank device gets. The device was not empty; Studio just couldn't parse
what it held.

**Root cause** — `DeviceContent::Unreadable` was mapped onto the
Connected-empty card state. The card vocabulary had a state for "there
is nothing" and reused it for "there is something I can't read" —
hiding the existence of content behind the absence state.

**Fix** — A distinct `HoldsUnreadableData` card state: amber, with the
parse detail as a sub-line so the user can see *why* it's unreadable.
The direction state table gained the corresponding row, so the state is
part of the documented card grammar rather than an ad-hoc mapping.

**Regression coverage** — Derivation test
`unreadable_content_maps_to_holds_unreadable_data`, plus a story so the
card state has a visual baseline.

**Lesson** — Never map two facts onto one state to avoid adding a
state. "Empty" and "holds data I can't read" demand different user
decisions — pushing over an empty device is safe; pushing over
unreadable content destroys something. Users make destructive decisions
based on the card, so the card must not summarize away the distinction.
(The conflation had a second layer: "device holds unreadable content"
vs "Studio failed to read" — registered separately as
`2026-07-22-read-failure-vs-unreadable-content`, still open.)
