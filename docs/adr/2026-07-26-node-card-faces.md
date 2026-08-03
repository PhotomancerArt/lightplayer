# ADR: Node cards — permanent kind-specific face + drawers

- **Status:** Accepted
- **Date:** 2026-07-26
- **Deciders:** Photomancer
- **Supersedes:** The "Agent | Code tabs where the editor lives" UI shape of
  `2026-07-25-studio-shader-agent-architecture.md` as the primary shader
  surface (that ADR's architecture — sessions, providers, write surface — is
  untouched, and the tab strip survives inside the classic-sections fallback
  path; this is the "full node-UX pass toward preview + chat" its Follow-ups
  anticipated). Builds on the pane grammar
  (`2026-07-05-studio-pane-grammar.md`) and the editing overlay model
  (`2026-07-04-studio-editing-model.md`).
- **Superseded by:** Partially — `2026-08-03-panel-visibility-is-derived.md`
  (`docs/design/modules.md` Q13, binding-is-publicity, implemented
  2026-08-03) deletes the `panel` flags this ADR introduced. The face
  grammar itself stands, and the Module face joins the family. See the
  amendment under "Faces are type-aware".

## Context

Node cards presented every kind the same way: generic slot-row sections, a
schema browser rather than an instrument. The shader-agent ADR recorded the
direction — "a shader node's UI trends toward preview + chat, code
secondary" — and a five-round standalone design spike (2026-07-25/26)
settled a concrete grammar, which Yona then asked to be rebuilt
stories-first in the real system behind a visual gate before wiring.

Constraints that shaped the decision:

- **Tabs were already dead UI on node cards**: every node produced exactly
  one `UiNodeTab::main`, so the tab strip never rendered. The spike's early
  tab-based revisions (v1–v4) also failed in use — editing code hid the
  output and knobs, exactly what a shader author needs to watch.
- The generic slot view is the only editor for most node kinds and must
  survive as the universal fallback (and remain reachable on face-bearing
  cards).
- Slot edits flow through one seam — `SlotEditOp::SetValue` on the editing
  overlay, coalesced per-address by the actor — and the shader agent's
  future set-binding/live-sim push enters at the same seam. New control
  widgets must not open a second write path.
- SlotMeta is in the firmware graph: only additive, optional,
  presentation-only fields are acceptable there.
- Faces landed across a phased plan (P1 seams → P2/P2b/P2c stories behind
  two Yona gates → P3 shader/fixture wiring → P4 playlist); the gates
  reworked several mid-flight choices, recorded under Alternatives.

## Decision

### Permanent face + drawers replaces tabs on node cards

A node card is a **permanent kind-specific face** — preview + panel
controls (+ agent chat on shader) that never leaves — with **drawers**
(code, advanced) expanding beneath it. Growth is downward-only from a
stable top. The device card keeps its tab pattern; this grammar is for
node cards. Drawer open-state is view-local signal state for now
(CardUiState re-home is a recorded follow-up). All node cards also show
their kind as quiet right-aligned header text before the ⓘ
(device-card convention).

