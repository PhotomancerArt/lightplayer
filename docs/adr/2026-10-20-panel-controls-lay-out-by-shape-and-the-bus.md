# ADR: Panel controls lay out by shape and the bus

- **Status:** Accepted
- **Date:** 2026-10-20
- **Deciders:** Photomancer
- **Supersedes:** None (amends `2026-08-03-panel-visibility-is-derived.md`
  and `2026-10-08-the-board-card-and-one-home-page.md`; see their
  2026-10-20 amendments)
- **Superseded by:** None
- **Plan:** `lp2025/2026-10-19-2255-board-card-panel-layout`; PR #1206; the
  spike and its answers are the study
  [`docs/ux/studies/panel-layout/`](../ux/studies/panel-layout/README.md)

## Context

PR #1196 put a project's whole panel on the connected board card. On the
PLAYFUL choker Yona saw the Scale knob alone on a line under the palette.
The cause was the panel's order: it was the walk's (the tree lists nodes and
slots sorted by name, so the clock comes before the fixture before the
shader, and palette before scale), and the card then pulled the first
control on the `brightness` channel to its front, by name (`MASTER_CHANNEL`).
A fixture's brightness was also redrawn as a knob for the panel
(`UiFixtureFace::panel_brightness`), so knobs, a fader-turned-knob, a
palette and toggles met in no order a layout could rely on. On
`catalog/small-dome`, whose dome and doors are modules with a fixture each,
the hoist put the dome's brightness at the card's front with no heading, as
if it were the whole project's. The card's story fixture listed PLAYFUL's
controls in a tidier order than the board does, so no story showed it
(`docs/defects/2026-10-19-the-card-panel-ordered-its-controls-by-name.md`).

A four-round spike settled how a panel lays out its controls. Yona's
answers are the decision below; the rounds, the options and why the rest
were passed over are in the study.

## Decision

**Three shapes.** Every panel control is one of three shapes, named in core
beside its widget (`UiPanelShape`, `UiPanelWidget::shape()`, an exhaustive
match so a new widget must say its shape):

- a **cell**: a knob, the Speed knob, a stepped knob, a toggle. Many to a
  line (three on a 300 px card).
- a **strip**: a fader, a palette. The line's whole width, its label and
  value on a row above its track.
- a **block**: the Pattern instrument. The line's whole width, an
  instrument of its own.

A surface lays controls out by shape, never by widget.

**The order is the shape's, then the bus's, then the walk's.** Inside a
group: its strips, then its blocks, then its cells; inside each run, a
channel's place in the bus's well-known registry (`WELL_KNOWN_CHANNELS`,
`lpc-model/src/bus/well_known.rs`: brightness, palette, clock.rate, …), a
channel the registry does not know after every one it does; the sort is
stable, so the walk's order breaks every tie (`panel_order.rs`). Nothing in
panel code names a channel: brightness leads every group that has one
because it is a strip and the registry ranks it first, not because a rule
says "brightness". Brightness is a fader on every panel again, the same
`UiPanelControl` its fixture's card draws.

**The root is a band; a group is a heading.** The root scope's controls sit
on a tinted band with no word ("global" is not said). Each nested group (a
child module's scope, a playlist's) sits under its own line and a heading of
its node's kind icon and name (`UiPanelGroup.kind`, `UiBoardPanelHeading`).
No boxes. Light lines run between every two controls, edge to edge, a step
lighter than a group's own line.

**The whole cell is the target.** A control's cell (or strip) is its target,
with a hover: a knob drags from anywhere in its cell, a fader from anywhere
along its strip (a press on its track still jumps there), a toggle flips
from anywhere in its cell. A press that never moves past a small slop (a
tap) writes nothing and opens the control's details; the Speed knob's tap
waits out its double-click (1×) first. Its two small actions are 24 px
circles in its top corners: let go (↺) top-left while held, details (ⓘ)
top-right on hover, while focused, and always on a toggle where nothing
hovers (its tap is its flip). They never move the label or the value. Each
widget keeps its own gesture and draws the cell's label and value inside its
own surface, so the drag math is written once.

**The connected board card first.** The card draws this today
(`board_panel_picks` orders and heads its sections; `CompactPanel`,
`PanelCell`, `PanelStrip`, `PanelCorners` draw them). The play page and the
editor's Panel section keep the walk's order and `ModulePanelControl` until
their own plans adopt the same pieces.

## Consequences

- **The registry's order is now the panel's.** Reordering
  `WELL_KNOWN_CHANNELS` (the picker's display order) moves every panel; a
  core test (`panel_order.rs`, `the_registrys_order_is_the_panels_order`)
  pins brightness < palette < clock.rate and says so in its failure.
  `lpc-model` itself was left alone: it is firmware-linked, and a doc
  comment there would have pulled the emulator and heap jobs into a
  Studio-only change.
- **One control, one widget.** The bend recorded in the panel-visibility
  ADR's 2026-10-19 amendment is undone (its 2026-10-20 amendment).
- **The card's panel is taller.** Strips carry a label row, and cells leave
  room for their corners; the card still grows once, on connect.
- **On a phone, a vertical swipe that starts on a knob turns it** (its cell
  is `touch-action: none`); a strip, a heading or the picture still scroll
  the page. Asked at the plan's gate.
- **The guide gains a proposed principle**, P21 "targets fit the hand"
  (`docs/ux/README.md`), asked at the same gate.
- **The playlist zone waits.** Where the Pattern instrument sits, what the
  playing pattern's group holds and whether Detail and Palette are shared
  wait on the playlist-bus vision; until then the card keeps today's two
  playlist groups, drawn in this look.

## Alternatives Considered

From the spike (each is drawn in `spikes/panel-layout` or the question
page):

- **Today's order with the hoist**: the walk's order plus brightness pulled
  forward by name. It is what left Scale alone and put a dome's brightness
  at the front of the card.
- **Cells before strips**: brightness would then need a "lead" flag on the
  bus, another exception with a name.
- **A dense packing** that fills holes out of order: the order would stop
  being one a person can predict.
- **Shared lines** (concept B, cells and strips on one line): a strip no
  longer keeps the line's width; not now.
- **Group tiles** (concept C): on a card or a phone it is the same as A.
- **The master in the frame** (concept D: brightness at the picture's foot,
  a thumb bar, the header): a special place for one channel, which the rule
  above refuses; not now.
- **A named band** ("Everything", "Global", "All patterns"): the tint says
  it, and the word was wrong for some projects.
- **One zone at a time** (G3) and **channel strips** (G4): rejected in
  round 2.
- **An authored order** (an ordered `panel` list in `module.json`): a
  project-format change, Yona's call; not now.
- **Glyphs, tabs, dog-ears or a foot bar for the corner actions** (round 3):
  circles were picked, 24 px, in the top corners.

## Follow-ups

- The play page and the editor's Panel section adopt the cells, strips,
  order, band and headings (their own plans).
- The playlist zone, after the playlist-bus vision (the plan's held phase).
- P21 and the two touch calls (the knob's swipe, a toggle's ⓘ) are settled
  at the plan's visual gate; this ADR is amended if they change.
