# ADR: Slot taxonomy — Settings, Panel, Debug, State

- **Status:** Accepted
- **Date:** 2026-08-01
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-07-04-studio-editing-model.md` (D2 introduced writable
  transient slots; this ADR names and bounds them);
  `2026-07-27-runtime-node-command-channel.md` (reconciled below —
  unchanged); `2026-07-05-studio-pane-grammar.md` (gains the debug colour
  family); `2026-07-09-declarative-default-bindings.md` (retrofitted the
  `read_only_transient` state records this ADR retires);
  `2026-07-27-node-authoring-operations.md` (its operation contract is
  unchanged; its `#[slot(policy = "read_only_persisted")]` spelling is
  superseded by `#[slot(role = "fixed")]`); `docs/design/panel.md` +
  `docs/glossary.md` (the panel vocabulary this taxonomy must not collide
  with)

## Context

`SlotPersistence { Persisted, Transient }` shipped 2026-05-12 with the clock
node as a pure, unconsumed hint; the plan that added it explicitly deferred
"the config/params/controls/state taxonomy" as future work. It stayed inert
for two months. The 2026-07-04 studio editing milestone then activated half
of it (D2: transient edits are staged, run live, and are retained across
save), and a later guardrail retrofitted `read_only_transient` onto seven
produced-state records to stop them presenting as editable. One variant
ended up carrying three unrelated meanings:

1. **writable session controls** — exactly one shape, the clock's
   `controls.{running, rate, scrub_offset_seconds}`;
2. **produced runtime state** — seven state records marked read-only
   transient, which is a fact about their *direction*, not a policy;
3. **studio-synthesized** — the view builder rewrote every produced-direction
   slot to `read_only_transient` at DTO-build time, papering over records
   nobody had marked (`TextureState` was safe only because of this patch).

The user-facing story was thinner still: a clean transient slot was
pixel-identical to a persisted one, so you could not know a value would never
be saved until after you edited it; transient edits appeared in the Save
panel as a "Live" bucket; and `node.schema.json` advertised fields the writer
refuses to ever write.

