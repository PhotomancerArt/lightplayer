# Mapping lab

A design lab for rebuilding LightPlayer's mapping and patching editor from
the ground up. It sits outside the app on purpose. The vision is in the
planning workspace: `lp2025/2026-10-10-1853-mapping-design-lab/vision.md`.

```bash
just lab-mapping        # serves the canvas; the printed URL is the source of truth
cargo test -p lab-mapping-model        # the statements
cargo test -p lab-layout -p lab-arrows # the layout's and the arrows' statements
cargo test -p lab-mapping-files        # opening and saving real projects
cargo test -p lab-mapping-fit-tests    # the proof against the product (slow: runs its engine)
cargo run -p lab-mapping-radiance-gen  # regenerates projects/radiance-dome
```

## The crates

- **`model/`** (`lab-mapping-model`): the editor's whole behaviour, in two
  halves (architecture §2). **`domain/`** is what's stored and the logic
  over it: nodes (modules, shaders, fixtures, outputs), the bus channels
  that wire them, components, objects, the patch, and the simulated board.
  **`ux/`** is the editor and every user verb: selection, picks, handles,
  properties, hints, and `Editor::view_tree()`, which emits everything
  the page draws as one tree with **no positions** (`UiCanvasView`). It
  never lays anything out: it builds the layout's input and takes the
  placement back as a fact. It has no IO, and its input is events; it
  never reads a clock, so the page passes the time in. Yona reads and
  edits this crate, so keep it small and plain. **It depends on no product
  crate**, only `lab-layout`, which depends on nothing at all. That keeps
  it abstract and lets it drop into the app later.
- **`layout/`** (`lab-layout`): a semantic graph in, placements out — the
  canvas layout by role and the stack layout, pure and dependency-free
  (its README).
- **`arrows/`** (`lab-arrows`): a semantic arrow in, geometry out — curves
  on the canvas, gutter lanes in the stack, labels lying along the line
  (its README). Dependency-free; the page uses it.
- **`web/`** (`lab-mapping-web`): the canvas, its panels and the hint bar
  as a **library** a host mounts, and the lab's own page (the bin) around
  it. The components read the host's editor through a `GraphEditor` and
  say what happened as `GraphInput`s (`model/src/ux/graph_input.rs`,
  applied by `Editor::input`), so the lab's page and **Studio's Graph tab**
  (`lp-app/lpa-studio-web/src/app/workbench/graph_view.rs`, over core's
  editor in `lpa-studio-core`'s `app/graph`) mean the same thing by the
  same input. The board's frame is the host's too: the lab simulates one;
  Studio builds one from the board's own published frames
  (`Project::board_frame_from_ports`). Its styles are scoped under
  `.graph`. No editor logic and no detail decisions live here (below,
  "The page").
- **`files/`** (`lab-mapping-files`): opens real LightPlayer projects into
  the model and saves them back, in the product's own formats (vision D51,
  fit-check §6). Fenced like the model: the model and `serde_json` only,
  no product crate — the product's documents it needs are re-created
  there. It opens shaders as render sources (their GLSL kept, to show)
  and carries what the lab doesn't model (other nodes, unknown fields)
  through untouched, writes only the files it owns and
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

The toolbar's **Project** picker opens the desk (the lab's own seed), the
circular display, or one of the test projects, and fits the view:

- **Circular display** (built in code, `domain/circular_display.rs`): a
  circle of four rings and a one-lamp centre (its innermost ring) on a
  five-port output, starting at D0. Its hardware preview is wired as one
  might really be soldered — ring 3 the other way round, rings 3–5 on
  D1 — for the lead journey. Built in code because the product's files
  can't hold a circle of rings yet (it saves as separate rings).

- **PLAYFUL choker** (`catalog/projects/playful-choker`): 12 paths, 73
  lamps, one output whose one port takes the rest; its shader is a card
  at the left feeding the fixture over `visual.out`, and its clock and
  power button are plain cards in a row above.
- **Zook dome** (`catalog/projects/zook-dome`): one repeat — five copies
  of a 300-lamp zigzag path with a jumper — on five ports.
- **Radiance dome** (`projects/radiance-dome`, generated): 55 panel
  shapes × 5 copies, 22,000 lamps, ten named outputs of 32 ports, every
  panel placed by hand. Its lamps and pixels are drawn on canvases
  inside their cards, so panning and hovering run at 16.7 ms a frame in
  the dev build (they took about half a second when everything was SVG);
  with a repeat selected and its lamps animating, about 83 ms.
  `web/perf/` is the harness that measured it:
  `bun lab/mapping/web/perf/run.mjs <page url> radiance-dome`.
