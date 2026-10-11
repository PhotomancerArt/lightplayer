# Mapping lab

A design lab for rebuilding LightPlayer's mapping and patching editor from
the ground up. It sits outside the app on purpose. The vision is in the
planning workspace: `lp2025/2026-10-10-1853-mapping-design-lab/vision.md`.

```bash
just lab-mapping        # serves the canvas; the printed URL is the source of truth
cargo test -p lab-mapping-model
```

## The two crates

- **`model/`** (`lab-mapping-model`): the editor's whole behaviour. Fixture,
  components, objects, selection, properties, hints. It has no IO, and its
  input is events. Yona reads and edits this crate, so keep it small and
  plain. **It depends on no workspace crate.** That keeps it abstract and
  lets it drop into the app later.
- **`web/`** (`lab-mapping-web`): a Dioxus page that forwards events to the
  model and draws what it says. No editor logic lives here.

## Statements

`model/tests/statements.rs` holds the behaviour as sentences, one test
each. The doc comment is the statement. To change how something behaves,
change the sentence first, then the code.

## What the first piece covers

A basic 2D canvas with selection:

- **Drill-down selection:** click selects the outermost thing, clicking
  again goes in, ⌘-click selects the lamp, Esc goes up, Enter goes down,
  and the arrow keys move between siblings.
- **Multi-selection:** ⇧-click adds or removes, and a box selects. The box
  takes what it touches, or with ⌥ only what is fully inside.
- **⌘A climbs** a level each time you press it, so ⌘A ⌫ always empties the
  fixture.
- **Editing:** drag moves things, ⌫ deletes, with refusals in rustc style.
  − and = change a count, r reverses direction, and ⌘Z undoes.
- **Drawing:** a line tool (L) and a circle tool (O). The lamp count follows
  the length you drag.
- **One tree and a data-driven inspector:** the inspector shows the
  properties everything selected shares, and edits all of them at once.
- **A hint bar** that says what every key does in the current state.

Not covered yet: devices, outputs and patching.
