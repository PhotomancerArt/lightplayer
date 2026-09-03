---
status: fixed
found: 2026-09-01      # how: hardware-walk (XIAO ESP32-C6, Meteor)
fixed: this change
area: lpc-engine node status + output fallback; lpa-devices heartbeat mirror
class: state-conflation
related:
  - 2026-08-01-classic-rmt-open-fault.md
  - 2026-08-07-boot-compile-oom-crash-loop.md
  - 2026-08-29-shader-jit-compile-transient-starves-classic-heap.md
  - 2026-08-31-c6-rmt-ws281x-dark.md
  - ../adr/2026-09-02-fault-is-never-black.md
  - 2026-09-01-2026-fault-is-never-black
---
# A recovery-quarantined node renders black with a card that still says Running

**Symptom** — 2026-09-01 evening bench glance, XIAO ESP32-C6 (`fw-esp32c6`
c3514826e056) running the Meteor example. The strip had been dark for two
days; the device card read "Running" the whole time. The sink journal (same
board, same firmware) showed:

```
allocation failed: requested=252 align=4 free=128 used=299872 context=compute shader node: compile
[RECOVERY] last run crashed (oom): at node:/studio.show/o/…/node:/studio.show/s
[WARN] [visual-shader-node] bound inputs failed to resolve: input using its default:
       produce: tick failed: recovery: node '/studio.show/s' (disabled after 3 crashes)
[WARN] [shader-node] sampling black fallback (frame 1): shader not compiled
```

The engine ran a steady 43 fps the entire time; the RMT driver on gpio18 was
genuinely open and genuinely sending frames. Every one of them was black.

**Root cause** — Two independent conflations, one per layer:

1. **Engine:** the compute-shader node's JIT compile OOM'd ~250 B short of
   the 300,000 B heap. `lp-recovery`'s ledger red-gated it after 2 crashes
   (`ledger.rs:141-148`, `check_enter` deny). The downstream visual shader
   node's input-resolve then failed ("node disabled after 3 crashes"), which
   the engine modeled as `input using its default` — the SAME code path used
   for "nothing bound yet, authored intentionally empty." The visual node
   rendered its default input correctly and completely: an honestly running
   program producing black, because the thing feeding it had been silently
   switched off. `NodeRuntimeStatus` had no way to say "the runtime, not the
   author, broke this" — `Error` covered both a missing binding and a
   crash-history quarantine, so nothing downstream could tell them apart.
   Deterministic across every non-power-on reset: the OOM recurs the very
   next boot, so quarantine re-arms within 2 boots every time.
2. **Device model:** `lpa-link`'s heartbeat mirror
   (`device_link/wire.rs:93-100`) mapped the wire's `Heartbeat` into the
   roster's view by destructuring and discarding `recovery`, `memory`,
   `outputs`, and `link` (`..`). The card's status pill derives only from
   "identity received, project loaded" — it modeled "a heartbeat is
   arriving" as "the device is healthy," so a board stuck rendering black
   under a live quarantine looked identical to one running its show
   correctly.

Both are the same shape: one state variable stood in for two facts that had
started to diverge (`Error` for "edit me" vs. "runtime broke it"; "Running"
for "heartbeat present" vs. "content is real").

