# ADR: The device model is an event fold in its own crate

- **Status:** Accepted
- **Date:** 2026-08-25
- **Deciders:** Photomancer
- **Supersedes:** None (partially supersedes the device-across-time half of
  `2026-07-15-device-session-model.md`, which stays authoritative for the
  transport/session layer)
- **Superseded by:** None

## Context

Plugging in a device is the product's moment of truth, and it has never
been successfully demo'd. A two-part discovery pass
(`~/.photomancer/planning/lp2025/2026-08-24-1612-device-serial-reliability/discovery-client-device-model.md`
and `…/discovery-firmware-io.md`, both line-cited against merge
`2c914c437`) established that the failure is architectural, not local:

- **Four state machines and five auxiliary stores, four key types, no
  shared lifetime.** `lpa_link::DeviceState`, the app-singular
  `ConnectFlowState`, the wizard's `SetupState`, and the derived
  `RosterCardState`; plus `RuntimeSession.server_state`,
  `device_card_ops` (keyed by `RuntimeId`, deliberately outliving it and
  unreachable after disconnect), `card_ui`, `DeviceSyncState`, and the
  on-disk `DeviceRegistry`. Every recurring bug family — two cards for one
  device, the vanished Danger tab, stale verdicts, the orphaned
  "incomplete flash" message — is an identity-join or lifetime failure
  *between* those stores.
- **Zero foreground cancellation.** Every device op is `Recovery`-class
  with no deadline, holding `&mut self` on the single Studio actor, so a
  hung connect blocks the very Disconnect gesture that would fix it. The
  only escape was a page refresh.
- **Escapes vanish in exactly the stuck states.** `OperationInFlight` had
  no danger section at all; `ConnectingRetrying` offered no disconnect; an
  anonymous board could never be forgotten (forget required a uid).
- **Verdicts latch.** `DeviceState`'s terminal states are sticky under
  passive observation, so a device that was blank a minute ago still reads
  blank after it has been flashed and rebooted, until a rebuild flow runs.

