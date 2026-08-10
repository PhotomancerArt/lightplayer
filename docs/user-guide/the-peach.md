# The peach

```embed sim-canvas sim=twod view=map fixture=peach_leaf
```

```embed sim-canvas sim=twod view=map fixture=peach_body
```

```embed panel sim=twod,oned mode=interactive
```

That's a peach: acrylic, laser-etched, with LEDs behind it. Fifty-six of
them, on **one strip**, running from the bottom of the fruit up one side,
across both leaves, and back down the other side. One wire in, one wire
out, the way you'd actually build the thing.

The leaves are green and the fruit is pink, and they are lit by two
different little programs. Nothing in the wiring says so — that split is
something you tell LightPlayer, and this page is about how.

## Two fixtures, one wire

A **fixture** is a group of lamps that gets its own picture. The peach has
two: `peach_body` (44 lamps) and `peach_leaf` (12). They both send their
colors to the same output, because they are both the same strip.

Each one lives in a little module of its own — `body/` and `leaf/` — holding
that fixture and the shader that draws for it, and nothing else. Inside a
module the wiring is as plain as it gets: the shader publishes a picture, the
fixture reads it, and neither has to say whose. The leaf's picture cannot
reach the body's lamps, because they are not in the same room. What a module
hands *outward* is its lamps, and the output at the top of the project puts
both modules' lamps on the one wire.

If you're coming from WLED, this is the segments idea — with the part that
always chafed removed. A WLED segment is a slice of the strip that happens
to run a different effect; here the fixture is the real thing, with its own
[shader](#/docs/what-is-a-shader), its own map, and its own place on the
wire. What it is and where it's plugged in are two separate facts, and
you'll see in a moment why keeping them separate matters.

## Where the lamps land: the patch

Walk the strip from the plug and count:

```text
ch  0–21   the body, up one side
ch 22–33   the leaves
ch 34–55   the body again, back down the other side
```

The body's lamps are not contiguous on the wire. Its own lamps *are*
contiguous — lamp 0 to lamp 43, in the order the picture is drawn — but the
leaves sit in the middle of the run. Something has to say so.

That something is a **patch**: a small file, one per fixture, that says
where each stretch of a fixture's lamps lands on the output.

```embed code-figure src=peach-body-patch
```

Two entries, and every number in them is a lamp. On the left, `range` counts
in the fixture's own lamps: lamps 0 through 21, then lamps 22 through 43. On
the right, `at.channel` counts along the wire: channel 0, then channel 34.
The first 22 lamps lead the strip; the other 22 pick up twelve channels
after them, on the far side of the leaves.

The leaves' patch is the one entry you'd expect — all twelve of them,
starting at channel 22.

Anything you don't mention flows on after whatever you did, in order. So a
patch is usually short: pin the piece you care about and let the rest fall
in behind it. Delete the patch entirely and the fixture goes back to
straight-through, from the top of the wire — nothing goes dark, and no lamp
position moves, because the map wasn't involved.

## What `reversed` does

The second entry carries `"reversed": true`, and that one word is worth the
paragraph.

The body's second leg runs *down* the peach. Its lamps are numbered the way
the picture is drawn — continuing up from lamp 22 — but the wire arrives at
that leg from the bottom, so channel 34 is physically the lamp with the
highest number, not the lowest. `reversed` lays the range down end-first:
last lamp at the first channel, walking backwards from there.

Without it the second leg lights upside down relative to the first, and the
symmetric glow the shader draws lands lopsided. With it, the gradient
mirrors around the fruit the way you drew it. It's the fix for the strand
you plugged in at the wrong end — which is every strand, eventually.

## Presentation and sampling are different questions

Here is the same peach again, and it is a different project:

```embed sim-canvas sim=oned view=map fixture=peach_leaf
```

```embed sim-canvas sim=oned view=map fixture=peach_body
```

Same shape, same 56 lamps, same wire. What changed is the kind of program
lighting it.

The version at the top of this page draws a **picture over the peach** — a
pink plane blushing from the bottom up, with a sheen crossing it — and each
lamp takes the color of the picture at the spot where that lamp actually
sits. That's a 2D shader, `vec4 render_2d(vec2)`, and the peach's shape is
the whole point of it. The sheen crosses both legs of the body at the same
moment because in *space* they're right next to each other.

The version just above runs **along the wire**. Its shader is
`vec4 render_1d(float)` — it gets one number, "how far along the strand are
you", and nothing else. It has never heard of the peach. Lamp 0 is hot and
the heat falls off down the run, and it does that in wire order, so the
glow travels the strand rather than crossing the shape. This is how art like
this runs on WLED today, and it is a perfectly good look — it is just a
different question being asked.

Both projects have the same drawing. That's worth saying plainly, because
it's the part people expect to be false: the 1D peach still *has* a map, and
Studio still draws it as a peach, because the map is the presentation — it
is how you and Studio see the fixture. Whether a shader samples that map or
walks the wire is a separate switch, on the fixture, next to it. Drawing
your art is never a commitment to a way of lighting it.

## The same patch, both ways

The two projects are `examples/peach-1d` and `examples/peach-2d`, and their
patch files are **byte-identical**. Not similar — the same bytes, pinned by
a test.

They have to be. A patch answers "which jack did this strand end up in",
which is a fact about the afternoon you built the thing. It has no opinion
about shaders, and shaders have no opinion about it. Rewire the peach and
you edit the patch; change your mind about the look and you don't.

## Make it yours

Open either one — they land in your projects, wiring and all. The 2D peach,
the picture painted over the fruit:

```embed open-in-studio example=examples/peach-2d
```

And the 1D peach, the same artwork lit along its wire:

```embed open-in-studio example=examples/peach-1d
```

Good things to try: change one entry's `at.channel` and watch the fixture
walk down the wire; drop the `"reversed": true` and see the second leg turn
around; delete a whole entry and watch the lamps flow in behind what's left.

## Where next

- [What's a shader?](#/docs/what-is-a-shader) — the little programs the two
  peaches disagree about.
- [Brightness and smooth fades](#/docs/brightness-and-smooth-fades) — what
  the output settings do to these colors.
