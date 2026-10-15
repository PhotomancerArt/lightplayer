# Mapping lab

A design lab for rebuilding LightPlayer's mapping and patching editor from
the ground up. It sits outside the app on purpose. The vision is in the
planning workspace: `lp2025/2026-10-10-1853-mapping-design-lab/vision.md`.

```bash
just lab-mapping        # serves the canvas; the printed URL is the source of truth
cargo test -p lab-mapping-model        # the statements
cargo test -p lab-mapping-files        # opening and saving real projects
cargo test -p lab-mapping-fit-tests    # the proof against the product (slow: runs its engine)
cargo run -p lab-mapping-radiance-gen  # regenerates projects/radiance-dome
```

## The crates

- **`model/`** (`lab-mapping-model`): the editor's whole behaviour. Nodes
  (modules, fixtures, outputs), components, objects, layout, the patch and
  its edges, selection, properties, hints, and the simulated board. It has
  no IO, and its input is events; it never reads a clock, so the page
  passes the time in. Yona reads and edits this crate, so keep it small and
  plain. **It depends on no workspace crate.** That keeps it abstract and
  lets it drop into the app later.
- **`web/`** (`lab-mapping-web`): a Dioxus page that forwards events to the
  model and draws what it says. No editor logic lives here.
- **`files/`** (`lab-mapping-files`): opens real LightPlayer projects into
  the model and saves them back, in the product's own formats (vision D51,
  fit-check §6). Fenced like the model: the model and `serde_json` only,
  no product crate — the product's documents it needs are re-created
  there. It carries what the lab doesn't model (shaders and other nodes,
  unknown fields) through untouched, writes only the files it owns and
  only when what they say changed, so saving an unedited project writes
  nothing. It also bakes the three test projects in for the page.
- **`fit-tests/`** (`lab-mapping-fit-tests`): the proof, and the only lab
  crate that depends on product crates. For each test project: an
  unedited save leaves every file byte for byte; every file the lab would
  write reads back through the product's readers as the original does;
  every lamp is `lpc_mapping::resolve`'s bit for bit; and every output
  carries the same lamp on every wire lamp as the product's engine,
  loaded headless from the same files. Plus edited-then-saved round trips.
- **`radiance-gen/`** (`lab-mapping-radiance-gen`): writes
  `projects/radiance-dome/`, the radiance dome in the product's formats,
  from lp2014's geodesic geometry. **Its lamp counts and patch are
  placeholders** (80 lamps a panel, 10 outputs × 32 ports, one panel a
  port); the project's README says what is unknown.

## Statements

`model/tests/statements.rs` holds the behaviour as sentences, one test
each. The doc comment is the statement. To change how something behaves,
change the sentence first, then the code.

## Real projects

The toolbar's **Project** picker opens the desk (the lab's own seed) or
one of the test projects, and fits the view:

- **PLAYFUL choker** (`catalog/projects/playful-choker`): 12 paths, 73
  lamps, one output whose one port takes the rest; its clock, power
  button and shader are plain cards at the left.
- **Zook dome** (`catalog/projects/zook-dome`): one repeat — five copies
  of a 300-lamp zigzag path with a jumper — on five ports.
- **Radiance dome** (`projects/radiance-dome`, generated): 55 panel
  shapes × 5 copies, 22,000 lamps, ten named outputs of 32 ports, every
  panel placed by hand. It loads in about a second and draws, but every
  interaction re-renders the whole SVG — 50,000 elements — so panning
  and hovering take about half a second each in the dev build. Making it
  interactive is the next piece.

