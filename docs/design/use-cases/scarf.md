# The scarf — one strip, one port, Just Works

Captured 2026-08-21 (Yona, walk-up patching G1).

A single strip of lamps on a single port. A scarf, a bike frame, a rope
light around a doorway, a costume seam. There is nothing to decide: the
strip has a beginning, the port has a beginning, and they are the same
beginning.

## What the user does

Plugs it in and picks something to run. That is the whole flow. If they
are ever asked which lamps go on which port, the product has failed
them — for THIS piece there is only one answer and the app already knows
it.

## Why it matters to us

This is **WLED's main case**, and WLED is what most people in this world
have used. LightPlayer has to handle it at least as gracefully as WLED
does, or the comparison is over before the interesting parts start. The
scarf is also the smallest complete piece: it is what a first-time user
builds, and the shape of every "does this thing work?" test.

## The flow flag here

`auto` — this is auto-mapping's home turf, and the reason auto exists at
all. The fixture's lamps flow onto the wire in order with no patch
document at all; if one exists and is empty, that means the same thing.
There is no unmapped state to track because there is nothing to track:
one strand, one wire, in order.

A scarf that somehow ended up `manual` would be a fixture the user has to
patch by hand for no reason at all. Keep it out of their way.

## What this case asks of the product

- An unpatched fixture must light correctly, immediately, with no
  patching gesture at all.
- Nothing in the Patching view may nag about a fixture that is simply
  flowing.
- Reversal ("I plugged in the far end") stays a one-gesture fix, because
  on this piece it is the ONLY thing that can be wrong.
