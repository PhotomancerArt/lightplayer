# Panel layout

How a panel lays out its controls: four spike rounds, 2026-10-19,
drawn for the connected board card first. This is the first study (`docs/ux/README.md`:
"a converged spike becomes a study, linked from its principles"). The
decisions are in [the board card's anatomy](../../anatomy/board-card.md)
(Panel, and L1–L10 under Decided); this page keeps what was asked, what was
picked, and what was passed over and why, so nobody tries the same things again.

Linked from P9, P10, P16 and P21 in [the guide](../../README.md).

## What was asked

PR #1196 put the board's whole panel on the connected card. On the PLAYFUL
choker, Yona saw the Scale knob alone under the palette.

The cause was the panel's order, which was the walk's. The tree keeps nodes
and slots sorted by name (the clock, then the fixture, then the shader;
palette before scale), and the card pulled the first `brightness` it found to
the front, by name. The card's story fixture listed its controls in a tidier
order (speed, brightness, scale, palette) than the board does, so no story
showed it.

The question grew from there: how should a panel lay out its controls, on the
card, the play page and the editor, and how do a project's own controls stand
apart from one pattern's?

## The four rounds

**Round 1** (`spikes/panel-layout`, commit `d6d7c56b8` on
`claude/panel-layout-spike`). Five layouts, drawn on real projects: 0 today,
A one column sorted by shape, B shared lines, C group tiles when wide (the
same as A on a card or a phone), D the master brightness in the frame (the
picture's foot, a thumb bar, the header). Yona's answer: every control is one of
three shapes, a cell, a strip or a block, and brightness is a fader, first,
but it "shouldn't be special cased ... the bus should handle this?" Yona also
asked for a strip's label above its control, light lines between controls, and
the whole cell as the hit area, with a hover, and for a way to tell a
project's controls from one pattern's.

**Round 2** (same page, commit `bea7947a9`). Brightness first, ranked by the
bus's registry rather than by name (small-dome showed why: its dome and its
doors each have a brightness, and today's card put the dome's first, as if it
were the project's). Four ways to separate global from specific controls (G1
to G4, below), the light lines, and the whole cell. Yona's answer: ranking by
the bus, yes; the rest "looks pretty good" (the global band, the lines,
dropping shared lines). On the whole cell Yona asked for corners for let go
and details, "clearly delineated", because today's targets were tiny. And
on the authored-order question, "a single unified system for these controls".

**Round 3** (the question page, Q1 to Q7). Seven questions, each with live
mocks in Studio's look. Yona's picks:

```text
Q1  which actions get a corner?      b  let go while held, details on hover
Q2  how is a corner marked?          c  circles
Q3  how big is a corner?             b  24 px
Q4  where do a strip's actions go?   a  the ends of its label row
Q5  a press without a drag?          a  opens details
Q6  "a single unified system"?       no pick: it applies to the device cards first, more surfaces later
Q7  what does the global band say?   d  nothing, the tint alone; the headings reflect the project's structure, with minimal labels
```

**Round 4** (the same page, Q8 and Q9):

```text
Q8  where do the corners sit?        a  circles in the cell's top corners: let go top left while held, details top right on hover
Q9  how do zone headings read?       b  the node's name and its kind icon
```

The panel design had converged, except for the playlist zone (below).

## What was passed over, and why

The reasons are the spike pages' own words.

**Layouts (rounds 1 and 2)**

- **B, shared lines.** A strip stretches and shares a line with the cells
  before it. With labels above, that puts a strip's header beside a knob's
  dial at a different height, a palette is 132 px wide on one card and 284 px
  on the next, and lines of knobs and bars go ragged (P3, P8). Dropped.
- **D, the master in the frame.** A special case on every surface, which the
  bus ranking rules out, and on the card it lays a control over the live
  picture, which belongs to the card's top (P18). Dropped.
- **C, group tiles.** On a card or a phone a tile is the surface's whole
  width, so C is A there. Parked for a wide surface.
- **Cells before strips.** Brightness would land under the knobs unless the
  registry also marked it a lead: one more fact in the bus, and still an
  exception. Strips come first.
- **Dense.** Later cells pulled into the holes. Where a knob lands can't be
  predicted.

**Global and specific controls (round 2)**

- **G1, headings only.** On small-dome two Brightness faders differ only by
  the heading above them, and the root's lone Speed knob doesn't say it
  reaches both zones.
- **G3, one zone at a time.** Three zones are always hidden, and a held
  control in a hidden zone has nowhere to show its gold.
- **G4, channel strips (a mixer).** Lovely on a desktop and for small-dome. On
  a card, Aurora's five columns wrap into a second row of columns, and a
  playlist's instrument has to sit outside the strips.
- **G2, the band,** was taken, without its words. The page named it for its
  reach ("Everything", "All patterns") and its zones "Scanner · this
  pattern"; round 3's Q7 and round 4's Q9 cut those back to a tint and a
  name with a kind icon.

**What a band says (Q7)**

- **a, its reach** ("Everything", "All patterns").
- **b, the project's name:** the band is the project, the zones are parts.
- **c, "Global":** short and exact, but our word, not theirs (P11).

**Corners (rounds 3 and 4)**

- **Q1.** a, both always (fifteen ⓘ on Aurora's card); c, let go only, with a
  press opening details everywhere; d, both on hover only, which hides a held
  control's way out until it is hovered.
- **Q2.** a, a tab (a square the light lines cut out of the corner; the page's
  lean); b, a dog-ear (the triangle's target is half its box); d, a header
  bar (biggest targets, but the knob sits lower). The page warned that a chip
  is a box on a cell on a panel (P9); circles were picked anyway.
- **Q3.** a, 20 px (still small for a thumb); c, 32 px on touch (on the narrow
  card the corners cover the whole top row).
- **Q4.** b, the strip's corners; c, an end cap (the track gets shorter).
- **Q5.** b, nothing (details then needs a visible corner on a phone); c,
  select it for the keyboard (invisible on a phone).
- **Q8, b to g.** b, circles at the bottom corners (a long label meets them;
  in a narrow cell they cover its ends); c, beside the label (the label gives
  up 48 px, and on the narrow card nothing fits); d, beside the value (the
  narrow card barely has room); e, tabs at the bottom; f, dog-ears at the
  bottom (half a box to press); g, a foot bar (the biggest targets, but the
  label keeps a third of the cell). The top corners won because the knob is
  round, so its top corners are empty space and nothing collides, even in a
  50 px cell.

**Headings (Q9)**

- **a, names only:** the fewest words, but the structure is only the order.
- **c, nested as in the tree** (the page's lean): a zone inside another
  indented on a thin rail. Yona picked b: a name and a kind icon says what each
  zone is (P1) without drawing the nesting.
- **d, paths** ("Pattern › Scanner"): exact, but more words on every deeper
  heading.

## What waits

- **The playlist zone.** Where the Pattern instrument sits, what the playing
  pattern's group holds, and whether Detail and Palette are shared all wait on
  the playlist-bus vision (its own session). One thing the trace found: the
  pattern library's vision calls Detail and Palette shared knobs that stay
  put across a switch, but in the model each pattern's shader reads them in
  its own scope, so they would show in the playing pattern's group, per
  entry, and start over on each switch. That was read from code, not run.
  Until the vision settles, the card keeps today's playlist groups, with the
  new heading and look.
- **An authored order.** An ordered `panel` list on a module would change the
  project format, so it is Yona's call. Not now: the shape, the bus's rank
  and the walk's order do the work.
- **Other surfaces.** The play page and the editor's Panel section (the editor
  has no panel dock; its panel is the root module card's Panel section) adopt
  this later. Yona's note on Q6: the device cards first, then more.
- **A pinned band and tiles.** The band staying in thumb reach on a phone, as
  a column on a desktop, and group tiles on a wide surface (C) are not drawn
  for the card.
- **Three calls pending Yona at the plan's gate:** a toggle's ⓘ always showing
  on touch (L9), a vertical swipe on a knob's cell turning it (L10), and
  P21 "targets fit the hand".

## Where the pieces are

- The spike page for rounds 1 and 2:
  [`spikes/panel-layout/index.html`](../../../../spikes/panel-layout/index.html).
  It is the round 2 version; round 1's is commit `d6d7c56b8` on the branch
  `claude/panel-layout-spike`. Open it by `file://`. It opens in a state with
  `?data=playful|smalldome|piece|aurora|porch|tiny`, plus `order=`, `lines=`,
  `cardCells=` and `held=`.
- The question page for rounds 3 and 4:
  [`spikes/panel-control-questions/index.html`](../../../../spikes/panel-control-questions/index.html),
  and published as an artifact at
  <https://claude.ai/artifact/XFf8KQBvLVhzETLceJeckZ>. It keeps its answers
  in the artifact's store (`answers/round3`, `answers/round4`), so on a plain
  `file://` there is no store behind it: it starts at "Loading…", settles on
  "Saved in this browser only", and shows none of Yona's picks (they are in
  the table above). The page in the repo is round 4's; round 3's questions are
  in commit `b80b0ac03` on the same branch.
- The anatomy: [board card](../../anatomy/board-card.md), Panel.
- The plan, in the planning workspace:
  `lp2025/2026-10-19-2255-board-card-panel-layout`.
