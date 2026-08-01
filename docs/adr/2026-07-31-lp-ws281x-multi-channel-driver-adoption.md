# ADR: lp2025 adopts `lp-ws281x` as its multi-channel WS281x driver core; C6 migration deferred

- **Status:** Accepted
- **Date:** 2026-07-31
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`fw-esp32s3` needed real WS281x output on 4 concurrent RMT channels,
replacing the `SerialReadoutWs281xDriver` stand-in
(`2026-07-31-0720-s3-led-output-4ch`, P3). The existing driver in this repo —
`fw-esp32c6/src/output/rmt_ws281x_driver.rs` / `LedChannel` — is
single-channel only, tracks its transmit state in `static mut` singletons,
and hardcodes its channel count into driver logic. Extending it in place for
4 channels meant fixing all three at once, inside the crate that also has to
keep working, unmodified, for the C6's own tight flash budget.

A hardware-validated alternative already existed outside this repo:
`lp-ws281x`, built and proven on an ESP32-S3 in the
`2026-esp32s3-experiment` repo — a chip-agnostic core (`no_std`, no chip
register names, an `RmtHw` trait a backend implements) with per-channel
state, a bit-cursor refill that works for any half-size (not just a whole
number of LEDs per half), and a guard-word design that fixes the C6 driver's
known start-of-frame race. It was imported unmodified into
`lp-fw/lp-ws281x` (P2) and is what the S3 backend (P3) implements `RmtHw`
against.

## Decision

1. **`lp-ws281x` is the multi-channel WS281x driver core going forward.**
   New chip backends (S3 today; classic ESP32 and, eventually, the C6)
   implement its `RmtHw` trait rather than growing a second bespoke
   register-poking driver. The portable sequencing (refill, bit cursor,
   guard word, telemetry) is tested exhaustively on the host once and reused
   unchanged across chips.
2. **The existing `lpc-hardware` `Ws281xDriver`/`Ws281xOutput` trait seam is
   preserved, not replaced.** The S3 backend
   (`Esp32S3RmtWs281xDriver`) implements those traits exactly as the C6's
   driver does; `lp-ws281x` sits *under* that seam as the chip-backend's own
   internal implementation detail. Callers above the seam (the engine's
   `OutputProvider`) do not know or care which driver core answers a given
   endpoint.
3. **Channel count is sourced only from the board manifest** — one
   `HwEndpointSpec` per declared `/rmt/ws281xK` resource — never a literal in
   driver logic. The C6 exposes 1 today, the S3 exposes 4; a future classic
   ESP32 backend can expose up to 8 by adding manifest entries and an
   `RmtHw` instance per channel, with no driver-logic change.
4. **Per-channel state lives in the driver instance, not process-wide
   statics.** The C6 driver's `static mut LED_CHANNEL` / `LED_GPIO` /
   `CURRENT_TRANSACTION` pattern is a known wart (documented inline in that
   file) that this decision does not propagate to new backends.
5. **The C6 stays on its own legacy driver for now.** Migrating it onto
   `lp-ws281x` is real, separately-scoped work (chip-specific `RmtHw` backend,
   a size check against the C6's tight partition budget) and was out of
   scope for the S3 bring-up plan. Tracked as debt:
   `docs/debt/c6-on-legacy-ws281x-driver.md`.

## Consequences

- Two WS281x driver implementations now coexist in the tree
  (`fw-esp32c6`'s legacy driver and `lp-ws281x`) until the C6 migrates. Bug
  fixes to the driver *behavior* (e.g. the guard-word race) do not
  automatically reach the C6; see the debt entry.
- `MAX_LEDS = 256` per-channel truncation is duplicated across both driver
  paths and neither logs when it caps — a pre-existing condition this
  decision does not fix, tracked separately:
  `docs/debt/output-channel-led-cap-silent-truncation.md`.
- Any future chip backend (classic ESP32) has a settled shape to follow:
  implement `RmtHw`, wrap it in a `Ws281xDriver`/`Ws281xOutput` adapter,
  declare channels in the board manifest. No new trait surface should be
  invented for a third chip.
- `lp-ws281x`'s host test suite (`cargo test -p lp-ws281x`) is now the
  primary regression net for WS281x sequencing bugs across every chip that
  adopts it, ahead of any chip-specific hardware test.

## Alternatives Considered

- **Extend the C6's driver in place for 4 channels.** Rejected: would have
  required fixing the static-singleton state, the hardcoded channel count,
  and the start-of-frame guard race simultaneously, inside a crate whose
  flash budget is already at ~99.7% of its partition
  (`docs/defects/2026-07-28-esp32c6-app-partition-overflow.md`) — the worst
  place to absorb that risk.
- **Write a new S3-specific multi-channel driver from scratch**, ignoring
  the experiment repo's work. Rejected: would have re-derived
  hardware-validated timing and refill logic the experiment repo had
  already proven on this exact chip, for no benefit.
- **Migrate the C6 onto `lp-ws281x` in the same plan.** Rejected (scope): the
  S3 bring-up plan's own acceptance criteria excluded it explicitly, to keep
  the C6 — the reference board every other firmware change is validated
  against — untouched while a different chip's driver landed.

## Follow-ups

- C6 migration to `lp-ws281x` — `docs/debt/c6-on-legacy-ws281x-driver.md`.
  Revisit when: a second C6 channel is wanted, a `lp-ws281x` fix needs to
  reach the C6, or maintaining two drivers becomes its own tax.
- `MAX_LEDS` silent truncation and duplication —
  `docs/debt/output-channel-led-cap-silent-truncation.md`.
