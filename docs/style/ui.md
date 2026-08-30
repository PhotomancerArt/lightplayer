# Studio UI Style

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

Every selection mark is static because motion stays the pointer's: the
iridescent ring (`ux-ir-ring`) on hover, its whisper variant on dense rows
(`ux-row-edge` — a lighter wash, the bloom, and a faint moving iridescent
hairline; hover never darkens a row), the press flare (`ux-press-flare`)
on `:active`, and the pinned ring plus a lifted shadow (`ux-drag-chip`)
mid-drag. A selected row and a row you're merely passing over stay
different KINDS of light — one holds still, one moves.

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
