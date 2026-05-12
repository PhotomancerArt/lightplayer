# Milestone 6: Semantic editor — TOML side-by-side, rich widgets, composition, file tree

## Goal

Promote the editor from "tweak in-memory params and re-render"
to a real authoring tool: TOML side-by-side editor (raw text
view alongside the widget panel, kept in sync), rich Slot
widgets (color picker, log slider, dial), composition widgets
(struct / array Slot UI), file tree refinements (multi-file
open, rename, delete, create), and persistence of param edits
+ bindings back to the underlying TOML files.

After M6, an artist can open a Pattern, tweak its colors via a
proper picker, save back to TOML, refresh, and see the saved
state restored.

Palette and gradient authoring stays recipe-based: widgets edit TOML
recipes, lpfx rebakes width-by-one runtime textures and writes them to
`params` struct fields, and the editor persists the recipe rather than
baked pixel buffers.

## Suggested plan location

`docs/roadmaps/2026-04-23-lp-render-mvp/m6-semantic-editor/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

**In scope:**

- **TOML side-by-side editor** in
  `lp-app/lp-studio/src/components/toml_editor.rs`:
  - Raw `<textarea>` (or a thin code-editor component — design
    phase decides between rolling our own and bundling
    monaco/codemirror) alongside the widget panel.
  - Edits in widgets reserialize the TOML; edits in the TOML
    re-parse and refresh widgets. Two-way sync.
  - Parse errors shown inline in the TOML pane; widget pane
    shows last-good state.
  - "Save" button writes the current TOML to the virtual fs
    (which auto-persists to localStorage via M2's
    `LocalStorageFs`).
  - "Revert" button reloads from fs.
- **Rich Slot widgets** added to `lp-app/lp-studio-widgets/`:
  - `color_picker.rs` — `ColorPicker` for `Kind::Color`. RGB +
    alpha, hex input, swatch grid for common values.
  - `palette_editor.rs` / `gradient_editor.rs` — recipe editors
    that produce height-one texture resources at runtime. They
    edit stops/entries in TOML and trigger lpfx rebakes; they do
    not serialize baked texture bytes.
  - `log_slider.rs` — `LogSlider` for slots with `presentation
    = { ui = "log_slider" }`. Maps a linear UI position to
    log scale of the underlying value.
  - `dial.rs` — `Dial` for slots with `presentation = { ui =
    "dial" }` (used for `Phase` etc.). Circular drag input.
  - Update `widget_for_slot` to dispatch to these based on
    `Constraint::Choice<Color>` and `Presentation` hints.
- **Composition widgets**:
  - `composite.rs` — UI for struct slots (the slot grammar
    supports nested slots; M3–M5 only handled scalars). Renders
    each child slot recursively, indented.
  - Array slot UI (when present in the model). Add/remove/
    reorder rows, each row is a sub-Slot.
  - These are the **deepest tests of the Slot grammar shape**.
    Surface any awkwardness here; lp-domain tweaks land back
    in `lp-domain` directly (M4–M6 of lp-domain are still
    deferred).
- **Persistence of edits**:
  - Param tweaks via widgets update `params` field values and
    reserialize into the in-memory TOML buffer (the side-by-side
    editor is the source of truth).
  - Palette/gradient widget edits reserialize their authoring
    recipes and invalidate the corresponding runtime resource
    texture so the next preview samples the rebaked texture via
    `params.palette` or `params.gradient`.
  - Bindings (`bind = { bus = "..." }`) added via the M5
    "bind to channel" affordance also serialize — the widget
    panel writes through to TOML.
  - Save button commits to fs.
  - Auto-save on blur / N seconds is a polish concern that
    can land here or be deferred — design phase decides.
- **File tree refinements** in
  `lp-app/lp-studio/src/pages/files.rs`:
  - Real tree view (not flat list).
  - Right-click / context menu: rename, delete, duplicate,
    create new file (with template picker — empty Pattern,
    empty Effect, empty Stack).
  - Drag-to-reorganize (stretch).
  - File icons by extension.
- **Multi-artifact open**:
  - Tabbed editor: open multiple files at once, switch
    between them, each tab maintains its own preview state.
  - "Close" tab.
  - Unsaved-changes indicator on tab title.
- **Tests**:
  - Round-trip: load Pattern → edit via widget → reserialize
    TOML → re-parse → struct equality.
  - Rich widget rendering (showcase entries for color picker,
    log slider, dial).
  - Composite widget rendering (showcase entries for struct
    slots and array slots).
  - File tree CRUD operations against `LocalStorageFs`.

**Out of scope:**

- Server-side persistence (still local-only).
- Real-time collaboration.
- Undo/redo across the whole app (per-textarea undo via the
  browser is fine).
- Visual node-graph editor for Stacks (drag + connect — later).
- Authoring brand-new Pattern shaders (GLSL editor with syntax
  highlighting / completion) — M6 lets you edit the GLSL via
  the raw TOML textarea, but a proper shader editor is later.
- Binary asset (texture, audio) management — text-only artifacts in
  this roadmap. Generated palette/gradient runtime textures are
  allowed because TOML stores recipes, not baked bytes.

## Key decisions

- **TOML is the canonical form, widgets are a view.** When
  widgets and TOML disagree mid-edit, TOML wins (re-parses on
  blur, widgets refresh from parsed state). This avoids
  divergence.
- **Save is explicit (not autosave) in M6 baseline.** Autosave
  is a polish concern; design phase decides whether to add it.
  Explicit save makes parse errors recoverable (you can fix the
  TOML before committing).
- **Composition widgets are where lp-domain shape gets stress-
  tested.** This is the milestone where the Slot grammar's
  composition story (struct, array) actually has to drive UI.
  Awkwardness surfaces here; tweaks land in lp-domain
  directly. Plan that lp-domain edits during M6 are
  expected.
- **Code editor: roll-our-own `<textarea>` first, monaco/
  codemirror later if needed.** A textarea + serde error
  display is enough for M6. Bundling monaco bloats the wasm
  budget; codemirror is lighter but still significant. Defer
  unless needed.
- **Rich widgets are added incrementally** — color picker,
  palette editor, gradient editor, log slider, dial are the
  identified widgets from the example corpus, `Kind`, and
  `Presentation` types. New widgets land as new
  Constraint/Presentation types appear.
- **Palette/gradient persistence is recipe persistence.** The
  source of truth remains TOML. Runtime texture allocation,
  baking, and invalidation are preview/runtime concerns and must
  not force localStorage to carry binary blobs.
- **Multi-tab open is a real UX requirement, not polish.** As
  soon as you're editing a Stack and want to tweak its referenced
  Pattern, you need both files open. M6 lands tabs.

## Deliverables

- `lp-app/lp-studio/src/components/toml_editor.rs`.
- `lp-app/lp-studio-widgets/src/color_picker.rs`.
- `lp-app/lp-studio-widgets/src/palette_editor.rs`.
- `lp-app/lp-studio-widgets/src/gradient_editor.rs`.
- `lp-app/lp-studio-widgets/src/log_slider.rs`.
- `lp-app/lp-studio-widgets/src/dial.rs`.
- `lp-app/lp-studio-widgets/src/composite.rs` (struct + array
  Slot UI).
- Updated `widget_for_slot` dispatch.
- Persistence wiring (widget edits → in-memory TOML →
  `LocalStorageFs`).
- File tree refinements (CRUD, templates).
- Multi-tab editor.
- New showcase pages for rich + composite widgets.
- Round-trip tests.
- Re-bake/invalidation tests for palette/gradient recipe edits.

## Acceptance smoke tests

```bash
cargo build -p lp-studio -p lp-studio-widgets
dx build --release  # both crates

