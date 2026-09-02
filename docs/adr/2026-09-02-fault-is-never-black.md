# ADR: A fault is never black — the `Fault` status level and the output fault pattern

- **Status:** Accepted
- **Date:** 2026-09-02
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-09-01-2026-fault-is-never-black` (PR #496)
- **Fixes:** `docs/defects/2026-09-01-silent-black-under-node-quarantine.md`
- **Supersedes:** None
- **Superseded by:** None
- **Related:** [2026-07-04-crash-recovery-model.md](2026-07-04-crash-recovery-model.md)
  (extends the "richer Studio recovery UI" follow-up),
  [2026-08-02-rv32-firmwares-are-abort-tier.md](2026-08-02-rv32-firmwares-are-abort-tier.md),
  [2026-08-03-memory-pressure-at-compile-safe-points.md](2026-08-03-memory-pressure-at-compile-safe-points.md)

## Context

2026-09-01 bench: a XIAO ESP32-C6 running the Meteor example rendered BLACK at
43 fps for two days while every device log looked healthy. The journal read:

```
allocation failed: requested=252 align=4 free=128 used=299872 context=compute shader node: compile
[RECOVERY] last run crashed (oom): at node:/studio.show/o/…/node:/studio.show/s
[WARN] [visual-shader-node] bound inputs failed to resolve: input using its default:
       produce: tick failed: recovery: node '/studio.show/s' (disabled after 3 crashes)