Meanwhile the nested-modules line landed **panel state** on main
(`docs/design/panel.md`, PR #230): unauthored per-`(scope, channel)` writer
state, persisted to `.lp/state.json`, never dirty. Two mechanisms now
answered the same user question — *does a live control value survive a
restart?* — in opposite ways. Nailing down the product language became the
point of the work, not a side effect.

## Decision

### The language

| Category | Meaning | Persistence |
|---|---|---|
| **Settings** | Authored node config | In the artifact; Save/dirty |
| **Panel** | End-user control surface; fronts any slot (usually a Setting). Authored value = default, panel writer = live override | `.lp/state.json`; never dirty |
| **Debug** | Transient BY NATURE — diagnostics/authoring overrides, no durable value underneath | Session only; dies on unload/reboot |
| **State / Outputs** | What the runtime produces | Never authored; declared `State` role, cross-checked against `direction = Produced` |

**G2 amendment (2026-08-02).** The fourth category was ratified at D1 as
*direction-implied* — produced state needed no role, because
`direction = Produced` already made a field read-only and never-serialized.
Yona rejected that at the final gate: "we don't have an Output/Produced
policy… my instinct is that it should be explicit." Declaration beats
inference. `SlotRole::State` now exists and every produced field declares it,
with `role_matches_direction` rejecting **both** halves of a mismatch — a
`State` field that is not produced, and (the case that let `TextureState`
sit unmarked for two months) a produced field that is not `State`. The
classification is unchanged: `State` + `Produced` resolves exactly as an
unmarked produced field did. What changed is that the marking is now
mandatory and machine-checked instead of merely implied.

### The taxonomy line

**Events → command channel; ephemeral state → Debug slots; produced state →
the `State` role (direction-checked).**

This is the sentence that makes the three mechanisms one system instead of
three accidents. Each ephemeral thing gets exactly one home, chosen by what
it *is*, not by which machinery is convenient:

- an **event** has no value to hold (activate this entry, press this button)
  — it is a poke on the runtime command channel;
- **ephemeral state** has a value that must persist for the session and reach
  the engine every frame — it is a Debug slot riding the overlay;
- **produced state** is written by the runtime and read by everyone else — it
  needs no marking at all, because `direction = Produced` already says
  read-only and never-serialized.

### Panel is not Debug (the boundary)

Panel and Debug both look like "a live value a user manipulates that is not
in the artifact", and they are not the same thing:

- **Panel is an *exposure* mechanism over any slot.** Anything can be exposed
  as a panel control — usually a Setting — and exposure IS binding: a control
  fronts a slot **via its bound channel** (`modules.md` R3, binding =
  publicity; control identity is `(scope, channel)`, never the slot itself).
  The authored value is the **default**; the panel writer is a **live
  override** on top of it. That is precisely why *latching* panel state
  persists to `.lp/state.json` and why `panel.md` says a Control "is NOT a
  slot": it *fronts* one. A show that was tuned on the panel must come back
  tuned. (Momentary panel controls — `panel.md` P14 — are session gestures
  that never persist and are still Panel, not Debug: their fallback is bus
  resolution, not a shape default.)
- **Debug is *transient by nature*.** There is no durable value underneath —
  nothing authored to override, nothing to come back to. It is session-only
  and dies on unload/reboot, **deliberately**: a rebooted installation must
  not come up in test-pattern mode.

So there is no retention *axis* to name. Panel state persists because it
overrides durable settings; Debug does not persist because there is nothing
durable to override. The two vocabularies coexist: a slot may be a Setting
exposed on a panel, or a Debug slot, and the Clear verb means the same thing
in both worlds (drop the override, fall back to what is underneath — for
Debug, the shape default).

### `SlotRole` replaces the policy axes

`SlotPolicy { writable, persistence }` is gone. Its replacement is
**`SlotRole::{ Setting, Fixed, Debug }`**, with writability implied
(Setting/Debug writable, Fixed read-only) and direction supplying the rest:

| Former policy | Now |
|---|---|
| `writable_persisted` (the default) | `Setting` |
| `read_only_persisted` (3 `ProjectDef` fields) | `Fixed` |
| `writable_transient` (clock ×3, `output.test_pattern`) | `Debug` |
| `read_only_transient` (7 state records) | *nothing* — `direction = Produced` implies it |

Declaration sites read `#[slot(role = "debug")]`. Persistence survives only
as a **derived** classification (`effective_persistence(role, direction)`) —
never a stored axis, never declarable, and the single function both the
studio (display, dirty accounting) and the registry (commit-time retention)
must consult, so the two sides cannot disagree about what an edit is. Paths
that resolve in no shape take one shared fallback,
`SlotPersistence::for_unresolved_edit()` — **Setting** — on both sides.

Retiring `read_only_transient` also retired the studio's view-build synthesis
patch: `TextureState` is now safe by construction (it declares its direction)
rather than by a rewrite in the DTO builder.

### Nothing authored is Debug

Debug values are session-only, which means they never appear in files:
schema-gen omits Debug fields from the authoring schema, the example projects
were scrubbed of their `controls.*` stanzas, the reader **warns and skips** an
authored Debug value rather than adopting it as a base, and the reset target
is the **shape default**, not whatever the file happens to hold. That last
one matters more than it looks: tying a Debug value's base to on-disk bytes
meant a commit could shift the base under the user and silently change what
Reset means. If shipping a preset ever matters, it is an explicit persisted
field feeding the control — never authored magic bytes in a Debug slot.

### Debug leaves dirty and save entirely; its verb is Clear

A Debug value is not an edit, so it carries no dirty weight: it counts in no
`DirtySummary` bucket, appears in no Save-panel section, and never arms the
unload gate. Warning about a knob turn would train users to dismiss the
dialog. Its verb is **Clear**, offered at three scopes (value, node,
project), matching the panel model's ratified Clear vocabulary. The mechanism
is unchanged — Debug values still ride the overlay to reach the engine, and
still survive a client disconnect because the overlay lives device-side.
Only the accounting and the presentation became honest. (A *failed* write to
a Debug slot still counts as failed: that needs attention.)

### Debug state must be unmissable, and the mapping lives in one place

A system in a debug state must announce it. Three tiers: a global header chip
whenever any override is active anywhere ("Debug active · N · Clear all"),
a marking on the node card that carries one, and the Debug **section** styled
as debug territory **always — even idle**, which is what structurally fixes
the clean-transient invisibility problem (you know before you touch it). The
section is policy-derived and **flattened**: any Debug field lands in it
regardless of which record declared it, so a node author gets correct UI
purely by marking a field.

Architecturally the treatment is split: `lpa-studio-core` carries a distinct
**semantic** variant (`UiAffordance::Debug`), and the attention-orange +
hazard-stripe rendering lives only in the web presentation layer — one
mapping seam. Changing the visual later is one edit, not a call-site hunt.
The colour family itself is recorded in `2026-07-05-studio-pane-grammar.md`.

### Naming: provisional but ratified

`Debug` is not a perfect word, and it is the ratified one: every example in
the corpus is authoring/diagnostic rather than performance — drive the clock
by hand to inspect a show, force a wiring test pattern, simulate a press.
Refine later if a better word emerges. The known tension is the clock's
`rate`/`scrub_offset_seconds`, which read as transport rather than
diagnostics; the expectation is that they migrate to a transport surface,
leaving Debug holding exactly the diagnostics it describes.

**Resolved 2026-08-07** (follow-up (a), full answer in
`docs/adr/2026-08-07-clock-transport-is-a-panel-instrument.md` §6): the
migration happened, and the name outlives it — `Debug` turns out to name
the persistence contract these fields have always carried, not the
flat-section rendering that used to be its only presentation.

### Reconciliation with the runtime command-channel ADR

`2026-07-27-runtime-node-command-channel.md` **stays valid and is not
amended.** It rejected modeling an ephemeral poke as an overlay edit —
"it pollutes the Save panel with a row for something that is not an edit …
its semantics are dishonest". That verdict was about an **event** (activate
this playlist entry): an event has no value to hold, so an overlay edit would
have had to be staged and immediately un-staged, claiming an authored trigger
the user never wrote. The taxonomy line above says exactly that: events go to
the command channel.

