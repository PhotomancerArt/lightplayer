# Mapping lab

A design lab for rebuilding LightPlayer's mapping and patching editor from
the ground up. It sits outside the app on purpose. The vision is in the
planning workspace: `lp2025/2026-10-10-1853-mapping-design-lab/vision.md`.

```bash
just lab-mapping        # serves the canvas; the printed URL is the source of truth
cargo test -p lab-mapping-model
```

## The two crates

- **`model/`** (`lab-mapping-model`): the editor's whole behaviour. Nodes
  (modules, fixtures, outputs), components, objects, layout, the patch and
  its edges, selection, properties, hints, and the simulated board. It has
  no IO, and its input is events; it never reads a clock, so the page
  passes the time in. Yona reads and edits this crate, so keep it small and
  plain. **It depends on no workspace crate.** That keeps it abstract and
  lets it drop into the app later.
- **`web/`** (`lab-mapping-web`): a Dioxus page that forwards events to the
  model and draws what it says. No editor logic lives here.

## Statements

`model/tests/statements.rs` holds the behaviour as sentences, one test
each. The doc comment is the statement. To change how something behaves,
change the sentence first, then the code.

## What it covers

A 2D canvas with selection, and the start of the whole node graph on it
(vision D36, D42–D44):

- **Nodes on one canvas.** The project is the root module. Modules are
  frames around nodes, fixtures are frames around a drawing, and outputs
  are cards with one row per port. Frames are see-through: a click inside
  one selects what's in it, and the frame itself is picked by its title or
  border. An output card is not a frame: click it to select it, click again
  for the port under the cursor.
- **Automatic layout** (D37), inside-out: in every container the fixtures
  and modules stack on the left and the outputs sit on the right, each at
  the height of the first fixture patched to it.
- **The auto patch rule.** A fixture's lamps fill an output's ports in
  order, after earlier fixtures on the same output. The effective patch is
  derived and never stored. A rule whose output was deleted shows as a
  problem and is never retargeted.
- **Edges are the patch** (D42): violet, one bundled edge per fixture and
  output from outside, one per object and port from inside a fixture,
  with an arrowhead where the data lands. Hovering an edge lights it and
  says what it carries ("tail → GPIO 10 · lamps 0–49 at pixel 0"); it
  never selects. Opening a fixture (Enter) frames it with its outputs; ⇧2
  frames the selection (an empty fixture with the outputs its rules name).
  A selected port rings the lamps it carries, and a selected object tints
  the ports it lands on.
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
  D13). While you draw a path or circle, only the draft's lamps light, on
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
- **Editing:** drag moves what's drawn, ⌫ deletes, with refusals in rustc
  style. − and = change a count (a port's pixels too), r reverses
  direction, and ⌘Z undoes.
- **Handles** (`handle.rs`): small squares on the selection — one on each
  point of a selected path, one on each ring of a selected circle (its
  radius), and a selected fixture's corner and right and bottom edges
  (its size; ⇧ keeps it square). Only a drag grabs a handle; a click on
  one is a click on what's under it. Dragging changes geometry, never a
  count: a path keeps its lamps and respaces them, and the cursor note
  says so. One drag, one undo.
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

Not covered yet: rules other than auto, choosing a rule's output, routes,
hand-placed nodes, other node kinds (patterns), a level above the
project, and a real board (the simulated one stands in until the lab moves
into Studio).