What the model took from the product to open them (each with its
statement, fit-check's resolutions applied): lamps resolved in the
product's own f32 arithmetic (`resolve.rs`, bit for bit); paths with
gaps and a `reversed` flag; polygons; repeats (copy *k* turned
*k*·360°/count, keyed `2` like `/sector/2`); place rules (an object on a
port's pixel, reversed, turned in its window) beside auto; an output's
ports as one flat wire, the last one free to take the rest; auto flow in
the product's attach order (each module's nodes by name); colour order
on the fixture; nodes the lab doesn't edit as plain cards; output names.

On disk (`files/src/save.rs`): an auto rule is the fixture writing the
channel its output reads; a place rule is a patch row, its port and
pixel flattened onto the wire; a lab circle is one ring shape per ring
and a group is its children (both lose their grouping until the product
has addressable rings and groups); a lab name the product refuses
(`tail rig`) is written as one it accepts (`tail_rig`). The lab doesn't
open grids, filled polygons, multi-ring rings, or patch rows that place
part of an object or a plain lamp range — it refuses them by name.

## What it covers

A 2D canvas with selection, and the start of the whole node graph on it
(vision D36, D42–D44):

- **Nodes on one canvas.** The project is the root module. Modules are
  frames around nodes, fixtures are frames around a drawing, and outputs
  are cards with one row per port. Frames are see-through: a click inside
  one selects what's in it, and the frame itself is picked by its title or
  border. An output card is not a frame: click it to select it, click again
  for the port under the cursor, and again for the pixel (D47).
- **Layout** (D37, D48), inside-out: in every container the fixtures and
  modules stack top to bottom, and each output sits just to the right of
  the first fixture that feeds it, a fixed gap away, level with its top.
  **Dragging a node pins it** where you drop it — by its title or border,
  an output by its card (or a port or pixel of it). The rest lay
  themselves out around it, and nothing overlaps. A pinned node shows a
  pin on its title; U, or "Unpin" in the right-click menu, gives it back
  to the layout. One drag, one undo.
- **Patch rules** (`patch.rs`): place (one object on a port's pixel,
  reversed, turned) and auto (what no place rule claims fills the
  output's ports in order, after every placed run and after fixtures
  before it in the product's attach order). The effective patch is
  derived and never stored. A rule whose output, port or object is gone
  shows as a problem and is never retargeted.
- **Edges are the patch** (D42, D45, D46): violet, one per object (per
  span, when an object runs on into the next port), leaving the object's
  FIRST lamp toward the output and landing — from the left, with an
  arrowhead — on the PIXEL where its span starts, inside the card. They
  don't change with the selection. A (fixture, output) pair bundles into
  one edge only by density: more than 24 objects (the dome), or zoomed out
  until the fixture has under 8 screen pixels per object. A bundle leaves
  the frame's side nearest where it lands. Hovering an edge lights it and
  says what it carries ("path1 → GPIO 10 · lamps 0–24 at pixel 0"); it
  never selects. Opening a fixture (Enter) frames it with its outputs; ⇧2
  frames the selection (an empty fixture with the outputs its rules name).
  A selected port rings the lamps it carries, and a selected object tints
  the ports it lands on.
- **Pixels** (D47): a port's pixels are squares on its row, laid out by
  the model (`pixel_grid.rs`), each selectable. Arrows walk them, ⇧-click
  selects a run from the first one picked. A selected pixel breathes on
  the board even with nothing patched to it, and the lamp on it is ringed
  on the canvas; select a lamp and its pixel is ringed on its port.
- **A simulated board** (D12, D16, D44): the lab is always attached to a
  board, and for now the model simulates it (`device_frame.rs`). Every
  port row on an output card shows its pixels as the board has them: a
  stand-in pattern (a hue sweep per object), then the selection in lamp
  space (everything else dims to a quarter; every object under the
  selection shows old LightPlayer's pattern from lp2014's
  `GenericShapeMapper` — a blue lead-in, a red lead-out, and between them
  black but for a white band sweeping start to end every two seconds; a
  lamp selected on its own breathes white), then a selected port or
  output in channel space (its whole range breathes, patched or not —
  D13), and a selected pixel on its own (D47). While you draw a path or
  circle, only the draft's lamps light, on
  the pixels it will take when it's finished. Lamps outside their
  fixture's box never light (D4). The page ticks at ~30 fps only while
  something moves. The seed's scarf is patched to "scarf out" (D10,
  60 px), so drawing in it lights pixels at once.
- **Lamps in their pixel's colour.** Every lamp on the canvas is drawn in
  its pixel's colour from the same frame the port rows show, so the
  drawing and the strip read as one. A lamp that lights nothing
  (unpatched, or outside its box) is a dim hollow dot.
- **The data path** inside each fixture (`data_path.rs`): a quiet dashed
  wire from each object's last lamp to the next one's first, in patch
  order, and an "in" where a port's data enters. Brighter in the fixture
  you're working in.
- **Drill-down selection:** click selects the outermost thing, clicking
  again goes in, ⌘-click selects the lamp, Esc goes up, Enter goes down,
  and the arrow keys move between siblings. ⌥-click cycles overlaps, and
  right-click lists everything under the cursor.
- **Multi-selection:** ⇧-click adds or removes, and a box selects. The box
  takes what it touches, or with ⌥ only what is fully inside.
- **⌘A climbs** a level each time you press it, out to everything you drew
  and then the nodes themselves, so ⌘A ⌫ always empties every fixture.
- **Editing:** drag moves what's drawn (and pins a node), ⌫ deletes,
  with refusals in rustc style. − and = change a count (a port's pixels too), r reverses
  direction, and ⌘Z undoes.
- **Handles** (`handle.rs`): small squares on the selection — one on each
  point of a path, one on each ring of a circle (its radius), and a
  selected fixture's corner and right and bottom edges (its size; ⇧ keeps
  it square). A path's or ring's handles show while it OR anything inside
  it is selected, a lamp included (D49). Only a drag grabs a handle — even
  from under a selected lamp; a click on one is a click on what's under
  it. Dragging changes geometry, never a count: a path keeps its lamps and
  respaces them, and the cursor note says so. One drag, one undo.
  **Points:** double-click a shaped path's segment, or right-click "Add a
  point to … here", to add one there; ⌥-click a point's handle, or
  right-click "Remove point N", to take one away. A path keeps at least
  two. Handles are never assumed to sit on lamps (Béziers are coming).
- **Move & draw** (D50): a toolbar toggle, named for what's on, and M.
  Off, nothing moves, pins, draws or reshapes (no handles, no tools, no
  counts or sizes that move lamps — `PropDesc::reshapes`); each try says
  "Move & draw is off · M turns it on". Selection, pixels, the board, a
  port's own count and reverse still work — select and patch only, for
  the dome.
- **Drawing:** a path tool (P, or L) and a circle tool (O). A path is
  clicked out point by point, like the product's: each click adds a
  point, the draft runs on to the cursor with its lamps lit on the board,
  double-click or Enter finishes, Esc takes back a point (and, with none
  left, leaves the tool). Its lamps are spread evenly along its length,
  one every 2.4 units. A circle is dragged out from its centre.
- **Square fixtures.** A new fixture is 100 × 100; the seed's are square
  too, sized to hold their drawing. Resize one by its handles, or set
  width and height in the inspector.
- **One tree and a data-driven inspector:** the tree lists modules,
  fixtures, outputs and ports. The inspector shows the properties
  everything selected shares, a fixture's rules and problems, and an
  output's ports with how full each one is.
- **A hint bar** that says what every key does in the current state. Its
  lead says what you're doing ("Drawing a path", "Moving circle1"), never
  what's selected — the breadcrumb says that.

Not covered yet: editing place rules on the canvas (they open, show,
save and light the board, but there is no gesture for them yet), choosing
a rule's output, saving from the page, routes,
other node kinds (patterns), a level above the
project, and a real board (the simulated one stands in until the lab moves
into Studio).
