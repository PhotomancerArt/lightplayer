# The sign — laid-out lamps, where each one goes matters

Captured 2026-08-21 (Yona, walk-up patching G1).

Lamps arranged into something that reads: letters, a logo, a shape.
Between one and about five ports. The whole point of the piece is that a
particular lamp is at a particular place, so **per-lamp placement
matters** — a strand in the wrong port is not a cosmetic problem, it is a
letter lighting up in the wrong word.

## What the user does

Sits down with the piece once, after it is built, and patches it object
by object: pick the free space on a port, look at what lit up, click that
object. Then it is done, and it stays done — a sign is **mapped once and
rarely changed** after that. This is exactly the walk-up loop the
Patching view is built around.

## Why auto-mapping is HARMFUL here

Auto-flow means every object the user has not placed yet is already
lit — somewhere, in whatever order the fixture happens to enumerate. That
destroys the one thing the user is navigating by:

> **not mapped = not lit.**

Unlit is the progress tracker. Half-way through a sign, the lamps that
are still dark are exactly the work that is left; the ones that are lit
are the ones already answered for. Auto-mapping fills that in with
guesses and the piece tells the user nothing.

Worse, the guesses are plausible. A sign that looks approximately right
because auto-flow happened to land near the truth is a piece nobody
double-checks until it is on a wall.

## The flow flag here

`manual`. Only authored entries place. Objects with no entry are on no
wire, dark on the piece and honestly unmapped in the editor — which is
what makes `Clear` (lp2014's `u`, unmap) mean something, and what makes
"unmap all" a safe way to start a re-patch from scratch.

## What this case asks of the product

- Unmapped must be a real, visible, reachable state — in the tree, on the
  canvas, in the panel, and in the lamps.
- The walk-up loop must stay one key and one click per object: the free
  segment sizes itself to the next object waiting, `a` arms, the click
  lands it, `m` moves on.
- Undo has to be trustworthy enough that "unmap all and redo it" is a
  thing the user reaches for without flinching.

## Marker: camera-assisted mapping

The obvious next move for this case is to let a **camera** do the looking:
flash a lamp group, see where it lit up in frame, write the assignment.
Nothing in the flow flag prevents it — a camera pass is just another way
to author the same entries — and manual flow is the state it would run
against. Not planned; recorded here so the shape stays visible.