Debug slots are the other branch. Ephemeral *state* does have a value, the
engine must see it on every frame, and the overlay is the mechanism that
already delivers exactly that. The Save-panel objection does not transfer,
because D7 removed Debug from dirty/save accounting entirely — the pollution
the command-channel ADR refused is now structurally impossible.

One follow-up of that ADR is **obsolete**: *"Sim button press and debug pokes
adopt the channel as new `WireNodeCommand` variants."* Button input is punted
to the input initiative — record/replay requires injecting at the input
*source* layer, so a per-node `ButtonEvent` command was the wrong injection
point — and "debug pokes" as a category is answered here by Debug slots, with
no wire change at all (`WIRE_PROTO_VERSION` stays 4). The channel itself
remains the right home for any future genuine event.

## Consequences

- The three mechanisms are now separable by a question the author can answer
  without reading code: does it have a value? does something durable sit
  underneath it? who writes it?
- `SlotPersistence::Transient` means exactly one thing — a writable live
  control — and produced-state protection no longer depends on anyone
  remembering to mark a record. A new state record is safe the moment it
  declares its direction.
- Debug values are absent from schemas, example projects, and save
  accounting, so "the file is the project" holds again: nothing on disk can
  be a Debug value, and nothing a user turns in a Debug section can dirty a
  project.
- Node authors get the Debug section, the hazard treatment, the global chip,
  and the Clear verbs for free by marking one field — `OutputDef.test_pattern`
  proved this end-to-end with zero output-specific UI code.
- The cost is a hard rename across four crates plus regenerated shape dumps
  (`"policy"` → `"role"`). It was paid now because there were only four
  declaration sites; it would not have stayed that cheap.
- Two vocabularies (Panel and Debug) now share the word "Clear" and the idea
  of an override. That is intended — they are the same gesture over different
  substrates — but it does mean the glossary, not the code, is where the
  distinction is taught.

## Alternatives Considered

- **Keep `SlotPolicy { writable, persistence }` and just document it.**
  Rejected: the axes cross-multiplied into combinations that could not be
  declared (an explicit `writable_persisted` inside a transient container is
  unrepresentable) and one of the four combinations was standing in for
  direction. A role enum covers everything that survives the taxonomy with no
  unreachable states.