- **Sean's eaves** (`projects/eaves`): two runs turning their corners on
  jumpers, D10 counted 60, the rest flowing on to D9. Its piece is wired
  as the house is (`projects/eaves-wired`: the front run reversed on D10,
  the side run on D9, both by hand), for the J2 walk.

What the model took from the product to open them (each with its
statement, fit-check's resolutions applied): lamps resolved in the
product's own f32 arithmetic (`resolve.rs`, bit for bit); paths with
gaps and a `reversed` flag; polygons; repeats (copy *k* turned
*k*·360°/count; the lab gives the repeated shape an id of its own, so
`/sector/2` is `path1@2`); patch rows (an object on a port's pixel,
reversed, turned in its window) beside auto flow in the product's attach
order (each module's nodes by name), frozen on open into each fixture's
Start and the fewest overrides that land every lamp where the product
does (`product_patch.rs`, the M3 upgrade step in miniature); an output's
ports as one flat wire, the last one free to take the rest; colour order
on the fixture; shaders as render sources (stand-in patterns, picked from
their GLSL); bus channels (`bindings`) — the visual channel a fixture
reads, the control channel an output reads; nodes the lab doesn't
simulate as plain cards; output names.

On disk (`files/src/save.rs`): rules and overrides can't be said in
today's patch rows (patch format 4 is M1's), so a fixture keeps the
patch file it opened with while the product, reading it, still lands
every lamp where the walker does, and is otherwise written expanded —
each object the walker lands at its address, `flow: manual` — and reopens
frozen (its rules come back as overrides). An object the walker clipped
at its port's end can't be said and refuses to save. A fixture writes the
channel its output reads; a shader the lab made is a shader node with a
small stand-in GLSL file (one it opened is never written); a lab circle is one ring shape per ring
and a group is its children (both lose their grouping until the product
has addressable rings and groups); a lab name the product refuses
(`tail rig`) is written as one it accepts (`tail_rig`). The lab doesn't
open patch rows that place part of an object or a plain lamp range, or
a patch that lays one object across two ports (the walker lays a leaf on
one port: `projects/test/quad-wire-oracle`) — it refuses them by name.

## What it covers

A 2D canvas with selection, and the start of the whole node graph on it
(vision D36, D42–D44):

- **Nodes on one canvas.** The project is the root module. Modules are
  frames around nodes, fixtures are frames around a drawing, and outputs
  are cards with one row per port. Frames are see-through: a click inside
  one selects what's in it, and the frame itself is picked by its title or
  border. A fixture's empty inside picks the fixture too, behind anything
  drawn in it (a drag from there still draws a selection box). An output card is not a frame, but its **spans** are picked
  directly, as a fixture's contents are (plan D12, D13; `span.rs`): a span
  is one object's pixels on a port, or a gap between them — a view, never
  stored, and spans may overlap. A click on one selects it (its inspector
  says what's on it, with "Select <object>", and a gap offers "Patch
  here"); click again for the pixel under the cursor (D47); Esc climbs
  output → port → span → pixel back up. The card's title selects the
  output. A press on a span never drags the drawing.
- **Layout runs in the page** (D63), from `lab-layout`, by role: controls
  (a clock, a power button) in a row on top, then shaders, fixtures and
  outputs in columns that follow the flow, each output level with what
  feeds it, modules as frames below, anything else at the right. Core says
  what there is (`Editor::layout_input`) and takes where it landed back
  (`Editor::set_placement`). It is **automatic only, on trial** (D64):
  dragging a node selects it and moves nothing. **Pinning** is under
  Debug: on, dragging a node by its title or border (an output by its
  card) pins it where you drop it, the rest lay out around it, and ⌥U or
  "Unpin" gives it back. Every reach for a drag is a finding for the
  lab's `notes.md`.
- **The engine, small** (D55; `board.rs`): a recreation of the product's
  dataflow, pulled from the outputs like the engine's and kept for the
  frame. A **shader** (a render source, `render_source.rs`) publishes a
  render product on a visual channel — the lab runs no GLSL, so it is a
  stand-in pattern (rainbow sweep, gradient, clouds, rings; `pattern.rs`),
  and the inspector says so and shows the GLSL. A **fixture** reads the
  nearest writer of its visual channel (its own module first, then
  outward), samples the picture at each lamp in its own box, applies its
  **fixture overrides** (`fixture_overrides.rs`: the selection inside it,
  or a draft being drawn) and publishes its **control product** — one
  colour per lamp, in data order (`control_product.rs`). An **output**
  takes each fixture on its control channel as its own control product,
  places them on its pixels by the patch, then applies its **output
  overrides** (`output_overrides.rs`: a selected output, port, run or
  pixel breathes; the rest dims) to its own pixels only. The patterns
  hold still, so the page only ticks while a selection or a draft moves.
- **Bus channels are the wiring** (`channel.rs`, `bindings.rs`). Every
  edge between nodes is a channel binding and says its channel's name,
  small, once per pair of nodes (D53): `visual.out` from a shader's card
  to each fixture it feeds, `control.out` (or an output's own channel,
  `control.scarf_out`) on the patch. The control binding is read the
  product's way: the fixture writes the channel its rules' output reads
  (fit-check §6); a fixture bound to an output with nothing landed yet
  gets one plain edge, node to node.
- **Objects, a tree under the components** (`object.rs`, `object_tree.rs`;
  theory "Components and objects"): every component renders exactly one
  object node — a path is its own leaf, a circle a container of its ring
  leaves, a group of its children, a repeat of its copies — and leaves
  hold lamps. Ids are flat: `path1`, `circle1/r3`, and a repeat (which
  holds a component with its own id) makes copy 0 its child's own ids
  and copy k `path1@k` (nested repeats stack inner first, trailing zeros
  dropped). A ring of one lamp is that lamp at the centre.
- **The walker** (`walker.rs`, `cursor_op.rs`, `patch_rule.rs`,
  `object_override.rs`; theory "Patching"): each fixture walks its object
  tree depth first, holding a cursor — an output, a port, a channel —
  that each object it enters moves, field by field, by **Keep**, **Set**
  or **Add** (values in pixels or channels). Rules on the fixture and its
  components render the steps: **Start** (Follows, At a port's pixel,
  Next port; a fixture's must say where — nothing flows across fixtures)
  and, on containers, **Children** (End to end, or One per port from a
  port: absolute, so slots are kept). An **override** on an object, by
  id, replaces its rule's step field by field, and can turn it **Off**,
  give it a **Direction** or an **Offset**. The cursor continues: putting
  ring 3 on D1 takes ring 4 and the centre along. A leaf that doesn't fit
  its port starts on the next one; the first leaf after a Set that
  doesn't fit is reported and clipped; past the last port is an error
  until the next Set. The effective patch is derived and never stored;
  anything naming something gone is reported and never retargeted; two
  objects on the same pixels are allowed and reported as an **overlap**
  (the board shows the later one). `model/tests/walker.rs` is the
  walker's statements.
- **Patching by hand** (the roadmap's stage 4, plan
  `lp2025/2026-10-18-2337-explicit-patching-spike`; `patch_edits.rs`,
  `ux/patching.rs`), on overrides. Looking is free; a pair takes an arm.
  **A** arms a patch from what is selected: one object (or its span), or
  a gap (a span with nothing on it; a run of selected pixels, or a port's
  first gap, works the same). While armed, **← →** walk the other end's
  candidates (free segments in wire order from an object; objects not on
  the wire from a segment), the board previews the pair as if it were
  made, and **Enter** or a click makes it. **M** is lp2014's `m`: armed,
  it patches and moves on to the next free segment still armed; on an
  object just patched (and lined up), it moves on the same way. **Esc**
  disarms first. **S** arms a swap between two objects on the wire. On
  the selected objects: **R** reverses the object's direction while Move
  & draw is off (on, it reverses the path itself), **K L** turn one lamp
  and **; '** a third (direction-aware, as lp2014's keys were), **[ ]**
  shift a pixel along the port, **U** hands its start back to its rule.
  **⇧← ⇧→** walk the wire span by span, port after port; **.** jumps to
  the next port's first gap. On a fixture, **⇧U** turns every object off,
  for patching by hand from nothing. An edit takes what follows along;
  nothing is pinned behind a person's back.
- **The hardware preview** (plan D4's piece; `piece.rs`,
  `web/src/piece_view.rs`): the real lights as they are really wired, lit
  by what the board sends, so a hand patch can be walked with no hardware.
  Its wiring (the truth) is the project's patch as it opened, or for
  **Sean's eaves** the `eaves-wired` project (the house). **Debug →
  Scramble the hardware** shuffles the first output's objects across its
  ports, each a random way round (seeded). The lab's page docks it at the
  foot of the right column, under the inspector; the **Hardware preview**
  toggle hides it. Studio has none: there the hardware is real.
- **Port grouping** (D14): the toolbar's **Ports** switch draws each
  output card's pixels as **objects** (a run per span), **groups of N**
  (free pixels cut to the next object's size) or **pixels**. View state.
  The journeys, J1 (the dome's first box, panel by panel) and J2 (the
  eaves across D10 and D9), run as key-driven tests in
  `files/tests/journeys.rs`.
- **Edges are node to node** (D57, amending D45/D46): one per pair of
  nodes, violet, with an arrowhead where the data arrives. Core says
  which there are and what each says; the page routes them with
  `lab-arrows` from where the cards landed. On the canvas an edge is a
  curve from its source's right side into its target's left, on an output
  level with the port rows it lands on. Its channel's name is always on
  it in a small box, and nothing more; edges touching the selection are
  lit, the rest quieter. Where a fixture's objects land shows on the
  output instead, as each object's span, lit while the object is
  hovered or selected. Hovering an edge (the page hit-tests it and tells
  core) lights it and says what it carries in full; it never selects. In
  the stack only the selection's edges show, in the gutter (below).
  Opening a fixture (Enter) frames it with its outputs; ⇧2 frames the
  selection. A selected port rings the lamps it carries, and a selected
  object tints the ports it lands on.
- **Pixels** (D47): a port's pixels are squares on its row, laid out by
  the model (`pixel_grid.rs`), each selectable. Arrows walk them, ⇧-click
  selects a run from the first one picked. A selected pixel breathes on
  the board even with nothing patched to it, and the lamp on it is ringed
  on the canvas; select a lamp and its pixel is ringed on its port.
- **A simulated board** (D12, D16, D44): the lab is always attached to a
  board, and for now the model simulates it (the engine above). Every
  port row on an output card shows its pixels as the board has them.
  Selecting inside a fixture is a fixture override, so it shows on the
  lamps AND their pixels: everything else dims to a quarter; every
  object under the selection shows old LightPlayer's pattern from
  lp2014's `GenericShapeMapper` — a blue lead-in, a red lead-out, and
  between them black but for a white band sweeping start to end every
  two seconds; a lamp selected on its own breathes white. Selecting an
  output, port or pixel is an output override: its whole range breathes,
  patched or not (D13, D47), and no lamp changes. While you draw a path
  or circle, only the draft's lamps light, on the pixels it will take
  when it's finished. Lamps outside their fixture's box never light
  (D4). The page ticks at ~30 fps only while something moves. The seed's
  scarf is patched to "scarf out" (D10, 60 px), so drawing in it lights
  pixels at once.
- **Lamps in their control product's colour.** Every lamp on the canvas
  is drawn in its colour in its fixture's control product — never an
  output's override, so selecting a pixel rings its lamp and leaves its
  colour alone. A lamp patched to no pixel is drawn fainter; a lamp with
  no colour (outside its box, or nothing renders for its fixture) is a
  dim hollow dot. Faintly behind each fixture's lamps is the picture
  they sample.
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
- **Editing:** drag moves what's drawn (and pins a node, with pinning on), ⌫ deletes,
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
- **Move & draw** (D50): a toolbar toggle, named for what's on, and D.
  Off, nothing moves, pins, draws or reshapes (no handles, no tools, no
  counts or sizes that move lamps — `PropDesc::reshapes`); each try says
  "Move & draw is off · D turns it on". Selection, pixels, the board, a
  port's own count and patching still work — select and patch only, for
  the dome.
- **Drawing:** a path tool (P) and a circle tool (O). A path is
  clicked out point by point, like the product's: each click adds a
  point, the draft runs on to the cursor with its lamps lit on the board,
  double-click or Enter finishes, Esc takes back a point (and, with none
  left, leaves the tool). Its lamps are spread evenly along its length,
  one every 2.4 units. A circle is dragged out from its centre.
- **Square fixtures.** A new fixture is 100 × 100; the seed's are square
  too, sized to hold their drawing. Resize one by its handles, or set
  width and height in the inspector.
- **One tree and a data-driven inspector:** the tree lists modules,
  fixtures, outputs and ports, and a fixture's components; an object is
  a row only where it differs from its component — a circle's rings, a
  repeat's copies 1…n (copy 0 is the repeated component itself) — drawn
  lighter, and selectable. On the canvas a click selects the circle or
  repeat, the next the ring or copy under it. The inspector shows the
  properties everything selected shares, a fixture's rules and problems,
  and an output's ports with how full each one is. One component or
  object shows **Component · <id>** first — its properties, its Start and
  Children (for a ring or a copy, the circle's or the repeated thing's,
  with a note that edits change every one) — then **Object · <id>**: a
  ring's own properties, then where it is on the walk and why: Port,
  Pixel, Direction, Offset, On the wire. A value from a rule says what it
  is and where it came from ("after ring r2 ↗", "from circle1: one per
  port ↗") and Override makes it the object's own without moving
  anything; Reset hands it back; an object with a start of its own says
  what follows it. A multi-selection edits together and says mixed;
  every edit is one undo step, the same edit the keys make.
- **Verbs by name** (`graph_verb.rs`): every key the hint bar names is a
  `GraphVerb` (`delete`, `go-in`, `select-all`, `path-tool`, …), and each
  hint carries its verbs; `Editor::verbs` lists the ones on offer now from
  those same hints and `Editor::press` does what the key does, through the
  key handler. Studio publishes them as offers at `project/graph/<verb>`.
  Moving the view (pan, zoom, fit, the layout) is never a verb.
- **A hint bar** that says what every key does in the current state, each
  action in a word or two ("select", "go in", "delete"), never naming what
  it acts on — the breadcrumb says what's selected. A hint comes and goes
  only as its action does, so the bar stays still as you point around. Its
  lead says what you're doing ("Drawing a path", "Moving"), in a word or
  two.

## The page

The canvas container (architecture §8), bottom to top:

- **background dots** on a screen-space canvas, spaced in world units and
  stepping coarser (×5) as you zoom out;
- **the world layer**, one `div` whose transform is the camera, holding
  every node as a flat, keyed DOM card in world units. Panning and zooming
  change only that transform, so no card renders for a pan; cards out of
  view are not mounted; a relayout eases each card to its new place.
  **Faces:** a fixture you work in (it or anything in it selected, or a
  drawing tool out) is the mapping editor: its drawing in SVG, in its own
  units, with its lamps as dots on a canvas inside it. Any other fixture is
  Studio's live view, its lamps alone as cells of light (`lamp_cells.rs`,
  built as `lpa-mapping-editor`'s `point_cells` builds them, worked out
  once per layout and zoom). Lines in a card divide their width by the
  zoom, so they keep one width on screen. An output's rows are SVG with its pixels on a
  canvas. Lamps and pixels are rasterized in Rust and put with one
  `putImageData` (`dense_canvas.rs`), drawn again only when the zoom moves
  a step or the colours tick;
- **the overlay**, in screen pixels, never scaled: frames' title tabs (a
  frame is picked by its tab), the edges, outlines (each thing's area —
  what's outlined is what a click hits), handles, the selection box, a
  drawing in progress; and, as DOM, the right-click list and the note
  beside the cursor.

The page draws the view tree's parts, each a memo core keys on what it
depends on (`view_keys.rs`), so a pan builds only the overlay again.

**Two layouts** (architecture §5), switched in the toolbar (Canvas |
Stack) and remembered per browser, never in the project: the **canvas**,
which pans and zooms, and the **stack**, one column in tree order at a
fixed zoom (card text at 13 px) that scrolls natively, a module a rail
down its children's left, a fixture filling the column (drawn bigger
than its box, whole). Switching eases every card to its new place while
the world's own transform eases between the two, and the edges fade for
the move. In the stack the selection's edges run in a **gutter** at the
column's left, each on its own lane (`lab-arrows`' gutter route), and the
card the selection is in scrolls into view.

**Size classes** (architecture §7), from the app's width by Tailwind's
breakpoints: **lg** (≥ 1024 px) has the tree left, the inspector right
and the hint bar; **md** puts the tree in a drawer (Tree in the toolbar)
and the inspector over the canvas's right while something is selected;
**sm** (< 640 px) starts in the stack, with the inspector a bottom sheet
whose handle steps peek, half, full, and the hint bar one line.

**Debug**, at the toolbar's right end: Pinning (above), and Size (Auto,
sm, md, lg), which draws as if the screen were that size and frames the
app at 390 or 820 px wide, the way devtools' device mode does.

Not covered yet: touch gestures (pinch, long-press: the touch plan, on
the test phones), GPU faces at dome scale, saving rules and overrides as
themselves (patch format 4, M1), the output/port picker, choosing a node's channels, saving from the page,
routes, running GLSL (shaders are stand-in patterns), clocks and other
inputs to a shader, and a level above the project. In the lab the board
is simulated; in Studio's Graph tab it is the real one (or the preview
host's), and saving goes through Studio's own asset edits.
