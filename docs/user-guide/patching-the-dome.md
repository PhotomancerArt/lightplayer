# Patching the dome

```embed sim-canvas sim=main view=map fixture=dome
```

```embed sim-canvas sim=main view=map fixture=doors
```

```embed panel sim=main mode=interactive
```

That's a real dome — the Small Dome: a 16-foot 2-frequency geodesic
shell of bare struts, hoisted on a riser ring, with glowing triangular
panels suspended inside the strut triangles. **Fifty** panels — the
forty 2V faces plus the ten downward-pointing triangles of the rung
below — each wrapped with **119 lamps** of strip, and one chevron door
that stays warmly lit no matter what the show does. Two control boxes
drive it, thirteen ports each: **"1"** takes the right half of the
dome (and the door), **"Box 2"** the left.

Here's the thing about domes: the mapping never changes — the geometry was
decided when the struts were cut — but the **plugging changes every single
build**. The crew connects each panel to whatever jack is nearest, the
door to whatever port has room left, and the software is expected to
sort it out afterwards. On the big dome this software grew up against,
sorting it out was most of the clicks the software ever saw.

Sorting it out is what a **patch** is. If you've read
[the peach](#/docs/the-peach), you've seen a patch place two fixtures on
one wire. The dome is the same idea at install scale — and it adds the
three things a repeated structure needs.

## Instances, by name

Open `dome/dome.map2d.json` and there are exactly **ten** panels in it
— one per position in a 72-degree sector (`rim-a`, `rim-b`,
`band-a`–`band-d`, `cap-a`–`cap-c`, `zenith`), each a closed 119-lamp
polygon repeated five ways around the center. Fifty panels, described
by ten shapes. (The geometry is generated from the real dome's
structure by `cargo run -p lpt-geodome`.)

```json
{
  "name": "rim a",
  "id": "rim-a",
  "stride": 40,
  "shape": { "repeat": { "shape": { "polygon": { "...": "...", "count": 119 } }, "count": 5 } }
}
```

The `id` is the load-bearing line. It's a stable name for the object —
assigned once, never changed by renames — and it gives every repeated
instance an address: `/rim-a/0` through `/rim-a/4`, and so on across
all ten objects. The patch speaks in those addresses — the shipped one
opens:

```json
{
  "format": 2,
  "outputs": ["1","Box 2"],
  "entries": [
    ["/rim-a/0",0,0],
    ["/band-a/0",0,119],
    ["/cap-c/0",0,238],
    ["...48 more rows..."]
  ]
}
```

Each row reads: *this panel* → *that output* (an index into the
`outputs` table above) → *at this wire lamp*. `/rim-a/0` means "that
panel, wherever its lamps currently are" — change the panel design
next year and every entry still points at the right physical panel,
because the lamp ranges are re-derived from the mapping every time.
Nobody maintains arithmetic, and at fifty panels nobody could.

## Backwards, and turned

Two of the fifty rows carry more:

- `"r"` on `/band-b/2` — that panel was plugged in at its **far end**,
  so its run is laid down the wire back-to-front. One flag, not a
  rewired panel.
- the `40` on `/band-c/0` — rotation. A triangular panel seated one
  corner on reads forty lamps further around its wrap than the design
  says. **Offset** turns it in software the way the crew seated it in
  hardware.

Forty is the panel's **stride** — authored on every panel object
(`"stride": 40`), because 119 lamps over three sides has no intrinsic
lamps-per-side the way an evenly divisible polygon does. The door puts
its own number on a turn: it's a **chevron** — a big open triangle
with no bottom edge, 180 lamps up each ~10-foot leg — and a door
plugged with its legs swapped is a rotation by one leg:

```json
["/door",0,2975,"",180]
```

Rotating by strides is how "it's on, just turned" becomes one edit
instead of a hundred and nineteen.

## Two boxes, shared ports

Look at the two patch files together and you'll see the install: each
box feeds its half of the dome through thirteen ports — two chained
panels per port, 238 lamps — and on box 1's last port the door's 360
lamps ride the **tail** of a panel's 119 (`/band-b/1` at lamp 2856,
`/door` at 2975). Any instance, any port, any output. The output names
in the rows ("1", "Box 2") are labels you choose on the output node —
in the real world that's "the box at 10.0.0.105", and renaming one
never moves a wire.

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
example ships for), `docs/use-cases/2026-08-28-three-domes.md` (the
real dome it models), and lp2014 field experience on the big dome.
