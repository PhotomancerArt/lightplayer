# ADR: The Clock Transport Is a Panel Instrument — Three Wires, One Control

- **Status:** Accepted
- **Date:** 2026-08-07
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-07-08-binding-ref-syntax-and-channel-naming.md`
  (amended below — `clock.*` channel-naming norm); `2026-08-01-debug-slots
  -taxonomy.md` (amended below — answers follow-up (a)); `2026-08-03
  -panel-visibility-is-derived.md` (the `panel = "show"` mechanism this
  ADR extends from scalar slots to a record field);
  `2026-08-06-panel-controls-carry-structured-live-values.md` (the
  narrow-field precedent behind the razor); `docs/debt/clock-transport
  -has-no-transport-ui.md` (this closes it); plan
  `2026-08-04-2355-clock-tape-hero` (P6/P7/P8); spike
  `spikes/clock-transport-hero/index.html` (PR #345, gate rounds 1–2)

## Context

The clock's transport — run/pause, speed, scrub offset — shipped 2026-05-12
as three independent `SlotRole::Debug` rows (`controls.running`,
`controls.rate`, `controls.scrub_offset_seconds`): writable, session-only,
rendered by the generic slot-row machinery in a hazard-striped Debug
section. `docs/debt/clock-transport-has-no-transport-ui.md` named this
structural rather than cosmetic: transport is a first-class performance
concept, and building a scrub UI inside a Debug drawer would cement the
wrong home. `2026-08-01-debug-slots-taxonomy.md` recorded the same tension
as its follow-up (a) — Debug's corpus is otherwise all diagnostics, and the
clock's rows were the one example that read as transport.

Plan `2026-08-04-2355-clock-tape-hero` closes both. The spike
(`spikes/clock-transport-hero/index.html`, PR #345 rounds 1–2) converged on
a **tape** surface — a scrolling timeline under a fixed playhead — over a
sweepwatch dial, a record platter, and a bare hairline scrub strip, all
three rejected round 1 for reading as decoration rather than an instrument
you drag. Round 2 chose a skeuomorphic log ¼×–8× fader with magnetic octave
detents (tenths tried and rejected — they didn't feel like gears) over
speed-linked zoom, which round 2 initially liked (constant pixel velocity,
adaptive tick ladder) but Yona reversed mid-plan (notes.md, "Second
feedback round," item 6): "I think I was wrong about zooming the tape …
in this UI, I really expect the speed slider to make that tape move
faster / slower." Fixed zoom shipped and passed G1 ("speedup looks good,
its better than zooming"); the zoom idea is banked for the future
piano-reel input recorder, where it belongs. An **actual-velocity**
variant (pixel speed also scaling with rate, e.g. √rate) stayed an open
note through the reversal and dies with it — nothing in this plan
proposes it.

P1–P5 (PR #345) landed the record rename, the tape hero on the clock card,
gesture handling, and Debug-row retirement, and passed G1. P6 then hit a
real blocker taking the transport onto the module panel: the original
design (Q3) bound the whole `ClockTransport` record to ONE bus channel,
but the engine's binding index is exact-`(node, path)` — a record-level
`default_bind` cannot feed the clock's own per-field accessor reads, and
the loader's default-bind walk was top-level-only. P6 reported three
options (A: rework the engine to read whole records off one channel: a
hot-path change; B: three leaf channels; C: no channels, panel dispatches
Debug `SetValue`s the way the pre-panel transport already did) and
stopped for a decision. This ADR is that decision plus the two amendments
it required, plus the taxonomy follow-up it was always going to answer.

## Decision

### 1. Three leaf channels (option B), not a whole-record channel (A) or no channels (C)

Yona: "I'm more worried about correctness than ease — we are in alpha,
now's the time to do it right." The clock transport rides the bus as
**three independent leaf channels** — `clock.rate`, `clock.play_state`,
`clock.scrub` — each declared by its own leaf's `default_bind`, not as one
channel carrying the whole `ClockTransport` record.

The argument, in order of weight:

- **The bus has no read-modify-write primitive.** A record channel forces
  RMW on every multi-producer patch — a panel writing `rate` while a
  remote toggles `play_state` is a structural lost-update race, because
  whichever write lands second silently reverts whatever the other write
  touched. Three independent scalars compose with no coordination: each
  producer owns exactly the field it writes.
- **`2026-08-06-panel-controls-carry-structured-live-values.md`'s lesson
  transfers directly.** That ADR fixed a panel control that could only
  ever show its own writes as opaque display text because the value
  behind it (a `GradientConfig`) could not round-trip through a string —
  "a whole-record panel write is exactly the shape that broke palettes."
  A record channel is the same shape, one layer up: a producer that only
  ever means to touch one dimension would still have to fetch, patch, and
  rewrite the whole struct.
- **Kinds are honest.** `rate`, `play_state`, and `scrub` each get a real
  semantic kind (`Ratio`, `Choice`, `Duration`) in the well-known channel
  registry — real picker hints, real mismatch warnings. A record channel
  has no honest kind of its own and would defeat every one of them.
- **Per-leaf channels let anything modulate one dimension in isolation** —
  a phasor driving `clock.rate` for a speed ramp, an ESP-NOW remote
  toggling `clock.play_state` — with no pack/unpack adapter. A struct
  channel needs a bespoke adapter for every such patch.
- **The engine evidence pointed at B all along.** Authored bindings at
  `transport.rate` already worked
  (`authored_clock_rate_binding_registers`,
  `project_loader.rs:5163` — exact-path resolution was never the
  blocker for a leaf). The only real engine gap was that
  `declared_default_binds` did not recurse into a record-typed field's
  own fields; that recursion is the entire engine cost of B. A, by
  contrast, needed the hot-path rework the exact-`(node, path)` index was
  built to avoid, for a case (whole-record dataflow) the clock itself
  does not even want — it reads the transport per-field.
- **C (no channels) was rejected as a regression, not a wrong answer.** It
  is exactly how the transport worked before this decision — the panel
  would dispatch plain `SlotEditOp::SetValue`s at each control's address,
  with no live echo, no bus presence, and nothing else able to modulate a
  dimension. Legal, but it throws away everything a channel buys, for no
  correctness gain over B.

The transport's `SlotRole::Debug` marking is unaffected by any of this —
role and channel identity are orthogonal (see §6).

### 2. The wiring razor

The general rule this decision falls out of, stated for reuse the next
time a record-shaped slot needs bus reach:

> **A channel value is something a consumer consumes WHOLE.**
> `GradientConfig` and `PhasorConfig`/`TimeProduct` both ride one channel
> because their consumers read them as one unit — a shader samples a
> gradient's stops together, a phase reader wants period/waveform/offset
> together. **If any field of a record would ever be produced or consumed
> in isolation, wire it per-leaf instead.** The clock transport fails the
> whole-consumption test on its face: even the clock node itself, the
> transport's own owner, reads `rate`, `play_state`, and `scrub` as three
> independent per-field accesses, not as one struct read.

Layering falls out of the razor directly: **one record in the model, one
widget on the panel, three wires on the bus.** The record groups the
concept for authors and for the widget that presents it; the wires are
what dataflow actually touches.

### 3. Model-declared grouping: `panel = "show"` promotes a RECORD to one control

`2026-08-03-panel-visibility-is-derived.md` established `panel = "show"`
as an additive hint that promotes a scalar slot's `default_bind`-derived
wiring to panel publicity (`public = authored ∨ (default ∧ hinted)`).
This ADR extends that mechanism from a scalar field to a **record**
field: `ClockDef::transport` itself carries `#[slot(panel = "show")]`,
never any individual leaf.

