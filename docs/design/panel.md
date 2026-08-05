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
> **Related:** `docs/design/modules.md` (publicity, scopes, resolution;
> §10 carries the shared future-work register),
> `docs/glossary.md` (terms),
> `docs/adr/2026-07-26-node-card-faces.md` (widget grammar the panel
> renders with),
> `docs/adr/2026-08-02-panel-writers-and-state-persistence.md` (the
> writer tier and its persistence),
> `docs/adr/2026-08-03-panel-visibility-is-derived.md` (what reaches a
> panel at all).

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
that behavior is the clock's. The supported idiom for phase-continuous
speed is the **clock's time product**: a shader declares a
`phasor`-kind uniform, the engine's timebase store integrates it, and
the panel's Speed knob writes a `PhasorConfig` onto the slot's config
channel — every reader of that channel rides the one integrator it
retunes (ADR 2026-08-04-time-is-a-product; shipped 2026-08-04,
PR #328). The knob holds a value like any panel writer; the phase
lives with the clock.

### P4 — Precedence (restates modules.md R11)

Within its scope, an engaged panel writer outranks authored writers for
the same channel until cleared. Across scopes, ordinary writer-shadowing
applies: an engaged writer in an inner scope shadows outer writers for
that subtree — touching detaches, clearing re-attaches.

> Status: implemented 2026-08-02 (engine writer store at panel priority,
> replacing the scope's provider set — max-priority-wins holds on ByKey
> merge channels too; survives `apply_project_changes` by construction).

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

> Status: implemented 2026-08-02. `WireProjectCommand::PanelWrite` /
> `PanelClear` (project-level arms — they address a scope, not a node),
> `PanelWriteOp` / `PanelClearOp` in Studio, coalesced per
> `(scope, channel)` in the actor's batch planner beside the slot-edit
> flood rule. The panel controls that already existed (shader uniform
> knobs, the fixture brightness fader) were re-pointed off
> `SlotEditOp::SetValue` onto this path wherever the backing slot
> consumes a bus channel; a control with no channel behind it still
> edits its authored default.

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

> Status: implemented 2026-08-02. The seam is Engine construction —
> `Project::new`, and `Project::reload` because it rebuilds the Engine —
> so restore completes before the first tick and therefore before the
> first render. On device that is the boot path (`auto_load_project` runs
> ahead of the main loop). `apply_project_changes` does NOT rebuild the
> Engine, which is why an ordinary edit leaves engaged writers alone and
> touches no file.

### P11 — Persistence

- Panel state persists to **`.lp/panel.json`** in the project folder
  (the framework-owned tier — modules.md §6); on device, to the
  device's own filesystem. Never in authored artifacts.
- Contents: a versioned map `scope-path / channel → { value }` — raw
  held values (P7); engagement is implied by presence. Unknown scope
  paths are dropped on load (vendoring/renames degrade gracefully).
- Writes are **throttled (≥ ~10 s apart)** for flash preservation, with
  a flush on clean shutdown/disconnect where the platform allows.
- **Auto-save is on by default** with a user toggle; Clear (P2) removes
  the corresponding persisted entries immediately.

> Status: implemented 2026-08-02 (`lpa-server/src/panel_state.rs`;
> ADR `2026-08-02-panel-writers-and-state-persistence.md`). The USER
> TOGGLE reached the UI 2026-08-03: `WireProjectCommand::PanelAutoSave`
> plus `ServerRuntimeStatus.panel_auto_save` (wire proto 9 — the current
> value rides every project read rather than a dedicated pull), rendered
> once on the project's ROOT module face, since the state file is
> per project folder.
>
> **Device-first, per settled D-B**: both sim tiers run on `LpFsMemory`,
> so sims stay ephemeral by construction; the unit tests are the
> correctness story and the device walk confirms it. Persistent sims are
> recorded future work.
>
> The throttle gates on a writer-store mutation COUNTER, not on the
> writer set: a clear followed by a re-write inside one window leaves an
> identically-shaped map, so comparing size-and-revision would miss it.
> An idle project writes nothing at all. Turning auto-save off records
> itself in the file, so the choice survives a reboot instead of quietly
> re-enabling overnight.
>
> **Prerequisite that made this safe:** a write inside the project fs
> fires an FsEvent back at the artifact-refresh path, so `/.lp/**` is
> filtered out of project changes before anything reads the batch —
> otherwise every save would rebuild the binding graph and the rebuild
> would schedule the next save. `Project::applied_refresh_count` makes
> that observable rather than assumed.

### P12 — Play mode

Play mode renders **panels only** — the root module's panel, which
recursively presents nested module groups (modules.md R8) — no faces, no
authoring surfaces. It speaks only P8's two ops plus reads. Anything
play mode can do, an end user is allowed to do.

> Status: implemented 2026-08-03 — mounted at
> `#/sim|device/<key>/play`, the same session as the editor route (the
> segment changes the surface, never the runtime). What a panel PUBLISHES
> is `docs/adr/2026-08-03-panel-visibility-is-derived.md`.

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

> Status: implemented 2026-08-02 (engine-side lifecycle). A momentary
> write carries a renewal deadline; the engine despawns the writer past
> it, in the tick. That makes despawn survive a dropped client — a
> gesture nobody is renewing releases on its own — and renewal is simply
> the next write. The wire shape is our own; PR #233's TTL/press_id
> direction was design reference only. Widget-side gesture classes (which
> channel kinds ARE momentary) arrive with the touch-set vocabulary,
> P-Q5.

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
  *Disposition 2026-08-02: still open, deliberately unimplemented.*
  Emission is immediate (P5 takeover = jump), which is correct behavior
  on its own and not a placeholder. The seam when slew arrives is
  **writer-side shaping**: the panel writer holds the raw value (P7) and
  what it emits is shaped on the way out, so nothing downstream of the
  writer — resolution, persistence, identity — changes.
- **P-Q2:** ~~engaged-affordance treatment (distinct from bound-violet) —
  UX spike owns the visual; confirm the *requirement* that Read-following
  -automation, Read-at-default, and Latch are three visibly distinct
  states.~~ **Requirement CONFIRMED and shipped 2026-08-03** (gate GV):
  the three states are visibly distinct and walkable — Read-following
  -automation names its driver, Read-at-default reads its authored value,
  and Latch is amber with an off-flow reset glyph that never reflows the
  control. Two threads stay open and are NOT to be changed without Yona:
  (a) amber (`status-attention`) may be too intense for "held" — he leans
  maybe-blue, "more thinking needed"; (b) the treatment still borrows
  `status-attention` rather than a minted `status-engaged` token family.
- **P-Q3:** ~~`panel.json` schema version field name/shape, and whether a
  clean-shutdown flush is feasible on device (or throttle-only).~~
  **Settled 2026-08-02.** The file is
  `{ version, auto_save, entries: [{ scope, channel, value }] }` with
  `version: 1` and **bump-and-refuse** semantics — an unknown version is
  ignored wholesale, never migrated, matching the alpha posture in the
  rest of the format story. Losing panel state costs one re-dim; a
  half-applied migration costs trust. A clean-shutdown flush IS included
  (project unload flushes past the throttle); an unclean power cut simply
  loses at most one throttle window, which is the trade the ~10 s
  interval buys.
- **P-Q4:** ~~does Clear-all also clear *sink-scope* (playlist entry)
  state, or only visible panels?~~ **Settled 2026-08-02 as leaned:
  clear-all reaches sink scopes** — a playlist entry's latched value
  clears with everything else. "Reset means reset"; a reset that leaves
  values latched in scopes the user cannot currently see is a haunting,
  not a safety feature. Implemented and pinned by test.
- **P-Q5:** the touch-set value shape (per-touch id, position,
  pressure/z, velocity — carried or derived?) and the multi-XY pad
  widget spec. Prior art: old lightPlayer's `MultiTouchInput.Touch`
  (id, initial/current x·y, z, velocity, interpolation). Vocabulary
  item — modules.md §7.
