# Switched power rails: metadata plus a provider-owned state machine

- **Status:** accepted (mechanism landed; bench verification of `settle_ms` is
  owed at the dig2go hardware gate)
- **Date:** 2026-08-08
- **Plan:** `2026-08-07-2336-dig2go-board-support` P1/P2
- **Relates to:** `2026-08-04-concurrent-ws281x-flush.md` (the start/wait split
  the drain rides on), `2026-08-05-manifest-soft-limits-are-measured-records.md`
  (the same "metadata on the manifest, not a model entity" shape),
  `2026-07-29-license-provenance-discipline.md`
- **Design notes:** `docs/future/2026-08-06-quinled-board-metadata-prep.md`

## Context

Some boards put the LED supply behind a GPIO. The QuinLED dig2go, probed on the
desk 2026-08-06, cuts LED power entirely with GPIO12: unasserted, the board is
not dim, it is dark, and a first bring-up reads as a driver bug. The Dig-Next-2
has three independently switched fused outputs and the Dig-Quad's Q1R drives a
*user-supplied* external relay board, so this is a family trait, not a dig2go
special case.

Nothing in our hardware model could express it. `HwCapability` is
`gpio-output`, `gpio-input`, `ws281x-output`, `rmt`, `radio` — every one of
them says "this resource can do X". None of them says "assert this or the
outputs are dead."

Three facts constrain any answer:

- **Polarity varies by install.** A solid-state gate and a user-supplied relay
  board can invert relative to each other.
- **Energising is not instantaneous.** Clocking WS281x data into an unpowered
  strip phantom-powers the first controller through its data-pin protection
  diode: garbage output at best, a latched-up pixel at worst.
- **The gate pin may be a boot strap.** The dig2go's GPIO12 is MTDI, the
  flash-voltage strap. High at boot selects 1.8 V VDD_SDIO and the board does
  not come up at all.

## Decision

### The descriptor is metadata, not a capability and not an entity

`HwManifest` carries a list of `HwPowerGate` — `/gpio/N`, `active_level`,
`open_drain`, `settle_ms`, `off_debounce_ms`, `feeds`, `note`. A list from the
start because the Dig-Next-2 needs three, and each entry names the outputs it
feeds (empty = all of them). It never becomes a node, a slot, or anything the
project model can see: a gate is a property of the board a project happens to
be running on, and authoring a project against a specific board's rail would be
the wrong coupling.

Polarity lives here for the same reason the LED envelope does — it is a fact
about a board, and code that assumed either polarity would be wrong on half the
family.

### The state machine belongs to the output provider

`Esp32OutputProvider` owns assert/settle/transmit and debounce/drain/deassert
(`lp-fw/fw-esp32-common/src/output/power_gate.rs`); the chip crate supplies
only a `PowerGatePin` and a monotonic-µs closure. The provider is the one place
that sees both every frame's content and every wire's transmission state, which
is exactly the pair the decision needs. `Esp32OutputProvider::new` is unchanged
and `with_power_gates` is a builder, so the chips with no gated rail — S3, C6 —
construct and behave precisely as before, and an ungated board's write path
pays one `Option` discriminant test.

The trait is *physical*: `set_level(high: bool)`, with the state machine
resolving `active_level`. A pin driver free to invert on its own would make the
descriptor's polarity mean different things on different chips, which is what
putting polarity in metadata was meant to prevent.

### The trigger is an all-black scan, not brightness

The provider cannot see brightness. `write` receives `data: &[u16]` that is
already post-gamma, post-brightness, post-power-limit, so brightness zero
arrives as zeros. An `is_off` flag would have to be plumbed down through the
whole pipeline; it does not need to be. All-black is a strict superset of
intent-off, and the trailing debounce separates them — a shader's transient
black never survives multiple seconds, a genuine off always does.

The cost objection dissolves with an early exit: a lit frame stops at the first
non-zero value, and only a genuinely black frame pays the full walk, which is
the frame whose rail we are about to switch off anyway. The scan reads the
incoming slice rather than the staged frame bytes, so it never touches storage
a transmitter may still be reading.

The residual failure mode is feel, not correctness: content legitimately black
for longer than the debounce cuts the rail, and coming back costs `settle_ms`
and, on a mechanical relay, an audible click. Invisible on the dig2go's
solid-state gate; on a Q1R-driven relay it argues for a longer
`off_debounce_ms`, which is why that constant is per-board metadata.

### The settle is explicit, and the deassert drains first

Both are consequences of the classic ESP32's dual-core pusher.