[WARN] [shader-node] sampling black fallback (frame 1): shader not compiled
```

The compute-shader node OOM'd ~250 B short of the 300,000 B heap at JIT
compile; `lp-recovery` quarantined it after 2 crashes; the downstream visual
shader node then resolved its input to a default and rendered black —
correctly, from a genuinely running program. The device card said "Running"
all day because the heartbeat mirror drops `RecoveryStatus`
(`lpa-link/src/device_link/wire.rs:93-100`).

This is not an isolated bug; it is a recurring shape, four entries in a
month: `docs/defects/2026-08-01-classic-rmt-open-fault.md` (an OOM
misread as an RMT fault), `docs/defects/2026-08-07-boot-compile-oom-crash-loop.md`
(device-side diagnosis that never reaches the client),
`docs/defects/2026-08-29-shader-jit-compile-transient-starves-classic-heap.md`
("ticks at 22.85 fps with black output", three days earlier), and
`docs/defects/2026-08-31-c6-rmt-ws281x-dark.md`'s app half (re-rooted by this
same bench glance, below). The system had no typed signal for "this output is
lying"; `NodeRuntimeStatus` was `{Created, InitError, Ok, Warn, Error,
Unsupported}` and reasons travelled as free-form strings, so "runtime
failure" and "authoring mistake" were the same status (`Error`) with the same
downstream handling: render whatever the graph produces, including nothing.

## Decision

### 1. A third status level: `Fault`

`lp-core/lpc-model/src/node/node_runtime_status.rs` gains
`NodeRuntimeStatus::Fault(String)`, alongside the unchanged `Error` and
`Warn`. The distinction is who owes the fix:

| Status  | Meaning | Examples |
|---|---|---|
| `Error` | The authored configuration is wrong or incomplete; the fix is an **edit**. | unbound fixture input, a mapping document that won't parse, a GLSL compile diagnostic, an output identity collision |
| `Fault` | The authored configuration was **valid** and the runtime failed it; the fix is a retry, a clear-faults, more memory, or a bug report — never an edit. | recovery denies a compile after a crash-history red-gate, a produce/consume tick that returns `Err`, a render/sample trap |
| `Warn`  | Degraded but still rendering. | a shader input resolving to its default because its producer is not runnable |

`Fault` never collapses into `Error` on the wire (`fault_round_trips_as_its_own_variant`)
and is kept out of the shader-diagnostic parser (`ui_shader_error.rs`) so a
recovery message is never read as a GLSL source location. Studio's badge
(`node_controller.rs`) shows the word "Fault" in the `Error` tone family — see
§5.

Accepted imprecision: a render/sample dispatch that returns `Err` is
classified `Fault` even when the underlying cause is really an authoring
mistake that only surfaces at render time (e.g. a missing uniform field).
Separating those needs the backend to classify its own errors, which no
frontend does today; `agent_support::engine_verdict` still passes such
messages through `Fault` (P1 deviation). Recorded here as future work, not a
regression — the bench case (recovery denial) is unaffected.

### 2. Project-level trigger, ≥1 s persistence

Fine-grained propagation (per-fragment, per-dependency-path) was the
starting design (see `notes.md`'s discovery map) and was deliberately cut for
this pass: **any node in `Fault`, anywhere in the project, makes every output
of that project paint the fault pattern**, once the condition has held
continuously for `FAULT_PATTERN_DELAY_SECONDS = 1.0` s, clearing immediately
when it lifts. The engine derives this once per tick (a `(count, newest
revision)` fingerprint, `lpc-engine/src/engine/project_fault.rs`) and exposes
it to both `TickContext`s.

This catches the bench case with no propagation at all: `Engine::
Resolve Host::produce_produced_slot` already sets the FAULTED node's own
status when its tick fails — the visual shader consuming its default stays
`Warn`, but the compute node itself is `Fault`, and that alone trips the
project verdict. Keep-last-good content under a compile `Error` is **not**
overridden, because `Error ≠ Fault` — a mid-typing syntax error in the
studio's live editor still shows the previous good frame until 1 s of actual
`Fault` accrues.

### 3. The pattern: a bypass, not an overlay

Painted in `OutputNode` (`lp-core/lpc-engine/src/nodes/output/output_node.rs`)
— the one seam that reaches both the wall (device outputs) and the screen
(the sim/studio preview reads the same published buffer), and the only place
the composited, per-lamp color order is already known.

- Red raised-cosine breathe, ~1 Hz (`FAULT_BREATH_SECONDS = 1.0`), floor
  `5 * 257` (~2%) to crest `30 * 257` (~12%) pre-pipeline 16-bit levels
  (`fault_level_16`) — visible without reading as an emergency strobe.
- Colour order is resolved per lamp from the published sample layout
  (`ChannelOrders::order_at`, falling back to RGB when no layout has ever
  been established) so red does not come out green on a GRB strand.
- It **overwrites the whole buffer**, not an overlay: `paint_fault_pattern`
  is the one place in this module that deliberately destroys the frame
  underneath, because under `Fault` the content is precisely what lied. A
  trailing partial lamp (not a whole RGB triple) goes dark rather than
  keeping stale content.
- It paints **even when this output's own render failed**. P1 found this the
  hard way: `examples/fault-demo` first went black, not red, because the
  fuel trap propagated out through `render_fragments` and `consume` returned
  `Err` before any paint ran. Fix: `render_graph_frame` is split from the
  fault-pattern application so the pattern paints regardless of the graph
  render's outcome, while the `Err` still propagates so the engine still
  faults the node.
- It paints even under the `test_pattern` bypass — nothing outranks it.

### 4. `FaultPresentation { Pattern, Black }`

An engine knob, default `Pattern`. `Black` turns the whole mechanism off
(the pre-existing behaviour). Persisting the choice (project.json or a
device setting) is explicitly deferred — see Follow-ups.

### 5. Heartbeat, mirror, card

The heartbeat carries the per-project fault additively:
`LoadedProject.fault: Option<ProjectFaultWire>` (`ProjectFaultWire`,
`FaultedNodeWire`, capped at 120 bytes / 8 named nodes) — an
optional `#[serde(default)]` field, so this does **not** bump the wire
version on its own. `lpa-link`'s mirror stops dropping recovery: it now
keeps `RecoveryFacts { level, safe_mode, paths, last_crash }` and
`ProjectFaultFacts` across heartbeats (previously `..`'d away at
`lpa-link/src/device_link/wire.rs:93-100`). `DeviceStatus::Degraded` is a new
refinement of `Ready`; `device_affordance.rs` maps both `Degraded` and
`NeedsAttention` to `UiStatusKind::Attention`, so the roster card shows a
Degraded line under the running face instead of "Running" — the honesty this
plan is named for.

Card tone summary: node badge `Fault` reuses the `Error` tone with the label
"Fault" (no seventh tone; the difference is the affordance, not the color —
see Alternatives). Card `Degraded` uses `Attention`.

### 6. Clear faults

`RecoveryHandle::clear_ledger` (`lp-base/lp-recovery/src/recovery.rs`,
`ledger.rs::clear`) forgets every path entry (yellow/red state) and the
consecutive-incomplete-boot (safe-mode) counter. It deliberately keeps
`last_crash`, `boot_count`, and `generation` — those live outside the ledger
because history is not blame; the next heartbeat still says what the last
crash was.