- **Model the ephemeral cases as runtime commands** (the shape PR #233 built:
  wire proto 5, `ButtonEvent`, `OutputTestPattern`, per-node runtime state,
  TTL leases and renewal loops). Rejected for *state with a durable home*:
  the leases only existed because a command gave this state no home, and a
  Debug slot provides one. (Liveness leases stay legitimate where no durable
  home exists — e.g. momentary panel gestures over the wire, the modules
  roadmap's P9 — that is an event-shaped problem, not this one.) The collapse was
  enormous — no wire variant, no ops, no renewal — and the device-side
  lifetime (survives client death, dies on unload/reboot) is exactly the
  wiring-test semantic we wanted. PR #233 was closed unmerged after its
  engine bypass logic was harvested.
- **One "live values" concept covering panel state and Debug**, distinguished
  by a retention axis. Rejected (D5): they differ in *what is underneath*, not
  in how long they last. Panel state persists because it overrides an
  authored default; Debug has no default to override. A single concept with a
  retention flag would have made "does this survive a reboot?" a per-slot
  configuration question instead of a consequence of what the value is.
- **Persistent per-row tinting for Debug values** (mirroring the bound-violet
  convention). Rejected in favour of a separate section: location is a
  categorical signal that is present *before* the user touches anything, and
  it leaves the dirty chrome doing only its own job.
- **Record-shaped grouping for the Debug section** (a section per declaring
  record). Rejected: it breaks the moment one record mixes Setting and Debug
  fields, which nothing forbids, and it made the clock's section look
  correct only because that record happens to be named `controls`.
- **A new colour family for debug** (magenta/pink — the only genuinely unused
  hue). Rejected in favour of attention-orange plus hazard striping: form,
  not just hue, following the repo's stepped-knob precedent, and it keeps the
  palette from growing a family per state.

## Follow-ups

Per the deferred-decision convention, these are indexed in
`docs/adr/README.md`.

- **(a) `Debug` naming re-check.** Ratified as provisional. **Revisit when**
  the clock's transport controls (`rate`, `scrub_offset_seconds`) move to a
  transport surface and Debug holds only diagnostics — if the word still fits
  then, it is permanent. *2026-08-05: the move happened — the tape transport
  (plan `2026-08-04-2355-clock-tape-hero`, P3–P5) claimed the clock's rows
  into a real instrument on the clock card, and the drawer's remaining
  in-tree occupant is `test_pattern`, pure diagnostics.*
  **Closed 2026-08-07 — `docs/adr/2026-08-07-clock-transport-is-a-panel
  -instrument.md` §6.** The name stands, and the answer is sharper than
  "the tension resolved": `Debug` names a **persistence contract**
  (transient, no durable value underneath, verb Clear), not a rendering
  location. Before the tape, the flat hazard-striped section was the only
  rendering that contract ever had, so a Debug field implicitly meant a
  Debug-section row. The clock's transport fields keep `SlotRole::Debug`
  (Q2 — still transient by design) but now render as a bespoke instrument
  that carries the contract's obligations directly (attention-orange tint
  on a changed control, its own `clear` affordance) instead of living in
  the flat section. A Debug-role field can legitimately render either
  way; both satisfy the same contract. `DebugSlotsSection` is not dead —
  `OutputDef::test_pattern` remains its one occupant, which is the good
  outcome the taxonomy predicted: Debug holding exactly the diagnostics
  it describes. *2026-08-10 amendment: it gained a second occupant,
  `OutputDef::highlight` — the patch-selection pulse
  (`2026-08-10-patch-selection-pulse.md`), again pure diagnostics.*
- **(b) Debug indication on preview/play surfaces.** D8 covered the workspace
  (chip, card, section) only; a running installation showing a test pattern
  has no indication outside the editor. **Revisit when** the panels/play-mode
  work defines its own chrome — the indication belongs there, not bolted onto
  the node card.
- **(c) `test_pattern` colour.** `TEST_PATTERN_RGB` is full white (the
  max-current case on long strips), deliberately chosen for pin discovery.
  **Revisit when** someone runs it on a long strip and wants it dimmer; it is
  a one-constant change.
