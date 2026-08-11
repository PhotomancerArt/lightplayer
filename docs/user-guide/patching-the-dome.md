# Patching the dome

```embed sim-canvas sim=main view=map fixture=dome
```

```embed sim-canvas sim=main view=map fixture=doors
```

```embed panel sim=main mode=interactive
```

That's a dome — a miniature of a real one. Five identical strut sectors of
thirty lamps radiating from the apex, and three triangular door panels
around the rim that stay warmly lit no matter what the show does. Two
control boxes drive it: **"1"** with three ports, **"Box 2"** with two.

Here's the thing about domes: the mapping never changes — the geometry was
decided when the struts were cut — but the **plugging changes every single
build**. The crew connects each sector to whatever jack is nearest, the
doors to whatever ports have room left, and the software is expected to
sort it out afterwards. On the real dome this one imitates, sorting it out
was most of the clicks the software ever saw.

Sorting it out is what a **patch** is. If you've read
[the peach](#/docs/the-peach), you've seen a patch place two fixtures on
one wire. The dome is the same idea at install scale — and it adds the
three things a repeated structure needs.

## Instances, by name

Open `dome/dome.map2d.json` and there is exactly **one** strut in it — a
path with thirty lamps, repeated five ways around the center:

```json
{
  "name": "sector",
  "id": "sector",
  "shape": { "repeat": { "shape": { "path": { "...": "..." } }, "count": 5 } }
}
```

The `id` is the load-bearing line. It's a stable name for the object —
assigned once, never changed by renames — and it gives every repeated
instance an address: `/sector/0` through `/sector/4`. The patch speaks in
those addresses:

```json
{
  "format": 2,
  "outputs": ["1","Box 2"],
  "entries": [
    ["/sector/0",0,69],
    ["/sector/1",1,0,"r"],
    ["/sector/2",0,0,"",10],
    ["/sector/3",1,39],
    ["/sector/4",0,39]
  ]
}
```

Each row reads: *this instance* → *that output* (an index into the
`outputs` table above) → *at this wire lamp*. `/sector/2` means "instance
2, wherever its lamps currently are" — add two lamps to the strut design
next year and every entry still points at the right physical sector,
because the lamp ranges are re-derived from the mapping every time. Nobody
maintains arithmetic.

## Backwards, and turned

Two of the rows carry more:

- `"r"` on `/sector/1` — that sector was plugged in at its **far end**, so
  its run is laid down the wire back-to-front. One flag, not a rewired
  strut.
- the `10` on `/sector/2` — rotation. The sector reads ten lamps
  further around than the design says. The dome doesn't care where a
  symmetric part starts; **offset** turns it in software the way the crew
  seated it in hardware.

The doors are where rotation earns its keep. Each door is a **polygon** —
a closed triangular outline with nine lamps, three per side. A door seated
one side off is a *rotation by three*, and the patch says exactly that:

```json
["/door/1",1,30,"",3]
```

Three is the door's **stride** — the polygon's lamps-per-side, derived
from its shape. Rotating by strides is how "it's on, just turned" becomes
one edit instead of nine.

## Two boxes, shared ports

Look at the two patch files together and you'll see the scatter: sectors
land on both outputs; doors land on both outputs; and on the ports they
share, a door's nine lamps ride the **tail** of a sector's thirty — port 0
of "1" carries `/sector/2` at lamp 0 and `/door/0` at lamp 30. Any
instance, any port, any output. The output names in the rows ("1",
"Box 2") are labels you choose on the output node — in the real world
that's "the box at 10.0.0.105", and renaming one never moves a wire.

Everything not named in a patch still takes care of itself: a fixture with
no patch at all flows onto the first output, in order, exactly like the
simple projects. Patches are sparse — you pin what the crew moved and the
rest follows.

## When it goes wrong

Nothing in a patch can kill the show. Point an entry at an output name
that doesn't exist and those lamps go quietly dark while the fixture
reports which name is missing. Land two runs on the same lamps of one
port and the contested lamps go dark and the output says which ones.
Everything else keeps lighting. A patch is install-day equipment: it
degrades and reports, it never dies.

Provenance: `docs/use-cases/2026-08-09-mini-dome.md` (the archetype this
example ships for), and lp2014 field experience on the dome it
miniaturizes.