`ClientRequest::ClearFaults` + `ServerMsgBody::ClearFaults { ledger_cleared }`
bump `WIRE_PROTO_VERSION` 19 → 20 (History entry in
`lp-core/lpc-wire/src/server/hello.rs`; the bump also covers the new
`NodeRuntimeStatus::Fault` variant riding `WireTreeDelta`, since both land in
the same train). The server handler clears the ledger when a recovery global
is installed (`false` on host/browser, which have none) and calls
`Engine::clear_faults` on every loaded project regardless, which re-arms
faulted shader compiles (`ShaderNode::clear_fault`). The card offers "Clear
faults" beside Reset only while `Degraded`.

Unlike Reboot, nothing resets: the cleared state takes effect on the next
tick. If the underlying condition recurs — the Meteor case does,
deterministically — the board crashes, reboots, the ledger re-yellows, two
more crashes re-quarantine, and the card degrades again within a heartbeat.
That is the intended, honest outcome, not a bug to chase.

## Accepted gaps

- A compile `Error` with no last-good program still renders black (status
  `Error`; the card names it). `Error ≠ Fault` by design — see §2.
- A faulting OUTPUT root aborts the remaining demand roots that tick
  (pre-existing `?` in `tick_nodes`, `engine.rs` ~:709) — one broken output
  can still take others dark that frame. Not addressed here.
- An output that has never established an extent (`control_samples` empty)
  has nothing to paint; `apply_fault_pattern` returns `false` for it.
- Render-time authoring errors ("missing uniform field") read as `Fault`
  until a backend classifies its own errors (§1's accepted imprecision).

## Two new examples

`examples/pulse` — the plainest possible shader, one colour breathing on a
phasor. No dependency on this plan; it is the hardware wiring baseline: push
it first and the strip must simply breathe blue, proving the board and
driver before any fault is judged.

`examples/fault-demo` — a shader that compiles cleanly but trips fuel every
frame (`while(true)`; `lpvm-native`'s `NativeOptions.fuel` defaults on, so
this is deterministic on every backend and never crashes a board). It is the
repeatable, non-destructive demonstration of the red pattern, independent of
the Meteor/C6 OOM which needs real hardware pressure to reproduce.

## Alternatives considered

- **Dependency-path fault taint + per-fragment verdict.** The original
  proposal: a typed `NodeFault` propagated along every edge the engine
  mediates, with a verdict computed per output fragment. Rejected for this
  pass as too much new machinery for the size of the plan; the bench case
  does not need it (the faulted node's own status already trips the
  project-wide rule). The discovery map in `notes.md` is the starting point
  if finer propagation is ever built.
- **Recovery-level-only trigger** (fire only on a recovery ledger red-gate).
  Rejected: misses authoring `Error`s that already render black today
  (an unbound fixture, a mapping error) where the honest pattern is
  strictly better than silence, and misses hosts/browsers, which have no
  recovery global at all.
- **Any-`Error` trigger** (fire on any `Error`, not just the new `Fault`).
  Rejected: it would paint the pattern project-wide on every mid-typing GLSL
  syntax error in the studio's live editor, even while keep-last-good is
  happily still rendering the previous good frame — over-alarming, and it
  breaks the D1 principle that genuinely rendering content is never
  overridden by an authoring mistake.
- **A seventh UI tone for `Fault`.** Rejected: reuses the existing `Error`
  tone with the label "Fault"; the difference the UI needs to carry is the
  affordance ("not an edit"), not a new colour, and a new tone would also
  need its own diagnostic-parser exclusion for no real benefit.
- **Names `Panic`, `Crash`, `Tripped`** for the new variant. Rejected in
  favor of `Fault` (D3, `notes.md`) — it reads correctly for a quarantine, a
  denied compile, or a render trap alike, without implying the process
  itself crashed.

## Follow-ups

- Persist the black-on-fault preference (`project.json` or a device
  setting; home undecided — either needs a format bump).
- Dependency-path fault taint and per-fragment verdicts (deferred scope
  from §2; `notes.md`'s discovery map is the starting point).
- Meteor heap headroom on the C6 — the compute-shader compile is ~250 B
  over the 300,000 B arena; a separate item, not addressed by this plan
  (the hardware walk still reproduces the quarantine because Meteor still
  OOMs at compile).
- Flash-persisted recovery ledger — deferred by
  [2026-07-04-crash-recovery-model.md](2026-07-04-crash-recovery-model.md)
  and still deferred; "Clear faults" makes the in-RAM ledger's quarantine
  liftable without reaching the USB cable, which was that ADR's "richer
  Studio recovery UI" follow-up, now partly done.