**The rule:** a promoted record whose named shape (`lp::clock::Transport`)
maps to a widget yields exactly ONE panel control; its leaf channels are
that control's wires; leaves covered by a group are suppressed from the
generic per-channel derivation that would otherwise turn three
`default_bind`s into three separate knobs. The widget is selected by a
match arm on the shape id today — "first implementation is a match arm,
not a registry," deliberately: the clock's Transport is the only grouped
control that exists, and a second one is a better trigger for a registry
than a speculative one built for a population of one.

**The partial-wiring contract** answers "what if only some of a group's
leaves are bound?" — probed directly (notes.md, "what if only 2 of 3 are
bound?"), with each fact given its own layer so a partial group degrades
predictably rather than through emergent behavior:

- **Rendering is a shape fact.** The faceplate always renders WHOLE —
  fader, run/pause, and scrub strip all draw, regardless of which
  dimensions are wired. A widget that hid a dimension because its wire
  happened to be absent would make the instrument's shape a function of
  binding state, which is exactly the kind of coupling the razor's
  layering rule exists to prevent.
- **Membership is a wiring fact.** The GROUP appears on the panel at all
  if and only if at least one leaf's wiring is panel-public. Zero wired
  leaves means no panel presence — the card face is unaffected either
  way, since the card always shows the instrument regardless of panel
  membership.
