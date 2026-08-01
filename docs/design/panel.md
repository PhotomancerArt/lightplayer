# Panel — Controls, State, and Persistence

This document is the single source of truth for the **panel**: the control
surface every node presents, and the runtime state behind it. It is the
sibling of [`modules.md`](./modules.md), which defines where panels *come
from* (publicity, channels, scopes); this document defines how controls
*behave*. The seam between the two is deliberately narrow (§4).

> **Status: Ratified 2026-07-31** (gate G1 of
> `planning/2026-07-31-1002-modules-buses-panels`). The open-question
> register (§5) records accepted leans — revisit at implementation, not
> before.
>
> **Posture.** The panel is the product surface — persona 3 ("end users
> control the art") lives entirely here. Two commitments shape every rule
> below:
>
> 1. **The panel is a lighting-console programmer.** Authored dataflow
>    plays; touching a control *captures* its channel and holds it until
>    an explicit clear — DAW **Latch** automation, not Touch. Chosen
>    because the canonical use is unattended hardware (see P10's scenario):
>    a value set from a phone must survive the phone leaving.
> 2. **Controls shape, they never integrate.** A panel writer holds a
>    value and may smooth its trajectory toward it; it never accumulates
>    state that *is* the signal. Anything that integrates (a phasor
>    turning `speed` into continuous `phase`) is a **node** inside the
>    module — ordinary authored dataflow and bus vocabulary, not a panel
>    mechanism.
>
> **Related:** `docs/design/modules.md` (publicity, scopes, resolution),
> `docs/glossary.md` (terms),
> `docs/adr/2026-07-26-node-card-faces.md` (widget grammar the panel
> renders with).

## 1. Concepts

| Term | Is | Is not |
|---|---|---|
| **Control** | The presentation of one channel in one scope, on a panel. | A value holder; a slot; per-widget state |
| **Panel writer** | The runtime writer behind an engaged control: per `(scope, channel)`, unauthored, engine-side. | Authored; a node; UI-side state |
| **Read** | Control state while no panel writer exists: displays the channel's resolved live value (authored / inherited / default). | "Disabled" |
| **Engaged (Latch)** | Control state after first touch: the panel writer exists and holds, shadowing other writers in its scope until cleared. | Released by letting go (that would be Touch mode) |
| **Clear (reset)** | Removing panel writers — per control, per module, or whole panel — restoring Read. | An edit; an undo of authored values |
| **Slew** | Optional shaping of a writer's output *toward* its held value (anti-zipper). | Integration; a signal generator |
| **Takeover** | The policy for how a grab of a moving (automated) channel acquires it. | Needed for Read channels at rest (grab is trivial) |
| **Momentary** | The control class for gesture channels (touch sets, buttons): writes while active, despawns on release, never latches or persists. | A user-selectable mode; Touch automation |

## 2. Control rules (normative)

### P1 — Identity

A control is identified by `(scope, channel)`. All state — engagement,
held value, persistence — keys off that pair. Widgets, faces, and clients
are views; two phones showing the same control show the same state (P9).

### P2 — The state machine is Read → Latch → Clear

- **Read**: no panel writer. The control displays the channel's resolved
  live value and whose it is (inherited / authored / default — the UI
  distinguishes these; an unwritten channel shows the consumer's default
  per modules.md R6).
- **First touch** materializes the panel writer *in the scope where the
  control lives* (lazy — modules.md R10) and captures the channel:
  **Latch**. Releasing the knob changes nothing; the value holds across
  frames, disconnects, and reboots (P10/P11).
- **Clear** removes the writer(s) and returns the control(s) to Read.
  Granularity: one control, one module's panel, or the whole panel. Clear
  is always one obvious gesture on every panel surface.

There is no Touch mode (release-returns-to-automation) and no Write mode
(recording into authored artifacts). Panel gestures never author. The
Read → Latch → Clear machinery governs **value controls**; gesture-natured
controls are **momentary** and follow P14 instead.

### P3 — Shape, don't integrate

A panel writer's entire state is: **held value** (+ optional slew toward
it). Slew is presentation-grade smoothing (anti-zipper on brightness
grabs), defined per widget kind with a project-wide default; it never
changes where a value *ends up*, only how it settles. Writers that
accumulate (`phase += speed·dt`), oscillate, or generate are forbidden —
that behavior is a node's. The supported idiom for phase-continuous speed
is: `speed` channel → phasor **node** → `phase` channel → consumer, with
`phase` as the bus-vocabulary convention example modules establish.

### P4 — Precedence (restates modules.md R11)

Within its scope, an engaged panel writer outranks authored writers for
the same channel until cleared. Across scopes, ordinary writer-shadowing
applies: an engaged writer in an inner scope shadows outer writers for
that subtree — touching detaches, clearing re-attaches.

### P5 — Takeover: jump, by default

Grabbing a channel that authored dataflow is actively moving acquires it
**immediately at the gesture's value** (jump). Rationale: on-screen
touch/drag controls carry the user's absolute intent, and performance
response must be instant. *Pickup* (control is inert until it crosses the
current value) and *scaled* takeover exist for future absolute hardware
inputs (MIDI faders — P13) and are a per-input-binding policy, not a
panel-wide mode.

### P6 — Display: live value always; meta from the binding

- A control always displays the channel's **resolved live value** — in
  Read that's whatever writer wins (watch the LFO move the knob); in
  Latch that's the held/slewing value.
- Engaged controls carry a distinct affordance (not the bound-violet
  family — bound means "wired", engaged means "captured"; the UX spike
  owns the treatment).
- Display meta (label, unit, range, step, widget kind) derives from the
  slot(s) currently bound to the channel in that scope, re-derived on
  binding change. Merge rule: numeric ranges union (widest wins); on
  label/unit conflict the channel name wins; a module-level authored
  per-channel override (modules.md R9) beats derivation.

### P7 — Meta changes under a held value: preserve raw, clamp emission

When derivation changes a control's range (a playlist switches entries;
a binding is edited) and a held value falls outside the new range: the
**raw held value is preserved**, the **emitted value clamps** to the
current range, and the control renders pinned at the edge. Switching back
restores the raw value exactly. (Consumers declared the range; feeding
them out-of-range values is meaningless — but the user's setting is never
silently destroyed.)

