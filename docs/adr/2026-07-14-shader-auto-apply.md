# Shader auto-apply: keep-last-good engine + always-on debounced apply

- Status: accepted
- Date: 2026-07-15 (planned 2026-07-14)
- Revises: the "Explicit Apply, no auto-apply" bullet of
  [2026-07-04-studio-editing-model.md](2026-07-04-studio-editing-model.md)

## Context

The M1/M2 editor chrome (PR #78) made GLSL editable in place, but every
edit needed a manual Apply (⌘↵) before the running project reflected it —
which killed the live-editing feel that is a core part of LightPlayer's
UX. The 2026-07-04 editing-model ADR had deliberately deferred auto-apply:
a mid-keystroke bad compile *stopped the node's rendering* (the engine
dropped the compiled program the moment new source arrived), so applying
half-typed shaders automatically would have blanked the output on nearly
every keystroke pause. That ADR said to revisit once
old-shader-keeps-rendering exists. This is that revision, as the minimal
slice: keep-last-good in the engine plus an always-on debounced client
apply (PR #92).

## Decision

### Keep-last-good node contract (both shader node kinds)

A shader node keeps producing with its **last successfully compiled
program** while a newer source or config revision compiles — or fails to.

- On source/config refresh the old program is retained; only the stale
  error is cleared. Compilation of the new revision happens in the render
  path, as before (synchronous, ~194 ms on device / ~82 ms emu).
- On compile failure the old program keeps rendering and the node status
  reports the **newest revision's** error. The output never blanks on a
  bad apply.
- Failed revisions **latch** (`needs_compile` flag): a broken source
  compiles at most once and is retried only when the source or config
  changes again — no per-frame recompiles.
- The first successful compile swaps in atomically.

Accepted costs: one long frame per apply (the synchronous compile stall),
and a transient 2×-JIT-buffer window while old and new programs coexist
during the swap (relevant to ESP32 pool sizing; typical shaders are
small).

### Auto-apply UX contract

- Edits apply themselves **500 ms** after the last keystroke (fixed const
  `AUTO_APPLY_DEBOUNCE_MS`). The debounce is epoch-guarded (only the
  newest keystroke's timer fires), waits politely while an apply is in
  flight, and **never auto-retries** a failed/oversize apply — editing the
  text re-arms it.
- Auto-apply is **always on**: no toggle, no Apply button. Apply-vs-Save
  is the two-axis model — applying is automatic and transient, saving is
  deliberate. ⌘↵ remains as a silent apply-now keymap.
- After an accepted apply the client runs a short **verdict chase**
  (250 ms × 3 refresh ticks) so the compile verdict lands promptly instead
  of waiting for the regular pull cadence.
- The editor bar is the **gentle two-half bar**: constant geometry in
  every state, color-only animated transitions, left half = compile/apply
  truth (identity, subtle applying dot, truncated error), right half =
  persistence state (Saved/Unsaved, always-mounted Revert and Save).
  Error state and persistence state display independently.
- **Revert is always available**, including during errors: it clears the
  applied edit and the engine returns to the saved program via the same
  keep-last-good recompile path.

### Manual-save firewall

Save-to-disk (⌘S) stays manual. Applied edits live in the device's RAM
overlay; flash holds the last manually saved content. A reboot therefore
sheds a bad session's applies — the flash copy is the **crash firewall**.
Auto-save-to-disk was explicitly rejected (a larger product decision,
deliberately deferred).

### Fuel posture

The JIT has no fuel/metering yet, so a mid-edit infinite loop in applied
shader code still hangs the render until the `FrameKind::NodeRender`
watchdog resets, blames, and blocks the node — survivable but ugly.
Shipping auto-apply ahead of fuel was an explicit decision: the recovery
path bounds the damage (editor text is client-local and survives the
reboot), and **fuel in `lpvm-native`** (back-edge counters in the emitted
RV32) is the planned fix with its priority raised by this work.

## Consequences

- Live editing works end to end: type → pause 500 ms → the running
  shader updates; a broken edit shows its error in the bar while the last
  good program keeps rendering; Revert works from any dirty state.
- Node status after a bad apply reflects the newest revision's failure
  even though the output is the older program — status is about the
  *latest revision*, output is about the *latest success*. Consumers of
  node status must not infer "no output" from an error status.
- Every pause-in-typing now costs a device compile (~194 ms frame stall).
  The debounce bounds the rate; the async/budgeted compile remains future
  work if the stall ever matters.
- The transient 2× shader-memory window exists on every apply.

## Alternatives Considered

- **Auto-save to disk** — rejected (U1): removes the crash firewall;
  bigger product decision than live-apply.
- **Client-side parse gate before apply** — rejected: duplicates the
  GLSL frontend client-side and diverges from the device compiler; the
  device's own verdict is the truth worth showing.
- **Waiting for fuel before shipping** — rejected (U3): the
  watchdog-reset → blame → block recovery path bounds the damage today;
  fuel lands independently.
- **Budgeted/async compile driver** — deferred: the full solution to the
  compile stall and the original condition in the 2026-07-04 ADR; the
  minimal keep-last-good slice delivers the UX now without it.
- **Auto-apply toggle (default ON)** — planned initially (D3), removed at
  the review gate (U6): always-auto is simpler and the apply/save split
  already expresses the intent a toggle would.

## Follow-ups

- **Fuel/metering in `lpvm-native`** — raised priority; own plan.
- Budgeted/async shader compile (spread the ~194 ms across frames).
- Per-artifact save op, structured compile diagnostics on the wire,
  auto-save-to-disk — all recorded as deferred product decisions.
