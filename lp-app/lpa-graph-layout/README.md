# lab-layout

Layout for the mapping lab: **a semantic graph of nodes in, placements
out.** Pure Rust, no IO, no dependencies at all.

Where cards sit is the view layer's job (architecture §5, D63): core says
what there is (each node's role, size and parent, the dataflow), and the
page runs this crate and draws the result. It is the layout piece of the
lab's view model, the way `lab-arrows` is the arrow piece.

```bash
cargo test -p lab-layout        # the statements
```

## Input and output

- **`LayoutInput`**: every **`LayoutNode`** (an id, its parent module or
  `None` at the root, a **`LayoutRole`**, and its size in world units),
  the dataflow as `(from, to)` pairs, and the **pins**. Ids are strings, so
  the crate needs no model types. It is owned and `PartialEq`, so a caller
  can keep the last one and lay out again only when it changes.
- **`Placement`**: every node's `Rect` in world units (the root's inside
  at `(0, 0)`), where each module's inside starts (what a pin there is
  measured from), and the extent around it all.
- The spacing is in `spacing` (`NODE_GAP`, `COLUMN_GAP`, `MODULE_PAD`,
  `MODULE_PAD_TOP`, the stack's `STACK_INSET`, `STACK_GAP`,
  `STACK_HEADER`), so callers and statements can name it.

## The canvas layout: by role

`canvas_layout` doesn't discover the graph's shape: LightPlayer knows what
a node is from its kind, and the catalog is nearly all one shape (clock →
shader → fixture → output, in one module). So it is a short set of rules.
Inside every container, the root and then each module, inside-out:

1. **Pins** first, where they were pinned; a pin inside a module never goes
   above or left of its inside.
2. **Controls** (clock, power, button, radio) in a row along the top.
3. Below them, **columns that follow the flow**: sources (shaders,
   textures, playlists), then fixtures, then outputs. Fixtures go in the
   order of the first source feeding them, so lines cross as little as the
   columns allow.
4. **Each output sits level with the fixture feeding it**, or with the
   middle of several; outputs push down to clear one another, keeping the
   order of what feeds them.
5. **Modules** stack below the pipeline, each a frame: its children's box
   plus padding, with room above for their title tabs.
6. **Free nodes** (anything else) in a column at the right.

Whatever is in a node's way moves it down until it's clear, so nothing at
one level overlaps anything else at that level.

## The stack layout

`stack_layout(input, width)` puts every node in one column in tree order,
as Studio lays out its cards: a module's children right after it, inset a
step per level, its box covering them (for a rail down its left). A
fixture keeps its box's shape at the column's width; every other node
keeps its height. Pins don't apply.

## Stable first

People keep a mental map of where things are (Misue et al., 1995, on
preserving it in graphs that change). An automatic layout that reshuffles
on every edit wrecks that, which is why most authoring tools are placed by
hand. So, in order:

1. **Stable.** The same input always lays out the same way (nothing reads
   a clock, a hash map or a random number), and a small edit moves things
   little and locally: adding a fixture moves nothing in the columns to its
   left; adding a node inside a module moves nothing in the pipeline
   outside it, and what sits below the module moves down as one.
2. **Animated:** the page eases every move (architecture §5.3).
3. **Good:** few crossings, compact, outputs beside their fixtures.

## Pins: the door left open

The lab is trying automatic layout only (D64): pinning is behind a debug
flag, and every reach for a drag is logged as a finding. Pins are an input
from the start, as fixed points everything else lays out around, so if the
trial says hand placement is needed, it is constraints added, not a
rewrite.