cd lp-app/lp-studio-widgets && dx serve --example showcase
# → showcase has new pages: color picker, palette editor,
#   gradient editor, log slider, dial, struct slot, array slot
#   — all variants visible

cd lp-app/lp-studio && dx serve
# → open psychedelic.stack.toml in tab 1
# → open the referenced fbm.pattern.toml in tab 2 (multi-tab)
# → tweak fbm's colors via color picker
# → see the change reflected in the TOML pane and in tab 1's preview
# → tweak a gradient/palette recipe, preview rebakes the strip
#   and samples the new texture without storing binary bytes
# → save → refresh → tab 1 + tab 2 restored, edits persist
# → introduce a TOML parse error → widget pane freezes on last good
#   state, error shown inline → fix → widgets re-enable
# → right-click in file tree → "new pattern" → template appears
```

## Dependencies

- M5 complete (bus + bindings — needed for binding-aware
  serialization).
- All previous milestones' tests still pass.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: TOML editor + 3 rich widgets + composite widgets
(struct + array — these are the deepest Slot-shape stress test
in the roadmap and likely surface lp-domain tweaks) + persistence
flow + multi-tab editor + file tree CRUD. Largest editor-side
milestone. Phaseable: TOML editor + persistence as one phase,
rich widgets + showcase entries as another, composite widgets +
lp-domain tweaks as a third, file tree + multi-tab as the
fourth.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