**Why it went unseen for two days** — Every reset available from the bench
(software, board button, replug/power-on) looked like it should have been
diagnostic, and each pointed investigation the wrong way. The board's
previous firmware build (b16f944a1, 2026-08-31) happened to sit ~250 B on
the *fitting* side of the same compile, so a physical-reset boot on that
build genuinely came up lit — which read as confirmation of an unrelated
BOOT-strap theory (see `docs/defects/2026-08-31-c6-rmt-ws281x-dark.md`,
corrected alongside this entry) rather than as a coincidence of build size.
The current firmware (c3514826e, wedge fix #491 plus build noise) no longer
fits, so it OOMs on every boot regardless of reset flavor — but by then the
card had already been read as "running" all day, and nothing on the wire
said otherwise. The RECOVERY subsystem logged the correct diagnosis, in
full, to serial, on every boot; none of it reached the client.

**Fix** — This change (plan `lp2025/2026-09-01-2026-fault-is-never-black`,
ADR `docs/adr/2026-09-02-fault-is-never-black.md`):

- A new `NodeRuntimeStatus::Fault(String)` separates "runtime broke a valid
  configuration" from `Error`'s "the configuration needs an edit"
  (`lpc-model/src/node/node_runtime_status.rs`). The compute node's
  recovery-denied compile and any produce/consume tick failure now set
  `Fault`, not `Error`.
- Any node in `Fault`, continuous for ≥1 s, makes every output of the
  project paint a red raised-cosine breathe pattern instead of whatever the
  graph produced — painted in `OutputNode` so the wall and the sim preview
  agree, and painted even when the output's own render trapped
  (`lpc-engine/src/nodes/output/output_node.rs`).
- The heartbeat mirror stops dropping recovery and fault facts
  (`lpa-link/src/device_link/wire.rs`); the roster card gains a `Degraded`
  status distinct from `Running` (`lpa-devices/src/device.rs`,
  `lpa-studio-core/src/app/devices/device_affordance.rs`) naming the
  quarantine reason.
- A "Clear faults" verb (`Action::ClearFaults`, wire `ClientRequest::
  ClearFaults` / `ServerMsgBody::ClearFaults`, `WIRE_PROTO_VERSION` 19 → 20)
  forgets the crash ledger and re-arms the quarantined node without a full
  power cycle; if the underlying cause recurs the card degrades again
  honestly within a heartbeat.

**Regression coverage** — Engine: `denied_shader_compile_is_a_fault`
(`lpc-engine/tests/recovery_gating.rs`), the project-fault derivation suite
(`lpc-engine/src/engine/project_fault_tests.rs`, 7 tests), 9 output-node
pattern-paint tests, `fault_round_trips_as_its_own_variant`
(`node_runtime_status.rs`), and `fault_demo_paints_the_pattern.rs`
(`lpc-engine/tests/`, loads `examples/fault-demo` end to end: shader status
names the fuel fault, project fault is `Some`, the published output buffer
is the red pattern). Heartbeat/mirror: `heartbeat_project_fault.rs`
(`lpa-server/tests/`, loads `examples/fault-demo` and `examples/pulse`), 4
`lpc-wire` additive-field/round-trip tests, 2 `lpa-link` tests, and
`lpa-devices/tests/degraded.rs` (5 tests) for the mirror fold and
`DeviceStatus::Degraded` projection. Hardware: the C6 + Meteor
re-quarantine walk at this plan's G1 gate.

G1 bench (2026-09-02) added the wall-level pin the engine-level tests could not
be: `lp-app/lpa-server/tests/fault_pattern_reaches_the_wall.rs` reads the bytes
the OUTPUT PROVIDER was handed — the first walk showed `fault-demo` red in the
sim and dark on the C6, because `Engine::tick` returned on the failed walk before
flushing the sinks. The sim reads the published buffer, the wall reads the flush;
a fault pattern must be proven at the flush.

**Lesson** — A status enum or a mirror struct that conflates "this thing
failed" with "this thing is fine but empty" (or "a heartbeat arrived" with
"the content is real") will eventually present a genuinely-running, honestly-
executing program as healthy while it silently renders nothing worth
watching. The fix is never a better fallback value — it is a typed
distinction the two states share no representation for, propagated all the
way to the surface a human actually looks at. Four defects in a month
(`2026-08-01-classic-rmt-open-fault`, `2026-08-07-boot-compile-oom-crash-loop`,
`2026-08-29-shader-jit-compile-transient-starves-classic-heap`, and this
one's sibling `2026-08-31-c6-rmt-ws281x-dark`) share this exact shape —
OOM at a compile safe point reads as something else entirely because nothing
between the recovery ledger and the eyes looking at the board carries the
word "fault."
