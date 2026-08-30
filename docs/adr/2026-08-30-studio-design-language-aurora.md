# ADR: Studio's design language is Aurora — spectrum interaction light, liquid glass, violet-graphite surfaces

- **Status:** Accepted
- **Date:** 2026-08-30
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Studio's original palette read pale and low-contrast: a mint/sage accent on
graphite surfaces, ~1.3:1 borders, an 8%-alpha hover wash, and a popover
background identical to the card background beneath it. Yona's framing from
the originating vision session: "LightPlayer should feel fancy — it's an LED
art engine, not a TodoMVC demo," plus a concrete ask — detail popups should
go darker with a blurred-background (liquid glass) effect, and
highlights/hovers should read bright, possibly multi-color.

A grounding survey found the palette concentrated in exactly two files
(`style.css`'s `:root` and `tailwind.css`'s `@theme` re-exports), token
discipline already near-total (~2105 semantic-token usages against 7 raw
Tailwind color classes), and the status-tone families
(violet=binding, amber=unsaved, orange=attention, gold=engaged, blue=live,
sage=export, green=good, stripes=error/debug) both load-bearing and already
test-enforced. The refresh's job was the *look*, not a rebuild of the token
architecture `docs/adr/2026-06-25-studio-tailwind-styling.md` already
governs.