- **Dispatch is a per-leaf fact.** A gesture on a wired dimension is a
  scalar `PanelWriteOp` on that dimension's own channel, with its own
  echo; a gesture on an unwired dimension falls back to a plain slot edit
  at that dimension's address — the same `panel_write_or_slot_action`
  rule every ordinary control already uses, and how the whole transport
  dispatched before this plan's panel work existed.
- **Anchor = the rate leaf's effective channel; if unwired, the next
  wired leaf in declaration order.** The generic panel machinery
  (per-channel dedup, the reset gesture, `Read`/`Following`/`Engaged`
  state) reads one `panel_target`/`address`/`live_value` triple per
  control, so the anchor's facts are mirrored onto the control's own
  fields — that single pair is the seam the rest of the system already
  understands, and the group needs no changes to it.

**The vacuous-promotion guard.** A `Show` on a record with no leaf
`default_bind` anywhere beneath it would silently never appear on any
panel — a declaration bug that would otherwise surface only as "why isn't
this control there," at runtime, possibly much later. Declarations are
compile-time facts, so this fails CI instead:
`panel_show_must_promote_a_default_bind`
(`lp-core/lpc-model/tests/shape_guardrails.rs`) walks the whole static
shape catalog, following `Ref`s cycle-safely, and asserts every
`panel = "show"` field has a `default_bind` on itself or a descendant.
It also asserts non-vacuity directly (`promoted >= 2`) so a walk that
silently stopped finding panel hints at all would not pass by finding
nothing to check. This guard is **strictly stronger** than the derive
macro's old lexical check (which could only see the annotated field's own
`default_bind`, never through a record-typed field down to a leaf) — the
macro's version was removed as part of this change rather than kept
alongside a check that supersedes it.

### 4. State-on-a-bus norm (D20): a self-describing enum, never a bare bool

`running: bool` became `play_state: PlayState { Playing, Paused }`,
`Kind::Choice`, wire tags `"playing"`/`"paused"`. Yona: bare booleans are
"notoriously hard and confusing" once a channel is the thing carrying
them — a reader arriving mid-stream, or a picker listing candidate
channels, has to already know the polarity convention to make sense of
`true`. A self-describing enum needs no side channel to interpret.

