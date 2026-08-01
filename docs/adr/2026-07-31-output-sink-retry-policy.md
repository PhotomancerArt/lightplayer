# ADR: Output sinks wait for a hardware change instead of asking every frame

- **Status:** Accepted
- **Date:** 2026-07-31
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `docs/debt/s3-frame-cost-scales-per-fixture.md` (the measurement
  that prompted this; its other half — per-frame dataflow re-resolution — is a
  separate change)

## Context

An output sink whose channel is not open re-attempted `OutputProvider::open`
on every flush, which is every frame. For a sink that *can* open, that is one
attempt and then nothing. For a sink that cannot — a project authored for
`ws281x:rmt:D9` loaded onto a board that has no `D9`, a pin another node holds,
a fifth strip on a four-channel board — it is an attempt per frame, forever.

Each attempt is not cheap. Opening by spec enumerates every endpoint every
registered driver offers; each endpoint is constructed with a formatted spec
string, parsed and validated, and carries a freshly computed status, which is
itself several registry lookups. On `projects/test/quad-strips` under the
emulator (esp32-c6 cycle model), where three of four authored endpoints do not
exist on the virtual board, this was **45.8% of all self cycles** in
`HwRegistry::endpoint_status_for` and **90.2% inclusive** under
`OutputProvider::open` — the frame spent mostly on relearning the same "no",
sixty times a second, plus a `log::warn!` per failed sink per frame.

The obvious framing — "cache endpoint status in the registry" — is the wrong
one. A sink that opened successfully never asks again, so there is no repeated
query to memoize on the success path; the repetition is entirely the retry
loop. And caching status is actively dangerous: a cached `Available` on a pin
that has since been claimed would hand two owners the same hardware. The
question was never "what did status say last time" but "is it worth asking
again at all".

## Decision

**Sinks park on failure, and a hardware generation counter tells them when to
wake.**

Three pieces, one per layer:

1. `HwRegistry::generation() -> u64` changes whenever a claim succeeds or a
   lease is released — the only two events that can turn a refusal into an
   acceptance. Reserved status and capability support come from the manifest,
   which is immutable after construction, so they need no signal. Failed
   claims deliberately do **not** bump: signalling them would let one hopeless
   endpoint wake every parked sink on every attempt, which is the storm this
   ends.

2. `OutputProvider::hardware_generation() -> u64` forwards it. The default is
   `0` — "this hardware never changes" — which is the honest answer for a
   provider that owns no registry: its failures are about the caller's own
   configuration, not about ownership. Registry-backed providers forward;
   wrappers delegate.

3. `EngineServices` records `parked_at_generation` on a sink whose open
   failed, and skips that sink while the generation still matches. The value
   is sampled **before** any open in the flush, so a claim made by this frame's
   own opens cannot park a sink against a number that already includes it.
   Parking is cleared on re-authored config, on provider swap, and on success.

Consequences worth stating:

- A parked sink does not fail its frame. The first failure is logged and
  returned exactly as before; subsequent frames are silent and free. Recovery
  logs one line. This turns per-frame log spam — which on silicon is per-frame
  serial traffic — into two lines per episode.
- Recovery latency is unchanged: a released pin bumps the generation, and the
  next flush retries. Nothing polls, nothing waits out a timer.
- Nothing caches endpoint status. Reserved pins and claim conflicts are still
  computed from live registry state at every open attempt, so the semantics
  that would be dangerous to stale cannot go stale.

## Alternatives considered

**Cache `HwEndpointStatus` in the registry.** Rejected: it optimizes a query
the success path does not make, and a stale `Available` on a claimed pin is a
correctness bug in exchange for no measured win.

**Time-based backoff** (retry a failed sink about once a second). Rejected: it
delays legitimate recovery by an arbitrary constant, keeps re-enumerating
forever for endpoints that will never exist, and puts a timing policy in a
layer that has no clock of its own. It would also still need the generation to
recover promptly, so it is strictly additional machinery.

**Give up on failed sinks permanently.** Rejected: contention is normal and
transient. A sink that lost a race for the RMT channel must light when the
winner releases it, and today it does.

If a transient failure ever appears that no claim or release follows — an RMT
peripheral that fails to initialize and later would not — a slow fallback
retry can be added on top of this without changing the contract.

## Notes

The measured win is emulator-side, because that board really does lack the
authored pins. On the desk ESP32-S3 all four channels open on the first frame,
so steady-state silicon paid little for this and gains little back; its own
flat per-fixture cost is the dataflow resolver, tracked separately in the debt
entry. What silicon does gain is that a *misconfigured* output — the common
authoring mistake — no longer costs a frame's worth of enumeration and a line
of serial output every frame it stays wrong.
