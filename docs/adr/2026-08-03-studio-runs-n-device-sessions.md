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

**Interim attribution (until M5).** The connect FLOW remains
app-singular: its narration rides the OLDEST device session's evidence
entry, or a session-less entry ("evidence of work, not of a session")
while nothing is attached. M5 makes it endpoint-targeted.

## Amendment 2026-08-03 — how an operation names its board (M4)

**One target vocabulary: the CARD KEY.** `DeviceTarget::Card(String)`
carries `UiDeviceCard::identity_key()` — a stamped board's `dev_…` uid or
an anonymous board's session key — and one resolver handles both, because
`identity_key` puts the uid first, so a live stamped card's key IS its
uid. This collapses the two bespoke vocabularies that were doing one job
(`DisconnectDevice { session_key }`, `ProbeBootloaderMode { card_key }`).

**An unresolvable target refuses; it never falls back.** Resolving to
"the" device when the named card is gone is exactly how an operation
reaches a board nobody named, and the worst outcome available here —
flashing the wrong board — is silent when it happens. The refusal names
the card.

**`DeviceTarget::Ambient` is a closed set of one.** Only
`ConnectLightPlayer` uses it, for its documented lens fallback (the sim's
reconnect). Every op added to that arm re-opens the hole this closes. The
console's device log-level selector looks like a member and is not: it
targets the LENS session directly — whichever runtime's console the user
is reading, sim included — so it carries no target at all.

**The card-owned op flow is keyed by `RuntimeId`, not by the card key.**
A first-provision flash STAMPS a uid mid-op, which moves that card's
`identity_key()` from its session key to the new uid — an op keyed by the
card key would lose its card at the instant the flash succeeded.
`UiDeviceCard.session_key` is set on every live card and does not move.
`op_in_flight` is per-session evidence for the same reason: it pins its
card's EXISTENCE through a `Gone` link, and pinning the wrong card is how
the 2026-07-31 "card vanished mid-op" regression comes back.

**The op flow migrates across a same-endpoint replace.** The replug that
ENDS a recovery write brings the board back as a NEW session, so the
"unplug the board and plug it back in" instruction would vanish at the
moment the user obeyed it. The ENDPOINT is a physical board's continuity
across a replug; the `RuntimeId` is not.

**Surfaces where the target had to travel rather than be looked up.**
`UiCardConnection` gained `device_key`: the project card's "Push to
<device>" / "Put on <device>" rows are the one place a device verb is
dispatched from a card that is not the device's, and they carried only a
display NAME, which identifies nothing with two boards. The roster
HEADER's "Flash firmware…" has no card at all — it acts directly only
when exactly ONE board is live, and otherwise opens the recovery chooser,
which asks.

## Consequences

- Two unprovisioned boards render as two distinct, chip-titled cards; the
  second is no longer silently dropped.
- *(M4, 2026-08-03)* Device operations act on the board whose card was
  clicked; an op aimed at a card with no live session refuses by name.
  `takes_card_op` is an exact session match, so one flash narrates on one
  card. A latent wrong-board WRITE closed with it:
  `write_back_live_identity_name` resolved "the" device and then checked
  the uid matched, writing a rename to the wrong board's
  `/.lp/device.json` whenever the renamed one was not the oldest attached.
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

- ~~M4: session-targeted ops~~ — done, amended above (2026-08-03).
- M5: endpoint-targeted connect flow (L1 requestPort dedupe, L2 explicit
  endpoint, L3 per-endpoint sweep guard); revisit empty-ending teardown.
  It also inherits the last app-singular reads, which M4 deliberately left
  alone and renamed `oldest_device_session()` so they stay greppable: the
  sweep guard, the recovery open, and the route-reload path — all of which
  run BEFORE a session exists, so they have no id to name.
- Move the console's device log-level selector onto the card's Console tab
  (D1b, deferred out of M4 to keep it off UI work): one board, one level,
  and `DeviceTarget::Ambient` loses its last non-lens justification.
- M9: virtual device behind a Web Serial shim, validated against the M8
  golden-trace library.