### P8 — Wire: runtime commands, never authored ops

- `PanelWrite { scope, channel, value }` — first write materializes the
  writer (P2). Drag streams write at input rate, coalesced per
  `(scope, channel)` (the slot-edit coalescing precedent); the engine
  applies the latest value per tick.
- `PanelClear { scope?, channel? }` — absent fields widen the clear
  (channel-level → scope-level → everything).
- Both are runtime pokes on the playlist-activate pattern: nothing
  staged, nothing dirty, no overlay interaction. Studio, play mode,
  phones, and future hardware inputs all speak exactly these two ops.

### P9 — Multi-client: the engine is the authority

Panel state lives in the engine (sim or device), never in a client. All
connected clients render the same resolved values and engagement flags;
concurrent writes to one control are last-writer-wins at the engine (a
knob fight converges on whoever moved last — acceptable; no locks, no
ownership). Clients learn state changes through the normal probe/refresh
path — two phones on one device stay in agreement.

### P10 — Restore-on-boot: before first render

Persisted panel state loads and rematerializes engaged writers **before
the first frame renders**. The defining scenario (verbatim requirement):
*4 a.m., Burning Man, LED scarf dimmed from a phone; unplug, replug — it
must come back dim, with not one bright frame; next night, connect and
reset.* A boot that renders even one frame at authored brightness before
applying restored panel state is non-conforming.

### P11 — Persistence

- Panel state persists to **`.lp/state.json`** in the project folder
  (the framework-owned tier — modules.md §6); on device, to the
  device's own filesystem. Never in authored artifacts.
- Contents: a versioned map `scope-path / channel → { value }` — raw
  held values (P7); engagement is implied by presence. Unknown scope
  paths are dropped on load (vendoring/renames degrade gracefully).
- Writes are **throttled (≥ ~10 s apart)** for flash preservation, with
  a flush on clean shutdown/disconnect where the platform allows.