This is now the **norm**, not a one-off fix: **bus state is a
self-describing enum; verbs and commands are `trigger`-channel business.**
`clock.play_state` carries the *desired* transport state, a noun — never
a verb like "toggle" or "tap tempo." A command (something that means "do
this now," with no value to hold between deliveries) belongs on the
`trigger` channel, where a missed message means a missed event; state
(something with a value that must be readable by a late-arriving reader)
belongs on a state channel, where the value itself is the whole story.
Conflating the two — modeling a command as a state flip, or state as an
event stream — has been the seed of confusion each time it has happened
in this codebase, and this ADR names the boundary explicitly so the next
control does not have to rediscover it.

**Requested vs. effective is already the def/state slot split — not a new
concept.** A follow-up probe asked whether `clock.play_state` (the
setpoint) needs to be disambiguated from the clock's actual, moment-to-
moment playing/paused behavior. It does not need new machinery: the
*consumed* transport (what a producer requests) and the *produced* state
(`ClockState`/the `TimeProduct` behind `bus:time`, which the tape's own
motion already renders from) are already the two sides of the existing
def/state slot split — requested state lives on the def side, effective
state on the state side. Today the two never disagree, so nothing
currently reads them differently; the split exists so a future quantized
pause or an external sync source has somewhere honest to land without a
model change. The setpoint keeps state vocabulary (`playing`/`paused`),
never a verb, precisely so it stays a legible request even before it is
honored.

### 5. Amendment (D21): `owner.purpose` channel namespacing

**This amends `2026-07-08-binding-ref-syntax-and-channel-naming.md`.**
That ADR's naming norms covered unitless canonical names
(`time`, `time.delta`), `.in`/`.out` at project boundaries, `/instance`
for parallel channels, and a `transport.*` family reserved for UI
transport **events** (`transport.next`, `transport.prev`,
`transport.pause` — commands on the runtime command channel, not the
media-transport state this ADR's channels carry). It said nothing about
namespacing a control's own STATE channels by the thing that owns them.

`clock.rate` / `clock.play_state` / `clock.scrub` establish
**`owner.purpose` namespacing** as a legal, declared pattern alongside
bare scope-level names (`brightness`, `time`, `trigger`). A bare name
suits a project-wide concept with one obvious referent; an owner-prefixed
name suits a control that is legibly "this node's own dimension" even
before a reader has looked up what it feeds — `clock.rate` reads as the
clock's rate the moment you see it, the way `transport.pause` already
reads as a transport command. This is additive: nothing about the
existing bare-name convention changes, and the well-known channel
registry (`lp-core/lpc-model/src/bus/well_known.rs`) documents both forms
side by side.

### 6. The taxonomy follow-up, answered: `Debug` now describes a persistence contract, not a rendering location

**This amends `2026-08-01-debug-slots-taxonomy.md` follow-up (a).** That
ADR ratified `Debug` as provisional, with the clock's `rate`/
`scrub_offset_seconds` cited as the one example in the corpus that read
as transport rather than diagnostics, and a "revisit when the clock's
transport controls move to a transport surface" trigger.

The move happened (P3–P5, PR #345): the tape claimed the three rows, and
`retire_face_claimed_debug_rows` suppresses their generic flat-section
rendering wherever the face carries the transport block. P6/P8 (this
plan) then gave the same instrument a module-panel presence. With that
done, the answer is:

**Yes — `Debug` now holds only diagnostics in what actually renders
generically, and the name stands, but the reason is sharper than "the
tension resolved": this plan separates a Debug field's *role* from its
*presentation*.** The taxonomy's `Debug` category names a persistence
contract — transient by nature, no durable value underneath, session-only,
verb Clear — not a UI location. Before this plan, the flat hazard-striped
section was the only rendering that contract had, so the two were
conflated by omission: a Debug field meant a Debug-section row. The
clock's transport fields are unchanged in role (`SlotRole::Debug`, Q2 —
still transient by design, still no durable value to fall back to) but
now have a bespoke instrument instead of the generic row, and the
instrument itself carries the contract's obligations directly — the
attention-orange tint on a changed control, and a `clear` affordance that
drops the override (`ClockFace`'s header clear, `TapeTransport`'s
per-control clears). A Debug-role field can now legitimately render
either way; both satisfy the same contract.

The `DebugSlotsSection` component itself is **not** dead code — it was
evaluated and kept during P5 (not re-litigated here): `OutputDef
::test_pattern` is a real, in-tree Debug field with no bespoke surface,
and remains the section's one occupant. That is exactly the state the
taxonomy predicted as the good outcome — Debug holding "exactly the
diagnostics it describes" — with `test_pattern` as the corpus proof that
the generic rendering still has a genuine job.

### 7. Panel exposure by default, and why: the transport is the primary speed control

`ClockDef::transport` carries `panel = "show"` unconditionally — the
transport is exposed to the module panel by default, with no authoring
step. This is a product decision, not just a mechanical consequence of
§3: Yona, mid-planning — "this may well be how most users control the
app speed. All the per-phasor controls… might not be used that much." The
module panel is what a user loads on their phone to run a show; the
per-phasor Speed knobs (`2026-08-04-time-is-a-product.md` D11) that
already live there are a fine-tuning tier for one shader's rate, while
the transport is the one control that changes the whole show's pace at
once. Defaulting it to visible — the same default-exposure pattern the
fixture brightness fader already established — means a fresh project's
panel is never missing the control most users will reach for first.

### 8. The panel got the strip: no stripless fallback shipped

P8's plan explicitly reserved a documented fallback — dropping the tape
strip from the panel variant if it did not read at phone width, keeping
fader + run/pause + digits only. It was not needed: the whole instrument,
strip included, fits at `sm` (390px), and G2 judged the fit rather than
this ADR presupposing it. The fallback stays documented as the option it
was, not as something exercised.

## Consequences

- `apply_default_binding_overlay` (`project_controller.rs:1846`) keys
  facts by the binding's FIRST path segment only, so all three transport
  leaf defaults collapse onto the single `transport` row in the studio's
  authored-binding overlay display. Harmless for that overlay (it is a
  display aid, not the source of dispatch truth), but it means the
  grouped-control derivation cannot read per-leaf wiring FROM that
  overlay — it reads the graph directly instead
  (`node_face_builder.rs`'s `clock_transport_control`). Anyone extending
  the overlay to something dispatch-relevant needs to fix the collapse
  first.
- `OptionSlot`'s `some` is a plain `Field` path segment, not a `Record`
  one. The dotted binding-fact key builder must stop descending at
  anything that is not a `Record` — descending through an `Option` wrapper
  silently unwires every option-shaped control. This surfaced once
  (fixture brightness) during P8 and is recorded here because it is the
  kind of thing a second grouped control would rediscover the hard way.
- The `ClockTransport` rename's +12 B `frame.retained` tripped CI's
  heap-budget ratchet; the budget was re-baselined in the same change
  (`scripts/heap-budget-record.json`) rather than treated as a regression
  to chase — the ratchet's job is to force acknowledgment, not to forbid
  growth.
- `docs/debt/clock-transport-has-no-transport-ui.md` closes: its exit
  criteria (a transport surface with a position readout; the three
  controls moved onto it; the taxonomy naming re-check answerable) are
  met, on the card since P5 and on the module panel since P8, and its
  entry is finished alongside this ADR.
- Three `clock.*` entries join the well-known channel registry
  (`lp-core/lpc-model/src/bus/well_known.rs`), which is what makes them
  discoverable in the binding picker with real kind hints rather than
  merely legal to type.

## Alternatives Considered

- **A — whole-record channel with engine hot-path support for record-
  shaped dataflow reads.** Rejected: solves a problem the clock itself
  does not have (nothing reads the transport as one unit), costs a
  hot-path rework of the exact-`(node, path)` binding index specifically
  to avoid, and still carries the RMW race §1 describes.
- **C — no channels; panel dispatches plain slot edits.** Rejected as a
  regression to the pre-panel behavior: legal, but gives up bus presence,
  live echo, and per-dimension modulation for no correctness win over B.
- **A general `UiPanelWidget` registry for grouped controls**, instead of
  a match arm keyed on shape id. Deferred deliberately: the clock's
  Transport is the only grouped control that exists; a registry built
  for a population of one is speculative generality, and a second
  grouped control is a better, cheaper trigger to build one against than
  a guess now.
- **Keeping `running: bool`** with the polarity documented in the well-
  known channel's doc string. Rejected (D20): documentation a reader has
  to already know to consult is exactly the failure mode a self-
  describing value avoids; `PlayState` costs one enum and removes a whole
  class of "which way does true go" bugs at every future call site.
- **A stripless panel fallback** (fader + run/pause + digits, tape strip
  dropped) for narrow widths. Documented as a fallback in P8, not shipped
  — the full instrument fit at `sm` width, so there was nothing to fall
  back from.
- **Dial, platter, and hairline tape surfaces**, and **tenths-based fader
  detents** — the spike's round-1 and round-2 rejects
  (`spikes/clock-transport-hero/index.html`). Dial/platter/hairline read
  as decoration rather than a draggable instrument; tenths detents did
  not feel like discrete gears the way octaves do.
- **Speed-linked zoom** (constant pixel velocity, adaptive tick ladder).
  Liked at the spike's round 2, shipped through early plan phases, then
  reversed by Yona mid-plan in favor of fixed zoom, which G1 confirmed
  felt right for THIS surface. Banked as the design record for the
  future piano-reel input recorder, where variable time-per-pixel is the
  point rather than a distraction. The accompanying actual-velocity
  variant (pixel speed also scaling with rate) was an open note tied to
  the zoom idea and is retired with it — not proposed here, and not
  future work this plan owns.

## Follow-ups

None opened by this ADR. It closes two open items already indexed in
`docs/adr/README.md`'s Deferred Decisions table — the taxonomy's
`Debug`-naming follow-up (a) and `2026-08-04-time-is-a-product.md`'s
"transport UI is still owed" item — both struck there in the same change
that adds this ADR.
