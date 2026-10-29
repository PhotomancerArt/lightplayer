# lpa-graph-web

The Graph's web: the canvas, its tree, its inspector and its hint bar, as
Dioxus components a host mounts. The Graph is Studio's one project canvas
and its mapping editor; Studio mounts these in
`lpa-studio-web/src/app/workbench/graph_view.rs`.

This crate is the humble part. `lpa-graph` **decides** (what is selected,
lit, dimmed, shown, offered); this crate **draws** what it decides and says
what the pointer and the keys did. No editor logic and no detail decisions
live here: if a component wants to choose something, the choice belongs in
`lpa-graph`'s view tree.

It has no tests of its own. What it draws is pinned by `lpa-graph`'s
statements; how it looks is pinned by Studio's Graph stories:

```bash
just studio-story-pngs graph     # the Graph's stories, as local PNGs
just studio-dev                  # the real thing; add ?emu=tab for an emulated board
```

## The seam with the host

- **`GraphEditor`** is the page's hold on an editor it does not own.
  `read` borrows the editor and subscribes the component to its changes;
  `input` hands a `GraphInput` to the host's sink (`GraphSink`), which
  applies it. Studio's core owns the editor and applies every input
  synchronously, because a pointer move can't wait behind a device read.
- **Colours are the host's.** The host hands `Canvas` the board's frame.
  In Studio that is the board's own published frames; in a story, with no
  board, `lpa-graph`'s simulated board stands in.
- **Styles** are in `graph.css` (`GRAPH_CSS`), every rule scoped under an
  element with the `graph` class, so nothing leaks into the host's page.

The components: `Canvas`, `TreePane`, `Inspector`, `HintBar`, and
`InspectorDebug` for the Debug switches.

## The canvas

Bottom to top:

- **background dots** on a screen-space canvas, spaced in world units and
  stepping coarser as you zoom out (`dots_layer.rs`);
- **the world layer**, one `div` whose transform is the camera, holding
  every node as a keyed card in world units (`world_layer.rs`,
  `node_card.rs`, `faces/`). Panning and zooming change only that
  transform, so no card draws again for a pan; cards out of view are not
  mounted; a new layout eases each card to its new place. A fixture you
  work in shows its drawing in SVG with its lamps as dots; any other
  fixture is Studio's live view, its lamps as cells of light
  (`lamp_cells.rs`). Lamps and pixels are rasterized in Rust and put with
  one `putImageData` (`dense_canvas.rs`);
- **the overlay**, in screen pixels, never scaled: frames' title tabs, the
  edges, outlines (what's outlined is what a click hits), handles, the
  selection box, a drawing in progress, the right-click list and the note
  beside the cursor (`overlay.rs`, `edge_layer.rs`).

The view tree is drawn part by part, each a memo keyed on what it depends
on (`lpa-graph`'s `view_keys.rs`), so a pan builds only the overlay again
(`view.rs`).

Edges are drawn with `lpa-graph-arrows` from where the cards landed: a
curve from the source's right side into the target's left, its channel's
name in a small box on the line. The page hit-tests them and tells the
editor which one is hovered.

## Layouts and sizes

- **Two layouts**, switched in the toolbar (Canvas | Stack) and remembered
  per browser, never in the project: the **canvas**, which pans and zooms,
  and the **stack**, one column in tree order at a fixed zoom that scrolls
  natively. In the stack the selection's edges run in a gutter at the
  column's left, each on its own lane.
- **Size classes**, from the app's width: wide has the tree left, the
  inspector right and the hint bar; medium puts the tree in a drawer and
  the inspector over the canvas while something is selected; narrow starts
  in the stack, with the inspector a bottom sheet and the hint bar one
  line.
- **Debug**, at the toolbar's right end: pinning, a forced size, how the
  inspector shows a start, and the patcher's own steps. Remembered per
  browser. The two inspector switches keep the browser storage keys they had
  in the mapping lab (`inspector_debug.rs`), so nobody's settings reset.