- **Auto-save is on by default** with a user toggle; Clear (P2) removes
  the corresponding persisted entries immediately.

### P12 — Play mode

Play mode renders **panels only** — the root module's panel, which
recursively presents nested module groups (modules.md R8) — no faces, no
authoring surfaces. It speaks only P8's two ops plus reads. Anything
play mode can do, an end user is allowed to do.

### P13 — External inputs (future, seam only)

MIDI, OSC, and hardware encoders enter as **panel writers through P8's
ops** — same identity, same latch or momentary semantics, same
persistence rules — with per-input takeover policy (P5). No second
control path will be added for them.

### P14 — Momentary controls: the gesture class

Some channels carry **live gestures**, not settings: multi-touch sets
(an XY pad), momentary buttons. Their controls are **momentary**, and
the class is intrinsic to the channel/widget kind — never a user-facing
mode switch.

- A momentary writer streams values while the gesture is active and
  **despawns on gesture end**. There is no held value, no Latch, and
  Clear is a no-op on them.
- **The despawn is the fallback mechanism.** Releasing the pad removes
  the writer, and resolution immediately falls through (modules.md
  R5/R6) to whatever is next — an inherited input in an outer scope, or
  nothing, letting an idle-generator *node* take over (see modules.md
  E7). No arbitration machinery exists beyond the writer lifecycle
  value controls already use.
- Momentary state is **never persisted** — P11 does not apply.
- Takeover is trivially jump (P5): a gesture *is* the value.
- Semantic emptiness is not the panel's problem: a connected input
  producing an *empty* gesture set (a camera seeing nobody) is a live
  writer writing "empty". Deciding that empty-for-N-seconds means idle
  is domain logic and belongs to a node (a gate with a timeout param),
  never to writer resolution.

## 3. Worked walkthroughs

- **The scarf**: boot → P10 restores `brightness` writer before frame 1
  (dim, no flash) → next night, Clear-all from the phone → P2 returns
  brightness to Read (authored default) → P11 drops the entry.
- **Grab the LFO**: an LFO node drives `hue`; in Read the knob visibly
  follows it (P6). Grab: jump-takeover (P5), writer materializes (P2),
  LFO is shadowed in this scope (P4) — the ride continues elsewhere if
  outer scopes consume it separately. Clear: the knob falls back into
  the LFO's motion.
- **Playlist switch under a held value**: entry A's `speed` (0–10) held
  at 7; switch to entry B (0–1): B's *own* control state is untouched
  (different sink scope — different identity, P1). Switch back to A:
  still 7. If instead one control's *meta* shrinks under a held value
  (binding edit), P7: emit 1.0 (clamped), remember 7.

## 4. The seam with modules.md

The bus provides the panel: **channels per scope, writer sets, and
resolution** (publicity R3, locality R4, shadowing R5, listing R6).
The panel provides the bus: **exactly one new writer kind** — the panel
writer (unauthored, lazy, latching, persistable). Neither document
reaches deeper than that: modules.md never specifies control behavior;
this document never specifies resolution.

## 5. Open questions (G1 redline register)

- **P-Q1:** slew defaults — which widget kinds slew at all (brightness
  fader: yes; stepped knob: no?), and is the time constant per-widget
  meta or one project default?
- **P-Q2:** engaged-affordance treatment (distinct from bound-violet) —
  UX spike owns the visual; confirm the *requirement* that Read-following
  -automation, Read-at-default, and Latch are three visibly distinct
  states.
- **P-Q3:** `state.json` schema version field name/shape, and whether a
  clean-shutdown flush is feasible on device (or throttle-only).
- **P-Q4:** does Clear-all also clear *sink-scope* (playlist entry)
  state, or only visible panels? Lean: everything under the cleared
  scope, sinks included — "reset means reset".
- **P-Q5:** the touch-set value shape (per-touch id, position,
  pressure/z, velocity — carried or derived?) and the multi-XY pad
  widget spec. Prior art: old lightPlayer's `MultiTouchInput.Touch`
  (id, initial/current x·y, z, velocity, interpolation). Vocabulary
  item — modules.md §7.
