# Studio UI Style

The principles behind these words and looks, with examples, are in
`docs/ux/README.md`.

Studio UI should be shaped around what the user is doing, not around the
internal shape of the data.

## Less Is More

The default rule is to show nothing we do not have to show.

Every visible label, border, icon, heading, badge, and metric competes with the
thing the user is trying to understand or change. Add UI only when it improves
orientation, comparison, decision-making, or action.

When in doubt:

- prefer fewer labels;
- prefer one strong focal surface over several decorated containers;
- prefer quiet inline text over badges;
- prefer whitespace over panel chrome;
- remove explanatory copy once the interaction itself is clear.

## Avoid Data-Shaped Nesting

Do not add a visual container every time the underlying model has another
object, enum, record, slot root, or view node. Deep nesting makes the UI feel
like a schema browser instead of a tool.

Prefer the shallowest presentation that preserves meaning:

- If a node has one primary produced visual, show the visual directly.
- If a section has one child, do not wrap it in an extra titled box just to
  mirror the data structure.
- If several technical facts describe one thing, prefer a single quiet caption
  over nested labels and badges.

Extra borders, cards, and panels should indicate a meaningful interaction or
separate workspace, not merely another layer of data.

## One Concept, One Frame

A visible frame should usually mean one user-facing concept:

- A node window frames a node.
- A product preview frames the image, control strip, or other produced output.
- A modal frames a temporary focused task.

Avoid putting a framed product inside a framed presentation section inside a
framed node unless each frame has an obvious job from the user's perspective.

## Progressive Technical Detail

Main UI should present the useful surface first. Debug panes, source panes,
inspectors, and tooltips can expose exact internal detail.

For example, main node UI can show:

```text
output visual 128 x 72
```

The debug pane can show:

```text
state.output = ProductRef::Visual(node=8, output=0)
revision = 102
slot root = node.8.state
```

The user can still inspect the system precisely, but the everyday view stays
calm.

## Details On Demand

Technical detail should usually be available, not always visible.

Prefer a small details affordance, inspector, source tab, debug tab, popover, or
tooltip over permanently showing implementation facts in the main surface. A
details affordance is useful when the information is sometimes important but
would distract from the normal workflow.

Good candidates for details-on-demand:

- revision numbers;
- slot roots and exact slot paths;
- internal product references;
- binding resolution details;
- source file locations;
- transport/probe diagnostics;
- raw serialized values.

Main UI can show the edited or observed value. Details UI can show why it has
that value, where it came from, and how the runtime represents it.

## Icons

Use icons as semantic affordances, not decoration.

Prefer shared Studio icon components over ad hoc glyphs, emoji, text badges, or
CSS-drawn symbols. Keep common icon sizes stable so live values do not change
button, header, or row dimensions.

Status must never rely on color alone. Pair tone with a distinct icon or shape,
and make the details available from the same trigger when the status matters.

Use text labels for primary meaning. Icons should speed recognition, mark a
compact control, or clarify repeated action types; they should not make the user
guess.

## Color & Light

Studio's palette is the Aurora direction: violet-tinted graphite surfaces with
the spectrum reserved for interaction, not decoration
(`docs/adr/2026-08-30-studio-design-language-aurora.md`).

**There is no accent hue.** The accent reckoning (D1, 2026-08-30) retired the
mint accent outright: at rest, chrome is neutral — saturated color belongs to
artwork, status, and interaction light. What used to reach for accent now
picks its role:

- **Actions** are neutral chips whose interaction answer is the spectrum:
  the outline CTA (`outline_action_class`) and the `InlineButtonTone::Action`
  default are bright-neutral at rest and take the iridescent hover ring; the
  one loud fill is the gradient Primary.
- **Links** are neutral at rest — muted text, underline, brightening to the
  strong neutral on hover (`markdown_text`, `help_link`).
- **Selected / current / you-are-here** wears the selection grammar (below):
  a static spectrum line for you-are-here nav, a static spectrum ring for a
  chosen object, over the neutral selection family's fill — never a status
  hue.
- **Authored values** (knob arcs, fader fills, sliders, the tape playhead)
  are the bright neutral `--studio-color-text-strong`: the artwork and the
  status families glow, the control doesn't.
