# Drawable picture

A connected board's live picture that a finger draws on. Shown wherever a
live picture of a connected board is: the board card's surface, Play's
banner, the editor's root-face hero. Built in core
(`lpa-studio-core/…/app/draw/`: the drawable fact, the stroke, the fit),
drawn by the web (`lpa-studio-web/…/node/lamp_draw.rs`).

Status: **built** 2026-10-20 (plan `lp2025/2026-10-20-0019-drawing-on-the-picture`,
P5/P6). Principles: P18 (the surface is the picture, no words over it),
P3 (nothing moves when it becomes drawable), P4 (quiet: no label, no mode),
P14 (the ring is a shape, not a colour).

## Whole

```text
┌──────────────────────────────────────────┐
│     ● ● ● ● ● ● ● ● ● ● ● ● ● ●          │   1 picture   the live lamps, as ever
│   ● ● ● ●  ◯  ● ● ● ● ● ● ● ● ●     ╭────┤   2 ring      under each finger, at once
│   ● ● ● ● ● ● ● ● ●  ◯  ● ● ● ●     │ ⋯  │   3 corner    the card's own, untouched
│     ● ● ● ● ● ● ● ● ● ● ● ● ● ●     ╰────┤
├────────────────────────────┬─────────────┤
│ Fluid                      │ ✓ Done      │
```

```text
1 picture   the board's live lamps. drawable only while core says so:
            a connected board (not watched, not the library) playing a
            pattern that reads `touches`.
2 ring      a thin bright ring under each finger, drawn by the page the
            moment the finger moves: "your finger is here". the lamps
            behind it catch up (the board draws); the ring is never a
            lamp colour. it fades within a quarter second of lifting.
3 corner    unchanged: the ⋯ is cut out of the picture, never drawn on.
```

## States

```text
state            picture                                   pointer
not drawable     today's picture, untouched                none (a phone scrolls past it)
drawable         the same picture, no words                crosshair; a drag draws, a tap dabs
drawing          the picture + a ring per finger            each move reported to core
lifting          the ring fades (250 ms); the dye drifts   one release reported
```

There is no label and no mode switch: a crosshair over a drawable picture is
the only sign, and the first touch answers. Yona's "ease over ceremony".

## The surface (`LampDrawSurface`)

```text
┌ div.ux-lamp-draw ─────────────────────────┐   touch-action: none, crosshair —
│  canvas  (the lamps, pointer-events none) │   ONLY on a drawable picture
│  svg     (the rings, pointer-events none) │
└───────────────────────────────────────────┘
```

```text
reports     StudioCommand::Stroke: every live finger as 0..1 of the drawn
            picture box (LampPictureFrame::fit, the canvas's own fit), an
            empty list when the last one lifts. a fact, like Place: the web
            builds no action; core paces, renews and releases.
capture     a finger keeps reporting past the picture's edge (it can draw
            off the lamps; core does not clamp).
```

bad → do, from the plan's feel questions:

```text
bad:  a "Draw" toggle on the card                       # Q5/Q6: a mode to find before the thing works
do:   the picture is drawable while it can be, and says so with the cursor

bad:  touch-action: none on every picture               # Q6: a phone could no longer scroll a page of cards
do:   only on a drawable one

bad:  the ring in the stroke's colour                   # ui.md: lamp colour belongs to the lamps
do:   a bright neutral ring; the colour arrives on the lamps
```
