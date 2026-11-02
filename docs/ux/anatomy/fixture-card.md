# Fixture card

One fixture in the Graph: its own coordinate system, drawn at a standard
width, with the picture its sources make under its lamps. The inspector
beside it holds the box and the four choices that decide what the lamps
sample. Built in core (`lp-app/lpa-graph`, offers in `lpa-studio-core`),
drawn by the web (`lp-app/lpa-graph-web`, `faces/fixture_face.rs`).

Status: **proposed**, for review at G1 of plan
`lp2025/2026-10-28-0935-fixture-space-theory` (the theory is that plan's
`theory.md`, D3–D28). Nothing here is built yet. Where a line replaces what
ships today, its comment says `was:`. The numbers marked *lean* are G1
questions: O5 (the card's width and cap), O6 (the margin of "Fit the box to
the parts") and O7 (physical size).

## Whole

The PLAYFUL choker (box 132.41 × 36.26, declared 132 × 36 mm), at zoom 1:

```text
 PLAYFUL choker  ⚠ 3                                        1 title tab · 2 problem mark
┌──┬──────────────────────────────────────────────────────┐
│mm│0        20        40        60        80       100   │  3 ruler (top)
├──┼ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤  4 overscan, the picture faded
│ 0│░░╔══════════════════════════════════════════════╗░░░░│
│  │░░║  ●  ●●●   ●     ●●   ●   ● ●●●  ●  ●  ●     ║░░░░│  5 the box line
│20│░░║  ●●● ●   ●●●   ● ●   ●   ●●   ●  ●  ●     ●○ ║░ ○ │  6 the picture, full strength
│  │░░║  ●   ●●● ● ●   ●     ●   ●    ●●●●  ●●●   ║░░░░│  7 lamps: ● lit, ○ outside (off)
│  │░░╚══════════════════════════════════════════════╝░░░░│
└──┴──────────────────────────────────────────────────────┘
 ↑ 3 ruler (left)          240 px wide at zoom 1; height from the box (66 px here)
```

```text
1 title tab    the fixture's name, in the overlay at one size, as every       P1 · P8
               frame's tab is today. It picks the fixture.
2 problem mark ⚠ and a count, beside the name: a mark only. The words and     P2 · P14
               the fix live in the inspector's Box section.
3 rulers       top and left, in the fixture's own units: mm (or m) when it    P15 · D8
               declares a size, "units" when it does not.
4 overscan     a margin past the box with the picture faded: wrapping,        R3 · D14
               tiling and a cut-off edge read at a glance.
5 box line     the view box, one solid line. Inside is lit; outside is off.   D12
6 picture      the source as this fixture sees it, through the same T the     R3 · D3
               lamps sample with (one probe, no fit math in the web).
7 lamps        each lamp where the drawing puts it, sized from the pitch.     D28 · P14
               Inside the box: filled, in the colour it shows.
               Outside: a hollow dashed ring, never filled. Off by shape, not by colour alone.
```

The card has no zoom or pan of its own: you zoom the Graph (one camera,
D10). There is no corner drag and no Width/Height row: the box is set in
the Box section (D9, D15).
`# was: the box came from render_size, with a corner drag and Width/Height rows that rewrote the canvas (the 256×1 → 8×8 bug)`

## Size

```text
width     240 px at zoom 1, every fixture (O5 lean).                         D9 · P8
height    width × box height / box width, the box's own ratio.
cap       3 × the width (720 px, O5 lean). A taller box is drawn contained:
          full height, narrower, centred. The card stays 240 wide.
minimum   the chrome's: the top ruler, one row for the left ruler's label,
          and the lamps' size. A thinner box is drawn at its true ratio, a
          line, centred in that minimum. Never stretched.
```

Three fixtures at the standard width:

```text
 Strip 256 × 1  (a box 256 × 1 units: 240 × 0.94 px, so the minimum holds it)
┌──┬───────────────────────────────────────────────────────┐
│un│0         64        128       192       256            │
│  │═══════════════════════════════════════════════════════│   the box, a line: honest, not a bar
└──┴───────────────────────────────────────────────────────┘

 PLAYFUL choker  (132.41 × 36.26: 240 × 66 px)
┌──┬───────────────────────────────────────────────────────┐
│mm│0        20        40        60        80       100    │
│ 0│ ╔══════════════════════════════════════════════════╗  │
│20│ ║   P   L   A   Y   F   U   L                      ║  │
│  │ ╚══════════════════════════════════════════════════╝  │
└──┴───────────────────────────────────────────────────────┘

 Tall ring  (100 × 400: 240 × 960 px, over the 720 cap: drawn 180 × 720, centred)
┌──┬─────────────────────────────┐
│un│      0    50   100          │
│ 0│      ╔═══════════╗          │
│  │      ║   ● ● ●   ║          │
│  │      ║  ●     ●  ║          │
│100      ║   ● ● ●   ║          │
│  │      ║    ...    ║          │   (720 px tall)
│400      ╚═══════════╝          │
└──┴─────────────────────────────┘
```

## Rulers

```text
edges     top and left, outside the box's own area, as Figma and Illustrator
          draw them. The origin square names the unit.                       D8 · P1
unit      "mm" when the fixture declares a physical size (O7 lean: a number
          pair in millimetres); "m" past 2 m; "units" when it declares none.  P15
          Ruled in the drawing's own numbers, scaled to the declared size.
ticks     a major tick every 1, 2 or 5 × 10ⁿ units, the smallest step at
          least 40 px apart at the card's scale; four minor ticks between.   P15
labels    on major ticks only, as few digits as read naturally: 0 20 40, not
          0.0 20.0 40.0; 0.5 m 1 m 1.5 m, not 500 1000 1500.                P15 · P17
zoom      a closer zoom adds ticks and labels (the 40 px rule again), never
          a different unit.
```

## The box, the picture and the lamps

```text
box       the view box, one solid line in the frame's ink. Not a fill and
          not a shadow: the line is the boundary that switches lamps off.    D12 · P9
picture   the source through T over the box plus the overscan margin: one
          texture from the device (or the simulator), sampled through the
          fixture's T and outside rule texel by texel (plan PD6). The web
          draws it in fixture space; it knows no fit.                        R3 · PD6
inside    full strength, under the lamps.
overscan  past the box by an eighth of its long side (at least 12 px at the
          card's scale), the same picture at about a third of its strength. D14
          A cover fit shows the picture running on past the box; contain
          shows its edges inside the box; wrap shows the tile repeating.
lamps     a disc at each lamp's centre, sized from the pitch (D28: lamps
          have no size field). Lit: filled with the colour the lamp shows.
          Outside the box: a hollow dashed ring, no fill, in the frame's ink. P14 · R4
          Dragging a lamp out turns it into a ring as it crosses the line.
```

`# was: lamps outside the box were clamped to its edge on the device and drawn "never lit" only in the Graph`

## Box section (inspector)

The fixture's inspector, first section after its facts:

```text
Box ──────────────────────────────────────────────────────────────
  x  0         y  0                                                 the box's corner
  w  132.41    h  36.26    units                                    its size, in the drawing's numbers
  Size        132 × 36 mm                              │ Set size │  physical size (O7), or "not set"
  Parts       11.35, 10.74 to 123.88, 26.23                         the parts' bounds
                                          │ Fit the box to the parts │
  ⚠ 3 lamps are outside the box. They are off.          ← orange
```

```text
numbers   x, y, w, h: four number fields, the drawing's own numbers, edited  R1 · D15
          in place. A width or height of 0 is refused where it is typed.     P7
size      the physical size, a number pair in millimetres (O7 lean). Shown
          as "not set" when absent; the rulers then read "units".            D7 · P13
parts     the parts' bounds, read-only, in the same numbers: what the        P1
          action below would set.
action    "Fit the box to the parts": the parts' bounds plus half a pitch on R4 · P12
          every side (O6 lean). Always offered; first among equals while
          lamps are outside.
warning   only while lamps are outside: the count, and that they are off.   P2 · P4 · P14
          Orange, with ⚠: attention, not an error. The card's ⚠ mark is
          the same problem's mark, and this is its one place in words.
new       a new fixture's box is 500 × 500 mm (D15). A generated fixture
          (a starter strip, a pattern rig) gets its parts' bounds plus the
          margin.
```

## The four choices

Each one is a typed offer in core (`Choice` params, each option carrying
its one-line `OfferChoice.detail`), drawn as tiles that say what each
option does (R5). Tiles, not a dropdown: one press, everything visible, as
the Nodes pane's dimensionality section does
(`lpa-studio-web/src/app/node/face/space_section.rs`). The picture under the
lamps redraws as soon as one is pressed (R6).

### The shader's interface (on the shader's inspector)

```text
Coordinates ──────────────────────────────────────────────────────
  ┌──────────────────────┐ ┌──────────────────────┐
  │ ✦ unit               │ │ ▦ shadertoy          │
  │ Centred, −1 to 1,    │ │ Pixels of a 64 × 64  │
  │ y up. The fixture    │ │ view, from the       │
  │ decides the frame.   │ │ bottom-left. For     │
  └──────────────────────┘ │ porting Shadertoy.   │
         (selected)        └──────────────────────┘
```

```text
unit       "Centred, −1 to 1, y up. The fixture decides the frame."          D19 · P1
shadertoy  "Pixels of a 64 × 64 view, from the bottom-left. For porting
           Shadertoy code." (N is O3's constant.)                            D19
```

### The fit (on the fixture)

```text
Fit ──────────────────────────────────────────────────────────────
  ┌────────┐ ┌────────┐ ┌────────┐      ┌───┬───┬───┐
  │▓▓▓▓▓▓▓▓│ │ ┌────┐ │ │▓▓▓▓▓▓▓▓│      │ ○ │ ○ │ ○ │
  │▓▓┌──┐▓▓│ │ │▓▓▓▓│ │ │▓▓▓▓▓▓▓▓│      ├───┼───┼───┤   where the picture sits
  │▓▓└──┘▓▓│ │ └────┘ │ │▓▓▓▓▓▓▓▓│      │ ○ │ ● │ ○ │   when it is not the
  └────────┘ └────────┘ └────────┘      ├───┼───┼───┤   box's shape
    Cover      Contain     Fill         │ ○ │ ○ │ ○ │
  (selected)                            └───┴───┴───┘
  Scale     1×        ─────●──────
  Rotation  0°        │ 0° │ 90° │ 180° │ 270° │  or a number
```

```text
cover      "Fills the box. The picture's edges may be cut off."             D16 · D17
contain    "The whole picture fits inside the box. The box may show past it."
fill       "Stretches the picture to the box, each way on its own."
glyphs     each tile draws its own mode: the box as a line, the picture as
           a square shaded in or past it.                                    P1 · P9
alignment  a 3 × 3 grid of dots, the chosen one filled; its line: "Where the
           picture sits when it is not the box's shape." Dimmed under fill,
           with that reason on hover (nothing to align).                     P7
scale      a number and a slider, read as "2×"; its line: "Zooms the picture
           about its centre."                                                P15
rotation   the four quarter turns as tiles, and a number for anything else;
           clockwise as drawn. Its line: "Turns the picture about its centre."
default    cover, centred, 1×, 0° (D17).
```

### Past the picture's edge (on the fixture)

```text
Past the picture's edge ──────────────────────────────────────────
  Only for a source with edges: a fluid, an image.
  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
  │ ▓▓▓▓▓▓▒▒ │ │ ▓▓▓▓▓▓▓▓ │ │ ▓▓▓▓▒▒▓▓ │ │ ▓▓▓▓▓▓   │
  │ Hold the │ │ Repeat   │ │ Mirror   │ │ Solid ■  │
  │ edge     │ │          │ │          │ │          │
  └──────────┘ └──────────┘ └──────────┘ └──────────┘
   (selected)
  Different for x and y ›
```

```text
lede       "Only for a source with edges: a fluid, an image." Dimmed, not
           hidden, while the source is a view (a shader), so the choice
           keeps its place (P3) and says why it does nothing now.            P7 · D6
hold       "Past the edge, the edge's own colour carries on." (clamp)        D6 · O2 lean
repeat     "The picture tiles." (wrap)
mirror     "The picture tiles, every other copy flipped." (mirror)
solid      "Past the edge, one colour." with its swatch, which opens the
           colour picker.
per axis   "Different for x and y ›" opens two rows of the same four tiles,
           x and y. Closed again, if they agree.                             P5
default    hold the edge (O2 lean).
```

### How a 1D source lands (on the fixture)

```text
1D sources ───────────────────────────────────────────────────────
  ┌───────────────────┐ ┌───────────────────┐
  │ 1 2 3 4 5 6 7 …   │ │ ╱╱╱ across        │
  │ Along the wire    │ │ Across the        │
  │                   │ │ drawing           │
  └───────────────────┘ └───────────────────┘
       (selected)
  (across only:)  ┌──────┐┌──────┐┌──────┐┌──────┐   Normal │ Mirrored
                  │ → →  ││ ↓ ↓  ││ ◎    ││ ↻    │   Normal │ Flipped
                  │left  ││top to││from  ││around│
                  │to rt ││bottom││centre││      │
                  └──────┘└──────┘└──────┘└──────┘
```

```text
order      "Lamp by lamp in wire order: the first lamp is 0, the last is 1."  D23 · D27 · G1 Q2 lean
across     "Through the fit, then along a line across the drawing."          D23 · D24
shapes     the dimensionality section's four tiles, its glyphs drawn by the
           same shape → mirror → flip chain, and its mirror and flip rows.   P8 · P16
           Shown only while "across" is chosen.                              P5
note       the fixture decides; the shader no longer offers its own
           opinion (D24).
           `# was: the shader's "show in 2D by" cell, with the fixture's force flag`
```

## Where it stands

- The card's width (240), cap (3×), the margin (half a pitch) and the size
  field (mm) are G1's O5, O6 and O7. A different answer is a one-line
  change here and one constant in core.
- The Graph's card today keeps `render_size`'s shape and a corner drag; P8
  and P9 of the plan build this one, and the visual gate (G2) looks at it
  on the choker, the eaves, the circular display and a 256 × 1 strip.