The projection half of the right architecture already existed
(`RosterCardState` is derived, not stored) — but it projected from the
accretion rather than from a model. The 2026-07 vision ("lpa-link holds
the device connection model, UI based directly off it") half-happened: the
link/session half shipped, the device-across-time half never did.

Yona ruled: no remorse, tear it down and rebuild. The vision was ratified
2026-08-25 with an explicit **anti-fifth-machine rule** — every
implementing PR must delete more state machines than it adds — because
past attempts added narration and projection layers without deleting
owners.

## Decision

The device-across-time model is a **new UI-free, IO-free crate,
`lp-app/lpa-devices`**, built as an event fold with a dependency-inverted
transport contract. Five concepts, one word each:

```text
Roster ──── owns ────► links (dumb transports) + the router
   │                   DeviceRecords (persisted identity + prefs)
   │                   Journal (flight recorder, both streams)
   └── owns ────► Device (one per known device)
                     │  intent      (prescriptive user state)
                     │  evidence    (incremental fold of events)
                     │  link        (routed by the Roster)
                     └─ activity    (Option — supervised reducer)
                            projection: view DTO = f(intent, evidence, activity)
```

### 1. One entry point, two arms with different rights

`Roster::handle(now, input) -> Vec<Command>`.

- `Input::Action` (user gesture) is journaled, may write `intent`, may
  spawn or cancel an activity, and may emit commands. It may **never**
  write evidence.
- `Input::Event` (the world) is journaled, folded into evidence, and
  forwarded to the running activity. It may **never** write intent.

Actions are *stateful intent*, not events to re-derive: a fully
event-sourced design was considered and rejected, because "stay connected"
is a standing instruction, not an observation.

### 2. Intent vs evidence, and the fold discipline

Evidence is written **only** inside `Evidence::fold`. That single rule is
the anti-fifth-machine mechanism in the small: a new fact enters as an
event or it does not enter, so nobody can grow a `bool` beside the fold
and create a sixth store. It is enforced by a test
(`actions_never_touch_evidence`) rather than by review alone.

Two properties follow structurally:

- **Verdicts are non-sticky.** `Classification` is not a transition
  target; it is recomputed from the current observation window on every
  fold. Opening a port, a successful reset, and a detach all clear the
  window, so the model *reacts* to reboots and replugs instead of
  latching. A device that is not attached is classified as nothing at all.
- **Freshness carries samples, not booleans.** Heartbeats update
  `last_heard`; only the went-quiet / came-back *transitions* are
  journaled, behind a hysteresis window wide enough that a lossy wire
  cannot flap the timeline. Staleness renders honestly ("last heard 12 s
  ago") instead of hanging or lying.

### 3. Identity is a chain of bindings

Port endpoint → chip MAC → provisioned uid → provisioned name, each
learned from evidence and each revocable, with `DeviceId` as a stable
app-side handle that is deliberately **not** the uid (an anonymous board
still needs an entry to render, name and forget). Promotion and **merge**
are first-class journaled operations, not accidents of map keys, and
routing is revisable: a link that reveals a different identity than
assumed is re-routed and the correction is journaled.

Pending links are **roster state, not devices**: a new link shows a
roster-level "new device found, identifying…" affordance with three exits
— identity matches a record (route, or merge), identity is a stranger
(create), or the user acts on a still-anonymous link (a blank chip may
never identify itself, so user action must be a creation trigger).

### 4. Activities are supervised sans-IO reducers

Existence is imperative, state is reduced: a gesture spawns the activity,
the spawn and end are journaled brackets the fold consumes — so "busy with
X" participates in derived state without a parallel store (the
`device_card_ops` disease made unrepresentable) — and between the brackets
the activity moves only by forwarded inputs. At most one per device.

Cancellation is **supervision**: cancel is requested, the activity gets a
bounded grace to wind down, and then it is *evicted* — the device journals
the eviction, emits link-rebuild commands, and re-derives from fresh
evidence. Because no `.await` holds controller state, eviction is safe.

M1 ships one activity, `Identify`. It mirrors the shipped hello-gate
semantics (`lpa-link/src/device_session/device_readiness.rs`) — readiness
is granted only by a proto-matching hello, ours or the boot one; non-hello
frames are absorbed as live-peer evidence and the *deadline* decides
frames-but-no-hello; boot lines are diagnosis — minus the sticky verdict.

### 5. Dependency-inverted transport contract

`Link`, `LinkEvent`, `LinkCommand` and `ResetKind` are defined in this
crate; `lpa-link` implements them (browser Web Serial, host serial, the M9
fake, eventually the sim). The model never calls a transport, and a
transport never classifies a device. `wire.rs` is a deliberate **minimal
mirror** of the four wire facts the fold reads — reusing `lpc-wire` would
drag `lpc-model` (the whole project/slot/tree model) in for one type — and
its header pins the M3 adapter mapping. `WIRE_PROTO_VERSION` is *supplied*
via `RosterConfig::expected_proto`; the model hardcodes no proto number.

### 6. Sans-IO, with caller-supplied integer time

No `tokio`, `embassy`, `wasm-bindgen`, `dioxus`, or futures executor —
`serde` and `serde_json` are the only dependencies, and the crate compiles
for `wasm32-unknown-unknown` as well as the host. Time is caller-supplied
epoch **milliseconds** (integers, not the repo's usual f64 epoch seconds,
because every stored instant is asserted on in replay fixtures and
journals must be byte-reproducible). Waiting is `Command::StartTimer` out,
`Event::TimerFired` in; each scope keeps one generation-stamped timer, so
a superseded fire is dropped and the vocabulary needs no `CancelTimer`.

### 7. The journal is a flight recorder, not a source of truth

It records both streams interleaved, plus derived notes (identity
promotions, quiet transitions, activity brackets, merges, reroutes), and
prunes as a ring. Derivation reads none of it. Two things do: forensics
and tests. `Journal::replay_inputs` hands back the recorded inputs with
derived notes filtered out, and replaying them through a fresh roster must
reproduce the same journal and the same projection — which is the M1
determinism test.

### 8. The projection is total and escapable

`view::roster_view` is a pure function of (intent, evidence, activity).
Two property tests replace hand-audited match arms: every reachable state
renders something honest, and every card carries at least one escape.
`Forget` is defined at the model level, so it cannot be conditioned away
— including for an anonymous board mid-activity.

## Consequences

- The device layer gets a single owner with a testable surface: ~600
  enumerated states in the property test, twelve replayed scenarios
  including the shipped hello-gate defect and the wedged-cancel case that
  used to require a page refresh.
- Invariants I1–I8 from the vision become structural rather than
  aspirational: no activity without a deadline and a cancel path, every
  cancel bounded by eviction, every state escapable, outcomes surviving
  disconnect, busy devices rejecting gestures visibly, facts entering only
  as events, and the UI never blocking on device IO.
- **M1 adds a model; the deletions land in M2/M3.** The anti-fifth-machine
  rule is not satisfied by this crate alone — it is satisfied by the
  teardown PR that deletes `ConnectFlowState`, `device_card_ops` and
  `RuntimeSession.server_state`/`operation`, and by M3's wire-up. Until
  then the repo carries both, which is a known and time-boxed cost.
- A pending link internally carries a provisional `Device` so the crate
  has exactly one fold and one supervision path. It is absent from the
  device list and never projects as a device card; the alternative
  (a parallel pending-state machine) is the disease itself.
- `wire.rs` is a second vocabulary that must be reconciled with `lpc-wire`
  in M3. That is a real, bounded cost paid to keep the model dependency-
  light and the inversion honest.
- Round-2 activities (Setup, Flash, Provision, Push, Pull) are new enum
  variants, not new architecture. The wizard becomes the projection of a
  Setup activity plus the user's choices.
- The crate boundary keeps `lp-cli` adoption possible without forcing it
  in round one.

## Alternatives Considered

**Keep patching the four-machine system.** Rejected on evidence: the
discovery maps showed every recurring bug family living in the *joins*
between stores, not inside any one of them, so each fix moves the failure
rather than removing it. Three fix rounds (D7 grant ladder, M6 retry
ladder, the hello-gate fresh-boot fix) each held while the next join
failed.

**One flat state enum per device.** Rejected: the shipped `DeviceState`
already is one, and its terminal states are exactly the sticky-verdict
bug. A flat enum also cannot represent "identified as blank, and also
mid-flash, and also last heard 12 s ago" without a combinatorial
explosion, which is what pushed facts out into the auxiliary stores in the
first place. Splitting into intent + evidence + activity is what makes
each part small enough to be total.

**A module inside `lpa-link`.** Rejected: `lpa-link` is the transport
edge, and putting the model there is what produced the
`DeviceSession`-classifies-devices coupling being removed. It would also
force the model to depend on Web Serial and `esptool-js` build
configuration.

**A module inside `lpa-studio-core`.** Rejected: it closes the door on
`lp-cli` and on host tests that do not want Dioxus, and studio-core is
where the accretion happened — the crate boundary is what makes "no
executor, no UI" checkable by `cargo check --target wasm32-unknown-unknown`
instead of by review.

**Fully event-sourced (actions as events, everything re-derived).**
Rejected by ruling: a user's standing instruction is not an observation,
and re-deriving intent from a prunable log means pruning can change what
the user asked for.

**`Box<dyn ActivityReducer>` for activities.** Rejected in favor of a
closed `Reducer` enum, which keeps the whole roster `Clone + PartialEq +
Serialize` — that is what lets a fixture assert on a model snapshot and a
journal be replayed.

## Follow-ups

- M2: the teardown PR (delete the device half of studio-core and the
  device UI surfaces down to a clean stub, tagged reference commit).
- M3: wire-up — `lpa-link` implements the `Link` contract, the
  `lpc-wire` ↔ `wire.rs` adapter lands, records and journal get their
  persistence, and the four shipped machines are deleted. That PR is where
  the deletion test (I8) is actually checked.
- Round 2: Setup / Flash / Provision / Push / Pull activities; the wizard
  as a Setup projection; the post-flash reset ladder as effects (vision
  R5).
- Vision R4 (firmware stamps identity into the heartbeat as well as the
  hello) is already modeled: `ServerFrameBody::Heartbeat` carries an
  optional `PeerIdentity`, so a mid-stream attach resolves identity
  passively within one heartbeat period once the firmware sends it.
