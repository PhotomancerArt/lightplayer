---
status: fixed          # the order is now the shape's, then the bus's, then the walk's
found: 2026-10-19      # how: hardware-walk — Yona at #1196's gate, on the PLAYFUL choker
fixed: 2026-10-20     # PR #1206
area: lpa-studio-core board_card/board_panel_picks.rs; lpa-studio-web card story fixtures
class: stand-in-divergence
related: [docs/adr/2026-10-08-the-board-card-and-one-home-page.md, docs/adr/2026-08-03-panel-visibility-is-derived.md, lp2025/2026-10-19-2255-board-card-panel-layout]
---
# The connected card's panel ordered its controls by name, and a tidier story fixture hid it

**Symptom** — On the real PLAYFUL choker, connected, Scale sat alone on a
line under the palette, after Speed and Brightness had been drawn as knobs.
On `catalog/small-dome`, the dome's brightness led the card with no heading,
as if it were the whole project's, though the project has two fixtures
(Dome, Doors) each with its own.

**Root cause** — The panel's order was the walk's: nodes and slots come out
sorted by name (clock, fixture, shader; palette before scale), and
`board_panel_picks` then pulled the first control on the `brightness`
channel to the front by name (`MASTER_CHANNEL`), from the root's controls or
else from the first nested group that had one. A fixture's brightness was
also turned into a knob for the panel (`UiFixtureFace::panel_brightness`),
so a strip, a palette and a row of cells met in no order a layout could rely
on. The PLAYFUL story fixture listed its controls in a tidier order than the
board's real walk, so no story showed it.

**Fix** — Each widget names its shape (`UiPanelWidget::shape()`: cell,
strip, block), and one function orders a group's controls: strips, then
blocks, then cells, each run by `WELL_KNOWN_CHANNELS`' order, then the walk's
(`panel_order.rs`). The picks hoist nothing: the root's controls are the
band (no heading), each nested group a section under its name and kind
(`UiPanelGroup.kind`). `MASTER_CHANNEL` and `panel_brightness` are gone, and
brightness is the fixture's own fader on a panel again. The PLAYFUL fixture
takes the board's real order in the web phase of the same plan.

**Regression coverage** — `panel_order.rs`: the PLAYFUL walk order comes out
brightness, palette, Speed, Scale; the registry's order is pinned (a reorder
of `WELL_KNOWN_CHANNELS` moves every panel, and its test says so); a stepped
knob on the palette channel is a cell. `board_panel_picks.rs`: nothing is
hoisted, the root has no heading, every section is ordered. The real
`catalog/small-dome` (`studio_face_e2e_tests.rs`): the band holds the clock's
Speed and no brightness, and Dome and Doors each lead with their own.

**Lesson** — A story fixture that is tidier than the data it stands in for
proves the layout against a case that never happens. A fixture for a
layout rule should carry the real source's order, and the rule should name
no channel: a hoist by name is a second, hidden order that the walk's
order and the widget never agree with.
