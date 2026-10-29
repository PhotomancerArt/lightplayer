# lpa-graph

The Graph's editor model. The Graph is Studio's one project canvas and its
mapping editor: every node of a project on one canvas, and every fixture's
drawing and patching inside it. A project's tabs are Nodes · Graph.

This crate is the part that **decides**. `lpa-graph-web` is the humble web
that **draws** what it decides. Studio's core holds the editor, offers its
verbs and saves its edits (`lpa-studio-core`'s `app/graph/`).

```bash
cargo test -p lpa-graph          # the statements
```

To see it: `just studio-dev` and open a project's Graph tab (add `?emu=tab`
for an emulated board in the page), or the Graph's stories,
`just studio-story-pngs graph`.

## Three layers

- **p1, the domain** (`src/domain/`): what is stored, and the logic over
  it. The project is the root module; its nodes are modules, shaders,
  fixtures, outputs, and plain cards for anything else (a clock, a
  button). Bus channels and bindings are the wiring between them. A
  fixture's mapping is `lpc-mapping`'s own `part::Mapping`: its parts are
  rendered by `part::render` and patched by `patching::patch_mapping`, the
  engine's patcher, so the lamps and where they land are the board's, bit
  for bit. The effective patch is derived, never stored. `board.rs` is a
  small simulated board, used only where there is no real one (a story, a
  drawing in progress).
- **p2, the editor** (`src/ux/`): the editor's state and every user verb.
  Selection, picks, handles, properties, hints, the inspector's rows, the
  tree, problems in words, and `Editor::view_tree()`, which says
  everything the page draws as one tree with **no positions**
  (`UiCanvasView`). It never lays anything out: it builds the layout's
  input, the page runs `lpa-graph-layout`, and the placement comes back as
  a fact. It has no IO and never reads a clock; its input is events
  (`GraphInput`), and the page passes the time in.
- **p3, the web** (`lpa-graph-web`): draws the view tree and reports
  pointer and key facts back. No editor logic lives there.

Keep this crate small and plain: it is the one people read to learn what
the Graph does.

It depends on `lpa-graph-layout` and on `lpc-mapping` with its `edit`
feature (the part-tree edits). No firmware builds this crate.

## Verbs and offers

Every key the hint bar names is a `GraphVerb` (`delete`, `go-in`,
`select-all`, `path-tool`, …). `Editor::verbs` lists the ones on offer now,
from the same hints, and `Editor::press` does what the key does. Studio
publishes each as an offer at `project/graph/<verb>`, so a key, an
inspector row and the app agent make the same edit by the same function.
Moving the view (pan, zoom, fit, the layout) is never a verb.

Verbs that take values have typed parameters and a `subject` (the node,
part or fixture they act on). A `GraphEdit` is one of them with its values
bound; `Editor::edit` applies a list of them as one undo step, and every
change to a mapping goes through `lpc_mapping::edit`. A verb is offered
whenever something can take it; the selection only preselects its subject.

- **Patching** (`graph_edit.rs`, `graph_verb_choices.rs`): `set-port`,
  `set-starts-at`, `set-direction`, `set-offset`, `turn-off`, `turn-on`,
  `back-to-the-flow`, `set-flow`, `set-fixture-start`, `set-part-start`,
  `set-output`, `remove-override`. Choosing a port or an output with no
  name names it, with the lowest free number, in the same undo step.
- **Layout** (`graph_layout_edit.rs`, `layout_verb_choices.rs`): `rename`,
  `repeat`, `unrepeat`, `expand`, `set-count`, `add-ring`,
  `add-centre-lamp`, `remove-ring`, `set-split`, `set-drawn-from`,
  `set-radius`, `set-way-round`, `set-lattice`, `set-routing`,
  `set-fixture-size`, `set-colour-order`, `set-port-lamps` and `delete`.
  A rename's or an expand's references are rewritten by the model's rules,
  and the editor says what was kept and what went ("1 override on
  repeat1's copies is kept; it shows as a problem").

Drawing stays a gesture, not a verb.

## The inspector

The inspector is drawn from rows the editor builds. For a segment's start,
each value says where it came from: "follows ring r2", "from circle1's
flow: one per port", "from the fixture's start", or "set here". A value set
here carries **Back to the flow**, which hands it back to its flow
(`start_words.rs`). Problems are plain sentences naming their segment or
port, such as "ring r5 doesn't fit on port D1: it lands as far as the port
goes" (`problem_words.rs`). An override whose node is gone carries Remove.

## Keys

The hint bar always says which of these do something right now
(`hints.rs`); the keys themselves are `Editor::key_down` (`editor.rs`)
and, for patching, `patching.rs`.

| Key | What it does |
|---|---|
| click · ⇧ click | select · add or remove |
| ⌘ click · ⌥ click | select the deepest thing · the next one under the cursor |
| click again · Enter | go in |
| Esc | go up, or deselect; cancels a gesture or a drawing point first |
| ← → (↑ ↓) | previous · next sibling |
| ⌘A | select all, a level further out each press |
| right-click | everything under the cursor, and what can be done there |
| drag | move what is drawn, or a handle; on empty space, box-select (⌥: only what is fully inside) |
| ⌫ | delete |
| − = | fewer · more (a count, a port's pixels) |
| R | reverse (Move & draw on: the path itself; off: its placement) |
| J | the segment under the cursor becomes a jumper, or lit again |
| D | Move & draw on or off |
| V · P · O | select tool · path tool · circle tool |
| A | arm a patch from the selection |
| ← → (armed) | walk the candidates |
| Enter · click (armed) | make the pair |
| M | patch and move on to the next free segment |
| S | arm a swap between two segments on the wire |
| K L | turn one lamp |
| ; ' | turn a third |
| [ ] | shift a pixel along the port |
| U | back to the flow |
| ⇧U | everything off: every segment of the fixture off the wire |
| ⇧← ⇧→ | walk the wire, span by span, port after port |
| . | the next port's first free segment |
| ⌥U | unpin (with pinning on, under Debug) |
| ⌘Z · ⌘⇧Z | undo · redo |
| ⇧1 · ⇧2 | fit · frame the selection |
| scroll · space-drag | pan |
| ⌘ scroll · pinch | zoom |

While a patch or swap is armed, any other key disarms and does what it
always does.

## Statements

The behaviour is written as sentences, one test each: the test's doc
comment is the statement. To change how something behaves, change the
sentence first, then the code.

- `tests/statements.rs`: the editor, on the desk project
  (`Project::seed()`).
- `tests/tree_rows.rs`: the tree pane's rows and the targets the keys walk.
- `tests/inspector.rs`: the inspector, ports as rows of spans, problems in
  words.
- `tests/patcher_answer.rs`: the Graph shows the patcher's answer.
- `tests/patching.rs`: patching by hand.
- `tests/layout_verbs.rs`: the layout verbs, made as an offer makes them.

The journeys on real projects (the choker, the circular display, a dome
slice, Sean's eaves) are in `lpa-graph-files/tests/`. The patcher's own
rules are tested in `lpc-mapping`.