Faces exist for **shader** (preview → knob row → agent section; drawers:
code = the real inline GLSL asset editor, advanced = today's slot view),
**fixture** (lit control-product preview + dominant horizontal brightness
fader on `FixtureDef.brightness.some`), and **playlist** (ENTRIES strip;
see the invariant below).

### Faces are type-aware and hand-built; data drives only the front panel

Fully data-driven faces are rejected (per the direction note in the
shader-agent ADR and Yona's explicit call): each face is per-kind code.
The **only** data-driven part is front-panel metadata — which slots
appear on the panel and how:

- `SlotMeta.panel: bool` + `SlotMeta.unit: Option<String>` — additive,
  serde-defaulted, presentation-only (never affects validation,
  resolution, or writeback). `StaticSlotMeta` carries the same fields for
  shape-level seeding (fw graph, riscv-checked).
  **Amended 2026-08-03 (modules.md Q13):** the `panel` flag is DELETED from
  both `SlotMeta` and `StaticSlotMeta`. Publicity is the binding: a control
  is on a panel exactly when it carries a `(scope, channel)` panel target.
  `unit` and the editor hints below are unaffected, and the fixture face's
  brightness fader is now named outright by the face rather than flagged.
- `ValueEditorHint::Knob { min, max, step }` beside `Slider`.
- Shader uniforms are per-instance data, not shape meta (all uniforms
  share the `ShaderSlotDef` record shape), so panel-ness is authored on
  the def: `ShaderSlotDef.panel` / `.unit`, widget derived from authored
  `min`/`max`. **Amended 2026-08-03 (Q13):** `ShaderSlotDef.panel` is
  deleted; a uniform is on the panel when it is bound to a bus channel.
  `.unit` and the min/max-derived widget stay as recorded.
- Fixture brightness is seeded by the `lpc_model::Brightness(u32)`
  newtype (serde-transparent; def JSON/wire unchanged) carrying
  `panel: true` + a 0–255 slider hint.

### Faces derive from the finished section DTOs

`node_face_builder::kind_face()` (studio-core project layer) builds the
face **from the already-projected section DTOs** — the same rows,
previews, edit-joined states, and embedded editors the generic sections
view renders — after `embed_asset_editors`, in both the node and child
walks. One derivation, two presentations: a panel control and its backing
slot row **cannot disagree** on value, bound-ness, or dirty state. The
builder never re-reads slot controllers or project state. The agent
handle on the shader face is decorated by the same
`AgentController::decorate_editor_view` pass as the editor view was.

### Panel controls ride the existing slot seam

`KnobField`/`HFaderField`/`ToggleField` are stateless field components
(`value, state, address, on_action`) dispatching
`slot_set_value_action` → `SlotEditOp::SetValue`; the actor's per-address
coalescing (built for oninput floods) absorbs drags. Transient-vs-
persisted stays a slot-policy property, not an op property. Bound slots
render the violet treatment (violet = status-bound family, never green).
This is deliberately the same seam a future agent set-binding /
live-sim push enters — controls added no new ops and no second door.

### Section language: left rail, full-bleed, flat

All face sections (output, controls, agent, entries, code, advanced) use
one `NodeCardSection` grammar: full-bleed content separated by 1px
hairline dividers — no box-in-box — with a slim vertical label rail on
the **left** edge (bottom-to-top label, reads like a book spine) when
expanded and a slim horizontal row (chevron + label + summary) when
collapsed. The pre-merge live review hardened the grammar's legibility:
the divider is `border-strong` (the original `border-muted` hairline
rendered but sat below the perceptual threshold against the card surface
on real displays), and the rail and the collapsed row are styled as **two
states of one control** — identical label typography (uppercase
small-caps in both writing modes) and one shared chevron glyph, pointing
right on the collapsed row and down at the top of a toggleable rail
(dimmed at rest), each rotating toward the other state on hover
(`prefers-reduced-motion` disables the transition). Non-toggleable
sections (the permanent face) keep the rail as a pure label: no chevron,
no hover tint, dimmer text. The agent section labels its role explicitly
("edits this shader with you").

### Control popover: the label is the trigger, the control is the visual

A panel control's **label is the detail trigger**: a real button carrying
the control name plus a small info glyph, so keyboard focus and
Enter/Space work. (The first landing used a hover-revealed corner ⓘ; the
live review killed it — it appeared on top of the knob it described.)
Opening still merges the **whole live control** with its aspects panel
via the contiguous-outline popover — "diving into the control", not a
duplicate rendering. `base/popover.rs`'s anchored mode
(`anchor_id`/`anchor_visual`) is unchanged: the merged outline measures
the control element instead of the trigger button's rect, the top-layer
visual paints the control's live copy, and clicks inside it do not
dismiss (it hosts working controls). `outline.rs` untouched. The popover
content is the SAME aspect list as the backing slot row
(`SlotDetailButton`/`DetailPopover` reused, not forked — `DetailPopover`
gained a custom-trigger mode beside its icon trigger), so binding info,
unbind, and edit state appear identically in both places.

The label also carries the slot's **state color**, reusing the slot-row
affordance ladder (`primary_affordance`): red failed/invalid, warning
unsaved (live-blue when the edit is transient), working while saving,
violet bound, quiet otherwise — green stays valid-only. This retired the
separate edit-state dot: the label now says what the dot said, and the
overlap case (edited while bound) loses nothing because the widget
itself keeps the violet bound family while the label wears the edit
color.

### Playlist: one live surface

> **Revised 2026-07-28.** The invariant below made every non-playing entry
> uneditable: a strip click set focus correctly, and this same derivation
> then filtered the focused child away
> (`docs/defects/2026-07-28-playlist-entry-selection.md`). The fix landed
> as the **activate-by-click** op this ADR's Consequences list as missing
> (`PlaylistActivateOp`): a non-active chip now activates its entry
> through the runtime command channel, which makes it active, which
> renders its card. The active chip keeps the child's select action, since
> re-activating it would be a no-op.
>
> The single-rendered-child rule below is therefore **unchanged** — it
> still follows `active_entry`. What changed is that the user can now move
> `active_entry` from the strip.
>
> Residual gap: selecting a non-active playlist child from the **project
> tree** still shows the active entry's card, because the rendered child
> follows playback rather than selection. A selection-follows-focus
> variant was built and dropped in favour of this one; see the defect
> entry for why the two do not compose.

The playlist face is the ENTRIES strip only — per-entry name, duration
chip, cue tag (`trigger_ids` non-empty), and an **ACTIVE** placard
(matching the engine's `active_entry` vocabulary) replacing the active
entry's thumbnail. The invariant: **the active child's output renders
exactly once, and no other child renders at all.** The playlist card's
own produced-visual hero is suppressed (the strip replaces it), and the
children list is filtered in place — in the same `kind_face` derivation
that reads `active_entry`, so strip and card can never disagree — to
exactly the active entry's child, which renders **below the playlist
card as a full sibling card** (its own face, drawers, editing). Dirty
summaries aggregate over the full child list before filtering, so hidden
children's edits still count. Strip clicks dispatch the child's existing
node-select action (`ProjectEditorOp::Focus`). If the face cannot be
derived (no entries row, unresolvable ACTIVE, unmounted child), the card
falls back to today's full rendering — never a blank card.

### Fallback guarantee

Kinds without a face render exactly as before (`kind_face` → `None`),
and the classic slot view remains permanently reachable on face-bearing
cards inside the advanced drawer. Fallback is proven by the pre-existing
suites plus face e2e tests against a real server.

## Consequences

- Shader authoring happens on the card: preview, knobs (violet when
  bound), and agent chat are simultaneously visible while the code drawer
  is open — the failure mode that killed tabs is structurally gone.
- **No "activate entry by click" wire op exists** — the engine switches
  playlist entries only via trigger ControlMessages or timed advance, so
  strip chips can only select/focus the child. Activation-on-click is a
  reported missing op for the roadmap (Follow-ups).
- `UiProducedValue` gained a stable `key` (slot path) so the playlist
  builder can read `active_entry` from produced rows without parsing
  presentation labels — a small additive DTO change the
  derive-from-sections doctrine required.
- Knob/fader emit is typed (`PanelEmit`: F32/U32/I32 with rounding on
  integer gestures) because brightness is u32; extraction stays in the
  panel layer, the wire op unchanged.
- The kind label in every node header caused broad but benign story
  baseline churn.
- Firmware graph: SlotMeta/StaticSlotMeta grew additive optional fields
  only; riscv release-esp32 check stays mandatory when lpc-model moves.
- Harvested refinement list (post-landing round, not this slice; the
  live-review round already landed section-divider legibility, the
  rail/row collapse affordance, and the label trigger replacing the
  corner ⓘ and the edit-state dot): bound knob shows the inert authored
  default rather than the live bus value; panel detail popover titles
  use raw address paths (want friendly labels); playlist entry thumbs
  are name-only until a preview snapshot lands; story fixtures fake the
  selection look on the mock active child; the knob advertises
  `role="slider"` but is pointer-only (no keyboard handler/tabindex yet
  — the label trigger and the fader's native range input are keyboard
  targets); ACTIVE-placard-follow under a live trigger still needs a
  hardware walk (browser sim has no virtual button).

## Alternatives Considered

- **Tabs on node cards**: built in the spike (v1–v4) and carried into
  early stories; rejected at the gate — switching to code/settings hid
  the output and controls, and the in-tree tab machinery was already
  dead (one tab everywhere). Device cards keep tabs.
- **Eyebrow section labels** (variant B, uppercase label over content):
  built for the re-gate beside the rail; rejected — the rail won, then
  moved from the right edge to the left at the re-gate.
- **Embedded active child inside the playlist face** (P2b): rejected —
  the active child renders as a sibling card below the playlist card;
  the existing child-card pattern is cleaner and the strip cannot be
  moved by child height changes.
- **Popover duplicating the control** (P2b `embed`: a second rendering
  of the control inside the popup): rejected for the anchored mode —
  "the button IS the whole actual control"; duplication risked drift
  between the two renderings.
- **Hover-revealed corner ⓘ as the popover trigger**: the P2c landing
  shape; rejected at the live review — the ⓘ materialized on top of the
  knob it described, and hover-only affordances hid the door. The label
  button (name + info glyph, state-colored) replaced it; the anchored
  "dive into the control" visual survived unchanged.
- **Fully data-driven faces** (face layout from metadata): rejected —
  faces are product surfaces, hand-built per kind; metadata only says
  which slots sit on the panel and how a value edits.
- **A new knob/fader write op**: rejected — `SlotEditOp::SetValue`
  through the overlay already coalesces floods and carries the
  permission/dirty/Save semantics; a second path would fork the seam the
  agent also relies on.

## Follow-ups

- **Shader perf line** — its own run of work (cycle model exists
  engine-side); the face reserves no space for it yet.
- **Playlist strip evolutions** — timeline view, cue-trigger UI,
  autoplay-to-cue controls; the strip was shaped for them.
- **"Activate entry by click" wire op** — engine-side activation op so
  strip chips can switch entries, not just select the child node.
- **Fixture mapping editor drawer** — the face's planned custom drawer;
  today fixture has only advanced.
- **Drawer open-state re-home into CardUiState** — view-local signals
  today (Yona Q4); re-home when the ui-state-audit plan lands.
- **Refinement round** — the harvested list under Consequences (bound
  knob live value, friendly ⓘ titles, thumb warming, story-fixture
  selection look, hardware placard walk).
