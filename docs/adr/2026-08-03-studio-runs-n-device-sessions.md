# ADR: Studio runs N device sessions

- **Status:** Accepted
- **Date:** 2026-08-03
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Studio was unusable with two ESP32 boards attached (found on the 2026-08-02
PR #293 hardware walk, a C6 and an S3 both plugged in). The runtime pool was
deliberately built plural (`BTreeMap<RuntimeId, RuntimeSession>`; the
2026-07-24 runtime-pool ADR: "capacity is a policy, never a shape"), and the
roster view already rendered N cards — but `DEVICE_SESSION_CAPACITY` was 1,
and the singular assumption had soaked into the consumers: the evidence
build read "the" device session, the op seams took no id, both
granted-connect paths took `endpoints[0]`, and an anonymous card's render
key was its display NAME, so two unprovisioned boards (both "Connected
device") collided and the second card was silently dropped by
`dedupe_by_key`. The UI promised multi-device; the model refused; the
mismatch cost unrecognized friction long before the walk made it
undeniable.

The alternative — keep ≤1 and make the UI stop lying — was considered and
rejected by Yona (2026-08-02, decision D1): "we need to make it actually
multi device."

## Decision

Studio models, renders, and operates N attached device sessions.

**Capacity: `DEVICE_SESSION_CAPACITY = 4`** — an explicit small bound.
Enough for a real desk (three boards on Yona's), small enough that the
oldest-first eviction path stays exercised rather than becoming dead code,
and a wall against runaway session minting (a hotplug storm must hit a
bound, not grow the pool forever). Raise the number when a desk outgrows
it; never the shape. Sim capacity stays 1.

**One session per endpoint.** `RuntimePool::install` replaces an existing
same-kind session holding the SAME endpoint id (refused while an op is in
flight on it — the DQ-A record), so reconnecting a port replaces its own
old session rather than minting a sibling card for the same physical
board. Sessions without a link record (test stubs) never match.

**Empty connect endings stop clearing the device slot.** The ≤1-era
"failed/cancelled/opened connect clears the kind's slot" semantics
survive only for the sim: with several boards attachable, opening the
picker or failing a connect to an ADDITIONAL board must not tear down a
live session (or yank the editor riding it). The mirror-quiesce pairing
in `clear_connect_slot` remains for the paths that still clear.

**Card identity before a uid is stamped.** `identity_key()` falls back
`uid → session RuntimeId → name`. `uid` stays FIRST — `CardUiState` is
keyed by this and must survive session replaces; a stamped board keying by
its per-session RuntimeId would drop its tab/sheet state on every replace.
The chip-based pre-provision title ("ESP32-C6") is a human affordance and
deliberately never feeds the key: two same-chip boards still key
distinctly by session.

**The lens with N devices attached** keeps the existing rule unchanged:
attaching observes, never steals. The editor is a lens on exactly one
session; installing another board leaves it alone; the lens card renders
the LENS session's evidence entry (matched by session key), not the first.

**Interim attribution (until M4/M5).** The connect FLOW and the single
in-flight card-op slot remain app-singular. Their narration rides the
OLDEST device session's evidence entry — the same session the id-less op
seams (`device_session()`/`device_session_mut()`, still un-targeted)
resolve — or a session-less entry ("evidence of work, not of a session")
while nothing is attached. Attach-time connect-as-pull targets the session
being attached (`refresh_device_sync_for(id)`); the remaining id-less
pulls (stamp, push, reconcile) go through the oldest, consistent with the
ops that trigger them. M4 makes ops session-targeted; M5 makes the
connect flow endpoint-targeted; both retire the id-less seams.

## Consequences

- Two unprovisioned boards render as two distinct, chip-titled cards; the
  second is no longer silently dropped.
- Until M4, device operations (flash/erase/push/reconcile) land on the
  OLDEST attached board regardless of which card was clicked — the reason
  the M4 milestone exists and the roadmap gates before relying on ops
  with two boards attached. The uid-less `takes_card_op` rule is
  ambiguous with several anonymous boards (the op can render on more than
  one card) until M4.
- A hard constraint shapes every future "pick your board" UI: **two
  Espressif boards are indistinguishable pre-connect.** Web Serial's
  `getInfo()` exposes only vendor/product id — both a C6 and an S3
  enumerate as `303a:1001`, and the USB serial number is not exposed, at
  all, ever. The only discriminators are post-connect: `detected_chip`
  from the ROM boot banner and the hello identity. Design within this,
  not around it.
- `remove_kind` removes the OLDEST session of a kind; its remaining
  device-kind callers are teardown paths. M5 decides what a failed
  additional connect should tear down.
- The device lifecycle event log (M0) records pool installs/removals,
  state transitions, connect-flow changes, and per-session parse-anomaly
  counts as JSONL — the trace format is a contract (golden-trace fixtures
  + replay tests). A deeper actor-level `(t, StudioCommand)` journal would
  be genuinely replayable (clock/timers/randomness are injected) and is
  recorded here as future work, not built.

## Alternatives Considered

- **Keep ≤1 and make the UI honest** — rejected (D1): the roster already
  rendered N cards; the constraint was costing real work.
- **Unbounded device capacity** — rejected: an explicit bound keeps the
  eviction path alive and caps runaway minting.
- **Key anonymous cards by detected chip** — rejected: two same-chip
  boards would still collide; the title and the key do different jobs.
- **Per-endpoint dedupe at the JS layer only** — insufficient alone: the
  pool must uphold one-session-per-endpoint regardless of which path
  minted the session (the JS `requestPort` dedupe gap is fixed in M5).

## Follow-ups

- M4: session-targeted ops (retire the id-less seams; per-card op slots).
- M5: endpoint-targeted connect flow (L1 requestPort dedupe, L2 explicit
  endpoint, L3 per-endpoint sweep guard); revisit empty-ending teardown.
- M9: virtual device behind a Web Serial shim, validated against the M8
  golden-trace library.