- **Progress** is the iridescent fill vocabulary, never a flat colored bar.

Do not reintroduce an accent token; a hard spot argues a per-surface
exception at a review gate, never a blanket revival.

Status hues never move, and chrome may never borrow them. Each family means
one thing and nothing else may wear its color: violet is a bus/binding
relationship, amber is an unsaved edit, orange is device/roster attention,
gold is "your hand is on this control right now" (engaged), blue is live,
sage is export, lavender-grey is example provenance ("you are viewing a
read-only example"), green is good/valid, and diagonal stripes mark error or
debug surfaces. A new feature that wants "a color" should reach for a
neutral tone or the spectrum before it reaches for a status hue — status
hues are load bearing, and test-enforced (see `inline_button.rs`'s
`violet_stays_the_binding_convention`).

**The selection grammar** (spike `spikes/spectrum-selection`, ruled
2026-08-30): selection and navigation are separate concepts and never share
a mark.

- **Nav "you are here"** is a STATIC spectrum line on the nav axis's edge:
  the full-rainbow underline on the view tabs and the site chrome's nav
  (`ux-here-line-x` grammar), the cool-sweep side line on vertical navs —
  the story-book nav (`ux-here-line-y`, `--studio-spectrum-cool`).
- **Object selection** is a STATIC spectrum ring around the chosen thing
  (`ux-sel-ring`): option cards, the workbench tree's focused row, and the
  Map/Patch fixtures tree, output headers, and port cells (one
  `UiSelection`). Small radii take the cool variant (`ux-sel-ring-cool`) —
  the full sweep compresses to its warm stops there and reads as
  attention-orange; clipping hosts add `ux-sel-ring-inset`.
- **Intensity scales with size**: large marks carry the full spectrum,
  small marks the cool sweep, so the warm stops never sit beside the
  amber/red status tints they resemble.
- On rows and cells the ring is the ONLY selection paint — no grey wash
  (G1: "the rainbow highlight does well enough on its own"), so a selected
  row keeps its natural ground: transparent when clean, its dirty tint
  when edited. Option cards keep the low-alpha selection wash and the
  neutral check badge under their ring; the neutral selection family
  (`--studio-color-selection-*`) survives there and in span markers.

Every selection mark is static; the pointer answers in its own light: the
iridescent ring (`ux-ir-ring`) on card/button hover, the press flare
(`ux-press-flare`) on `:active`, and the pinned ring plus a lifted shadow
(`ux-drag-chip`) mid-drag. Dense rows go QUIET instead (`ux-row-edge`): a
plain white outline on hover — no shadow, no wash — that flashes into the
static spectrum hairline on press, previewing the selection ring the
click lands. A selected row and a row you're merely passing over never
share a paint.

Multi-color is a moment, not a wall. The spectrum belongs on hover/press/drag
feedback, the selection grammar's static marks, the brand mark, and the hero
— never on resting chrome, and never on a status-toned control (a status color already
answers "what is this," and a rainbow sweeping across it would contradict its
own meaning). The four "extras" this refresh shipped are the standard
vocabulary for working/progress/drag/press moments and are not optional
flourish: the conic spinner (`ux-conic-spinner`) for in-flight work, the
iridescent progress fill (`ux-iri-fill`) for determinate/indeterminate bars,
the drag ring for a lifted card, and the press flare for every button's
`:active` state. Reach for these rather than inventing a new spinner or
progress treatment.

Glass (`--studio-glass-*`, `ux-glass-panel`) is for overlays only —
popovers, sheets, card bars, anything that floats above the canvas.
`backdrop-filter` promotes its own compositing layer, and glass under glass
reads as mud, not depth; resting surfaces (panes, cards at rest, the
sidebar) stay opaque. The merged-outline popover's chrome carries an
additional decorative spectrum edge (a second, low-alpha SVG stroke) on
quiet/neutral chrome only — a status-toned popover keeps its pure semantic
stroke, because a status border is meaning and the spectrum edge is only
ever decoration layered on top of a border that has none to give up.

Structural contrast is a floor, not a target: borders need at least ~1.8:1
contrast against the surface they sit on, and a surface that reads as
"raised" needs at least one visible lightness step over what's behind it.
A border or a raised panel that vanishes into its background is a bug, not
a quiet aesthetic choice.

Every interactive control gets the `--studio-color-focus-ring` treatment
(`ux-focus-ring`, or the bare-element `:focus-visible` rule for text
inputs). Keyboard focus is not optional polish — a control a mouse can
reach that a keyboard user can't see focused on is broken for that user.

Buttons are `ActionButton` or `InlineButton`. A hand-rolled
`rounded+px-+hover:` class string on a new button-shaped control is a
review flag: check whether one of the existing solid/quiet/menu-item/
outline/inline-link helpers in `action_button.rs` already says what you
mean before writing a new one.

## Stable Layout

Dynamic values should not cause page reflow in normal operation.

Status, errors, metrics, revisions, probe results, frame counts, and other live
runtime data should fit inside reserved space, truncate, scroll, or move into a
details surface instead of changing component height or pushing nearby UI around.
This is especially important in node windows, device panels, tables, toolbars,
and other surfaces users scan repeatedly.

Acceptable layout changes are mostly tied to explicit local user action, such as
switching a tab, expanding a details panel, or opening a popup. Remote changes,
like another user editing a project or a device changing state, should preserve
the current reading surface as much as possible.

## The board card

A board is one card: on the home page, in its list view as a row, and
wherever else a board appears. Yona: "these cards are the most important
in the whole app." The converged spike is `spikes/board-card-stack/index.html`
(the page is the card as decided, with its rulings at the foot); the
decision is `docs/adr/2026-10-08-the-board-card-and-one-home-page.md`.

From the top:

- **The surface.** The card's top is its main surface, and it is never
  black (Yona, 2026-10-17: "the primary surface should always give the
  most important information"). While the board is well it is the board's
  lights, and nothing covers them. Otherwise it is the notice (round 3's
  concept B: `spikes/board-card-surface/index.html` on the branch
  `claude/spike-card-surface`): the state's icon in a
  30 px circle in the state's tone, then one or two plain sentences,
  centred, and the work's bar and percent under them (or the spinner in the
  icon's place). The tone is the icon's alone: the sentences stay plain. A
  picture under it dims, fully covered, with one dim — the cover's, never a
  stale picture's own dim as well. There are no buttons on the surface: it
  explains, and the primary gives the thing to do. On the narrow card the
  same notice is smaller, so its longest words still fit.
- **The status corner,** cut out of the picture's corner, not laid over it:
  a quiet ⋯ in every state. It opens its own details — every notice, the
  picture's frame rate, age and source as facts, and the board's terminal.
  What needs you is the surface's to say, and so is the editor holding the
  board.
- **The name bar.** The board's name, its group or owner under it, and one
  primary action: the most likely thing the person wants to do, even when
  it repeats a row's own button. The action is a plain, flush section of
  the bar with an icon leading its verb and object: the link's icon before
  Connect, a lock before Unlock, ✓ before Done, + before Add a project, the
  firmware mark before Update firmware and Install firmware. The spectrum
  ring lights it on hover, like any action. **The primary is never a
  greyed verb:** a board with nothing on it offers the next step (Add a
  project); while the board is being reached or identified the primary is
  the conic spinner alone; and when nothing can be pressed there is no
  primary at all — the surface says why. The name keeps about twelve
  characters: on a card too narrow for that beside the whole primary, the
  primary says its verb alone (Update, Install, Reinstall), and the
  surface's sentence names the object.
- **The rows,** always in this order: project · link · access · firmware ·
  hardware. Each is one line in one grammar (`docs/ux/anatomy/board-card.md`):
  **icon · kind · name · state · action**. The icon is the kind's own in
  every state (layers for a project); the kind is a fixed, muted 56 px
  column, so the names line up down the card; the name is the thing itself
  and never moves, and it keeps its width: short of room, the state gives
  way first, cut at its end and then gone, before the name is cut (a
  name still too long beside its button is cut at its end, and the
  details hold it whole; the kind column stays, so every card in a grid
  reads alike); the
  state follows only when it is not normal; at most
  one action fixes it, with a way out beside it where the fix needs one
  (Restore · Reset). The Link row has no kind word: the link's name is its
  kind.

The rules:

- **The card is a fixed height, but for connected.** A board's events
  never change it (see Stable Layout). Offline, locked, blank and updating
  cards all measure the same. A connected card grows once, on connect, to
  hold the board's panel, and shrinks back on Done; the grid keeps every
  other card at its own height.
- **No boxes in boxes.** The bars are full-bleed rows of one card,
  separated by hairlines. Nothing inside a bar draws its own frame.
- **A state tints its row,** in ui.md's status families and no others,
  and its word says what (never colour alone): **orange** (Attention) when
  a device needs you; **red, striped** (Error) when it is broken — a
  shader error, crashing firmware, a connection that failed. Nothing on
  the card is yellow (an unsaved edit's colour), and blue is the lit
  cloud's alone: an update on offer is untinted, and its button carries
  it. In a toned row the kind takes the row's colour, quieter. The rest of
  the card stays neutral.
- **The rows go quiet** while the surface says their problem: the row keeps
  its state word and its own button, and loses its orange or red. The
  surface is the card's one loud place.
- **"Someone has it" is orange,** on the surface's icon. The Link row says
  **Open in another tab**, **Taken by another tab** or **Someone else
  connected**, quietly. A take-over under way is the row's work, and one
  that failed carries Retry.
- **Old looks old.** An offline board's rows keep the last thing known,
  dimmed, their words unchanged; only the Link row says since when.
- **A bar's action sits flush at its end,** as a section of the bar, not a
  chip floating inside it.
- **A mark is an icon alone at a row's end,** after the state, with its
  words as its accessible name and hover: the cloud on a board also
  reachable through lightplayer.app, filled in the live tone while it is,
  struck through while Cloud relay is on and the board isn't there. Its
  shape says the fact, not its colour alone. A fact a mark carries is not
  repeated in words on the row.
- **Every row names its subject,** with its kind word: `Project none`,
  `Project not known`, `Firmware unknown`, never a bare "Not known yet"
  and never a false "none". A stand-in's Link row
  says what it is, and only it: "Simulated · running", "Emulated · off".
- **Details open as Studio's detail card, merged with the bar that opened
  it** (the merged-outline popover). Every fact the bar's summary leaves
  out lives there; nothing on the card is lost, only moved.
- **Work in progress shows in the row doing it,** as its state. The row
  goes neutral, the conic spinner takes its icon's place, its name stays,
  and the work's words are the state ("sending · 40%", one number for an
  update), with the iridescent fill along its foot. The fill sweeps when the percent is unknown, and it lives
  in a clipped track the bar's width: the bar and the card clip nothing, so
  the details that float over a bar and the corner's notch are never cut,
  and no pixel of the fill leaves the card. When it's done, the row is green
  for a few seconds; when it fails, the row is striped, says only that it
  failed ("send failed"), and offers Retry, the reason the first thing in
  its details. The corner doesn't change; the surface says the work over
  the dimmed picture, with its bar and percent.
- **Connected, the bars become the panel.** Connect turns the card's bars
  into the board's whole panel, drawn with Studio's existing panel widgets,
  and the card grows once to hold it. Done turns them back. Top to bottom:
  - **the root's controls, the master first** (the project's brightness, a
    knob on a panel): knobs and toggles in rows, as many to a row as the
    card is wide (three on a phone's narrow card); a palette, a fader or
    the Pattern instrument across the card;
  - **each nested group's controls** under a quiet heading, its name on a
    hairline;
  - **the foot**: a bar with **Edit** (or Edit with a lock, on a board you
    can only play) flush at its end.

  The clock is one **Speed** knob there (1× at the top); its own card keeps
  the tape. The controls keep the panel's own look and gestures: the gold
  held state, the let-go glyph, the detail popover. Nothing links away: no
  All controls, no "· N more". The panel's reset and auto-save switch are in
  the card's ⋯. The name bar's primary is Done (✓) while connected. A card
  that is connecting, reconnecting or failed to connect keeps its bars, and
  the connection bar carries the work.

The card is built in core, as a name bar and bars, and drawn by a few
generic pieces. The web decides how a bar looks, never what it says or
which action it carries (`docs/adr/2026-10-01-agentic-control-offers-in-core.md`).