The settle is a real wait between the assert and the `start`, not an ordering
convention: the RMT refill ISR lives on the APP core and a wave can be queued,
so "we asserted before we painted" proves nothing about what the wire is doing.
The wait runs on the injected clock, which keeps it testable — the phase's
tests advance a `Cell` rather than sleeping — and keeps fw-esp32-common free of
ambient time (`2026-07-06-sans-io-core.md`).

The deassert is evaluated at the frame barrier (`flush`), and it drains the
gate's wires *unconditionally* before dropping the pin — including
`background_tx_safe` outputs, which the barrier deliberately leaves
transmitting. Cutting the supply out from under a transmission is the failure
the whole sequence exists to prevent. The drain is free in the steady state:
the debounce is unexpired until it isn't, and once dropped the rail is down, so
it runs once per off-transition.

### The data line must be low when the rail is down

This is a hardware-safety invariant, not polish: a data line held high into an
unpowered WS281x phantom-powers the first controller through its input
protection diode. On the classic it is already satisfied from two directions —
every wire's pad is parked plain-GPIO solid low at open (`v3_rmt::park_gpio`)
and every RMT TX channel is configured `idle_output_level = Low` — so no extra
parking step exists. It is written down at the deassert wiring anyway, because
a future chip whose mux parks pads high would need one and the absence would
otherwise read as an oversight.

### Off at construction

`PowerGateController::new` drives every pin inactive before any frame is seen,
and the chip-side pin is constructed with the inactive level as its initial
level so it is never momentarily active between the two. On the dig2go the
strap-safe state and the rail-off state happen to agree, but the requirement is
independent of that coincidence: pin-mux defaults must never idle an
MTDI-class gate high.

## Prior art, and what we deliberately did differently

> WLED — MIT License, Copyright (c) 2016 Christian Schwinne
> Read at the pinned MIT commit `44e28f96`; behaviour described, never ported.

WLED solves this problem in `handleIO()` and independently lands on the same
overall shape: immediate turn-on at the off→on edge, a trailing debounce before
turn-off, a check that the strip no longer needs an update, and reversible
polarity plus open-drain as configuration rather than code. That convergence is
useful calibration — it says the shape is the obvious one, not a clever one.

Three deliberate differences:

- **Trigger.** It uses the strip's brightness because it has that value to
  hand. Our provider sits below brightness in the pipeline and does not.
- **Settle.** It has none explicit; its ordering works because the loop is
  single-threaded. Ours cannot borrow that, per above.
- **Debounce length.** Its ~600 ms is tuned for a responsive UI toggle. Ours is
  a power-saving heuristic driven by content, so it defaults to 5 s.

The licensing rules that govern that reading are not incidental: WLED
relicensed to EUPL-1.2, which is reciprocal and therefore incompatible with the
commercial half of our dual licence. Only the pinned MIT snapshot may be read,
behaviour is described in our own words, the attribution line above travels
with the description, and no post-relicence source enters this tree — it would
go in the EUPL-1.2 modpack, never in core. See
`docs/future/2026-08-06-quinled-board-metadata-prep.md` § "Reading WLED
safely".

## Consequences

- Boards that declare no gate are unchanged, byte for byte, on every chip. The
  provider's existing transmission-contract tests were not touched and still
  pass.
- `feeds` is matched against the **endpoint's own address**, the only address
  the provider can resolve for an open channel. Which `/rmt/ws281xK` slot
  carries a wire is decided per transmission on the classic, so a slot address
  is not a stable identity to scope a rail by. Profiles that scope a gate must
  name endpoint addresses.
- Frames continue to transmit while the rail is down. They are black by
  definition (a lit one re-energises first), the pad rests low, and suppressing
  them would add a second reason for a wire to go quiet.
- `settle_ms` and `off_debounce_ms` are per-board constants with no measured
  provenance yet. The `note` field exists to carry that provenance; filling it
  for the dig2go is owed at the hardware gate.
- A gate GPIO's exclusivity rests on the profile reserving that resource rather
  than on a registry lease. Nothing enforces it today; a profile that both
  declares a gate pin and offers it as a wire would be accepted.

## Alternatives considered

- **A new `HwCapability`.** Rejected: capabilities describe what a resource can
  do, and every consumer of the capability set would have to learn that this
  one variant means something categorically different.
- **An `is_off` flag plumbed from the brightness control to the provider.**
  Rejected: it threads product state through four layers to tell the provider
  something the data already says, and it would still need the debounce.
- **Letting the chip-side pin driver own polarity.** Rejected: it makes the
  descriptor's `active_level` unverifiable from the shared code and untestable
  without a chip.
- **Deasserting from `write` rather than `flush`.** Rejected: `write` sees one
  channel, and the invariant is frame-wide.
