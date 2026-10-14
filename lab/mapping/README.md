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
  its edges, selection, properties, hints. It has no IO, and its
  input is events. Yona reads and edits this crate, so keep it small and
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
  output from outside, one per object and port from inside a fixture.
  Opening a fixture (Enter) frames it with its outputs; ⇧2 frames the
  selection. A selected port rings the lamps it carries, and a selected
  object tints the ports it lands on.
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
- **Drawing:** a line tool (L) and a circle tool (O). The lamp count follows
  the length you drag.
- **One tree and a data-driven inspector:** the tree lists modules,
  fixtures, outputs and ports. The inspector shows the properties
  everything selected shares, a fixture's rules and problems, and an
  output's ports with how full each one is.
- **A hint bar** that says what every key does in the current state.

Not covered yet: rules other than auto, choosing a rule's output, routes,
hand-placed nodes, other node kinds (patterns), and a level above the
project.