Direction was explored in a spike (`spikes/design-language/index.html`,
draft PR #463) rather than argued in the abstract — four candidate
directions side by side in the app's own visual language, gated twice:
round 1 picked direction C ("Aurora": the spectrum as the interaction
light, violet-tinted graphite surfaces) over Phosphor, Ice, and Limelight;
round 2 converged the glass recipe and ruled the iridescent ring as the
hover language. The spike is the design record — production code never
imports from it, and `spikes/design-language/` does not exist on the
production branch; it lives only on the spike branch behind PR #463.

Planning proceeded through phases P1–P4 (token foundation, glass popovers,
interaction light, button consolidation) with several decisions explicitly
deferred to a post-implementation gate (G1) once the direction could be
judged in the running app rather than the spike page. G1's rulings (recorded
in the plan's `notes.md`, 2026-08-30) are folded into "Decision" below.

## Decision

Studio ships the Aurora design language: violet-tinted graphite surfaces,
the spectrum reserved for interaction feedback rather than resting chrome,
and liquid glass on overlay surfaces.

**The rules** (also codified in `docs/style/ui.md`'s "Color & Light"
section):

- Status hues never move and chrome may not borrow them — the eight
  families above stay exactly what they were.
- Selection stays the neutral white-ish outline
  (`--studio-color-selection-*`); hover, press, and drag-in-flight wear the
  spectrum instead (`ux-ir-ring`, `ux-press-flare`, `ux-drag-chip`).
- Glass (`--studio-glass-*`, `ux-glass-panel`) is for overlays only —
  popovers, sheets, card bars — never resting surfaces.
- Multi-color is a moment: the spectrum lights up hover/press/drag, the
  brand mark, and the hero. It never sits on resting chrome, never on
  selection, and never on a status-toned control.
- Structural contrast is a floor: borders ≥ ~1.8:1 on their surface, raised
  surfaces ≥ 1 visible lightness step.
- Every interactive control carries `--studio-color-focus-ring`
  (`ux-focus-ring`, or the bare-element `:focus-visible` rule that now
  covers text inputs for the first time — no Tailwind preflight is loaded,
  so this rule was missing entirely before this refresh).
- Buttons are `ActionButton`/`InlineButton`; a new one-off button class
  string is a review flag (P4 folded ~27 ad-hoc call sites into the shared
  helpers).

**G1-2 — gradient primary: KEEP.** "Gradient is good." The Primary action
tier's fill (`.ux-primary-gradient`, a 105° spectrum sweep) is simply the
Primary fill now, not a gated alternative to a flat accent fill; the P3
build included a boolean knob (`GRADIENT_PRIMARY` in
`action_button.rs`) to make this an easy either/or at the gate, and this
ADR's implementation removes that knob — the gradient is the only path
through `solid_class`'s `Primary` arm.

**G1-3 — all four "extras" KEEP.** The conic working spinner
(`ux-conic-spinner`), the iridescent progress fill (`ux-iri-fill`), the
drag-in-flight ring (pinned via `ux-ir-ring-on` plus `ux-drag-chip`'s
lifted shadow), and the press flare (`ux-press-flare` on `:active`) are all
standard vocabulary for working/progress/drag/press moments, not optional
flourish subject to later culling.

**G1-4 — spectrum edge on the merged-outline popover: YES, quiet/neutral
only.** "Looks OK as is, but a little bland" plus cheap to add. A second,
low-alpha SVG stroke (`.ux-popover-outline-spectrum`, opacity driven by
`--ux-popover-spectrum-edge`) rides on top of the same merged-outline path
the tone stroke already draws, but only where `.ux-popover-chrome-quiet` or
`.ux-popover-chrome-neutral` opts in. A status-toned popover (working,
live, bound, warning, attention, error, debug) keeps its pure semantic
stroke — the semantics-beat-decoration rule applies to popover chrome the
same way it applies to buttons: a status border already means something,
and the spectrum edge is only ever decoration layered on a border that has
nothing to give up.

### Liquid glass material and the popover mechanism

The glass recipe (bg `rgba(12,12,20,.55)` +
`backdrop-filter: blur(22px) saturate(1.5)` + an inset top edge light +
a deep drop shadow + a masked-overlay spectrum ring) lives centrally as
`.ux-glass-panel` and its `--studio-glass-*` tokens, so a single edit tunes
every glass surface in the app. `saturate(1.5)` is what makes the blur read
as liquid glass instead of gray mud.

The merged-outline popover (ADR 2026-07-15) needed its own version of this,
because that surface is not a normal DOM box with a background — it's one
SVG path drawing the fill/border/shadow for the union of a trigger and a
panel rect. `.ux-popover-glass` is a fixed, full-viewport div positioned
UNDER the SVG in DOM order, clipped with
`clip-path: path(evenodd, '<the same path string the outline SVG draws>')`
so its blurred silhouette exactly coincides with the outline. The path's
own fill goes translucent (a `color-mix`'d tone at 26–58% depending on
chrome variant) and paints above the glass layer, so the result reads as
one blurred, tinted surface even though it's two elements.

**The panel stand-down.** `DetailPopover` can render AS the merged-outline
popover's panel — the same DOM element carries both
`.ux-svg-popover-panel` and `.ux-glass-panel`. Left alone, that would
nest two `backdrop-filter`s (double the compositing cost for no visible
difference — the panel sits directly on top of the popover glass already)
and put the masked spectrum ring in a fight with the tone stroke the
outline SVG draws. `.ux-popover-panel.ux-svg-popover-panel.ux-glass-panel`
turns the panel's own `backdrop-filter` and ring pseudo-element off; the
merged-outline system supplies the material for that surface, full stop.
Standalone `ux-glass-panel` uses (anchored detail panels not hosted inside
the merged outline, static story sheets) keep the full material.

## Techniques And Their Traps

- ⚠️ **`border-image` ignores `border-radius`** and paints a square frame
  regardless of the element's rounding — discovered in the spike's round-1
  build. The spectrum ring is instead a masked overlay: an
  absolutely-positioned pseudo-element at `inset: -1px` (`inset: 0` for
  glass panels, which are already `overflow: hidden`), `padding: 1px`, a
  gradient background, and two solid mask layers XOR'd together
  (`mask-composite: exclude` / `-webkit-mask-composite: xor`) so only the
  1px padding band shows, clipped to the host's own `border-radius`. The
  two-background padding-box/border-box trick some CSS references suggest
  instead does **not** work over a translucent glass fill — the ring's own
  background paints through the gap.
- **`@property --ir-a { syntax: "<angle>" }`** registers the ring's
  rotation as an animatable custom property. Without the registration, a
  `conic-gradient(from var(--ir-a), …)` cannot be interpolated by a CSS
  transition/animation — the browser treats it as a plain (non-animatable)
  string swap. Registering the syntax turns the same `conic-gradient` into
  a smoothly rotating one for free.
- ⚠️ **`backdrop-filter` creates its own stacking context and containing
  block.** A `position: fixed` descendant of a `backdrop-filter` ancestor
  no longer positions against the viewport — it positions against that
  ancestor, which silently broke `z-index`/`position: fixed` assumptions
  elsewhere in the tree (hit previously in the gallery card footer glass
  bar, #454: "raise container + pointer-events"). Any new glass surface
  needs its stacking/positioning context checked, not assumed.
- ⚠️ **`Page.captureScreenshot`'s `captureBeyondViewport: true` silently
  drops `backdrop-filter`** in Chrome's headless screenshot path — the
  compositor never runs the backdrop pass for content it treats as
  "beyond" the viewport, even when the actual clip region is on-screen.
  Verified glass rendering only in viewport-sized captures during this
  effort. Studio's own story-baseline capture
  (`scripts/studio-story-pngs.mjs`) passes `captureBeyondViewport: true`
  on every per-story screenshot — see this phase's baseline-flip review in
  the plan's ship report for what that means for glass surfaces in the
  baseline set.

## Alternatives Considered

- **Phosphor** (spike direction A): a single cool cyan/green glow language,
  closer to a CRT/terminal aesthetic. Rejected — read as one-note next to
  Aurora's fuller spectrum, and didn't answer the "multi-color" half of the
  ask.
- **Ice** (spike direction B): a cold blue-white glass language with no
  spectrum at all. Rejected — solved the glass/contrast half of the brief
  but not the "fancy… maybe multi-color" half; felt closer to the
  low-contrast starting point than a genuine refresh.
- **Limelight** (spike direction D): content-lit accents — a per-card glow
  sampled from that card's own art. Rejected as the *direction* (too much
  motion/variance for chrome that has to stay legible at a glance across
  hundreds of cards) but explicitly parked for harvest: the hero treatment
  already does a version of this, and a future session may pull the idea
  into a narrower surface.

## Consequences

- The retheme flips essentially the full story-baseline set (~1938
  PNGs across 646 stories × 3 viewports) — a `:root` token change
  cascades everywhere by design. Baselines live in the companion repo
  `PhotomancerArt/lightplayer-stories`; per
  `docs/adr/2026-08-17-story-baselines-companion-repo.md`, merging this PR
  *is* the acceptance of the new baseline set — no baseline files are
  hand-edited or committed here.
- `backdrop-filter` promotes a compositing layer, so glass is deliberately
  bounded to overlays (popovers, sheets, card bars) and never resting
  chrome — an unbounded rollout would have made every scroll/repaint pay
  for a blur pass. Iridescent ring/progress animation runs only while the
  control is actively hovered/mounted/working, and
  `prefers-reduced-motion: reduce` turns every spin/sweep/lift off,
  leaving only the static paint.
- The spike (`spikes/design-language/index.html`, PR #463) stays as the
  design record of the explored directions and the glass/ring lab that
  converged the recipe. It is not imported by production code and does not
  exist on the production branch.
- This ADR amends the MECHANISM ADR 2026-07-15 defines for popover chrome,
  not its architecture: the merged-outline path still owns
  fill/border/shadow for the union shape, computed the same way. What
  changes is that the fill can now be translucent with a clipped glass
  layer sitting beneath it, and the path can optionally carry a second,
  purely decorative stroke. Geometry, animation, and the top-layer
  re-parenting story are untouched.

## Follow-ups

- Limelight harvest (content-lit, per-card accents) — parked, not
  scheduled.
- Light mode remains fully out of scope; Aurora is dark-only, matching the
  rest of Studio.

## Amendment — the accent reckoning (2026-08-30)

Aurora as originally shipped removed mint-as-chrome-state but kept
`--studio-color-accent` (mint) as the action/link/identity color. That
stance is superseded: the accent reckoning (planning dir
`2026-08-29-2140-accent-reckoning`, PR #478) ruled **full no-hue** (D1,
Yona: "no hue if we can make it work. the aurora gradient really is the
look I'm going for") and retired the accent token set outright.

North star, ratified: **at rest, chrome is neutral; saturated color
belongs to artwork, status, and interaction light.** "Accent" dissolved
into roles — actions are neutral chips answered by the iridescent ring
(the gradient Primary stays the one loud fill), links are neutral with a
hover brighten, selected/current is the neutral selection family,
authored values (knob arcs, fader fills, the tape playhead) are the
bright neutral, progress is the iridescent fill. One new frozen family
was added beside EXPORT: **EXAMPLE** (`--studio-status-example-*`,
lavender-grey) for example provenance. All five recolored surface groups
passed the in-app feel gate with no per-surface exceptions.

The escape hatch is per-surface exceptions argued at a review gate —
never a blanket accent revival. Current guidance lives in
`docs/style/ui.md` Color & Light.

## Amendment — the selection grammar (2026-08-30)

The reckoning's "selected/current is the neutral selection family" stance
is refined, not reversed. After the full no-hue landed, the ruled feedback
was "the ux feels a bit dry — I like the rainbow," and the
spectrum-in-selection spike (`spikes/spectrum-selection`, PR #481, three
gate rounds) converged on a grammar rather than a single treatment:

- **Selection and navigation are separate concepts and never share a
  mark** ("a surface wears the language of the concept it IS").
- **Nav you-are-here = a STATIC spectrum line** on the nav axis's edge:
  full-rainbow underline on the view tabs and the site chrome nav
  (`ux-here-line-x` grammar); cool-sweep side line (`ux-here-line-y`,
  `--studio-spectrum-cool`) on vertical navs — the story-book nav.
- **Object selection = a STATIC spectrum ring** (`ux-sel-ring`): option
  cards, the workbench tree's focused row (G1 ruled it selection, not
  nav), and the Map/Patch selection surfaces (fixtures tree, output
  headers, port cells). Small radii take the cool variant
  (`ux-sel-ring-cool`) — the full sweep compresses to its warm stops
  there and reads as attention-orange.
- **Intensity scales with size**: full spectrum on large marks, cool
  sweep on small ones — the cool tri exists because the full run's
  amber/red/orange stops are the unsaved/error/attention hues and would
  collide beside a dirty row.
- On rows and cells the ring is the ONLY selection paint (no grey wash;
  a selected row keeps its natural ground, dirty tint included). Option
  cards keep the selection wash + neutral check under the ring. Every
  mark is static — motion remains exclusively hover/press/drag light.
  The dense-row hover is the hover ring's whisper variant: a lighter
  wash (never darker — dimming reads wrong), the bloom, and a faint
  MOVING iridescent hairline. Its old blue inset edge read as a
  competing mark beside the ring; moving-vs-still is what keeps hovered
  and selected different kinds of light.

Status hues remain frozen; nothing here touches them. The design
library's main page (the design-language story) documents the grammar
with live examples.
